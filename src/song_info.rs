use gtk::gdk_pixbuf::{Colorspace, InterpType, Pixbuf, PixbufLoader};
use gtk::glib::Bytes;
use gtk::prelude::*;
use mpd::{Client, State};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use walkdir::WalkDir;

use crate::{
    color::{average_color, Rgb},
    config::MpdConfig,
};

// -- Declared response times for cover art ------------------------------------
//
// Cover art is fetched on its own thread, so the overlay never waits for it: it shows up straight away with the placeholder and the art fades in when ready.
// These limits only bound how long that background lookup may take.

/// How long to wait for MPD to accept the cover-art connection.
const COVER_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// Longest any single read or write on that connection may block.
const COVER_IO_TIMEOUT: Duration = Duration::from_millis(1500);
/// Hard limit for finding one cover (MPD albumart, embedded picture, folder file).
/// When it runs out the overlay simply keeps the placeholder for that song.
const COVER_BUDGET: Duration = Duration::from_secs(4);
/// Lookups slower than this are logged, so a slow server or library is easy to spot.
const SLOW_COVER_LOG: Duration = Duration::from_millis(300);

/// MPD sends binary data (cover art) in 8 KiB pieces unless told otherwise, one round trip each.
/// Ask for 1 MiB pieces so even a big cover takes a few requests.
const COVER_BINARY_LIMIT: usize = 1024 * 1024;
const MAX_COVER_BYTES: usize = 32 * 1024 * 1024;

/// Covers are shrunk to at most this many pixels on their longest side while decoding.
/// The card never shows them larger, and a multi-megapixel image would only cost memory and time.
pub const COVER_MAX_PIXELS: i32 = 512;

#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub state: State,
    pub song_key: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub elapsed: Option<Duration>,
    pub duration: Option<Duration>,
    /// `None` until the background lookup finishes, and for songs without art.
    pub cover: Option<Arc<CoverImage>>,
}

impl NowPlaying {
    pub fn stopped() -> Self {
        Self {
            state: State::Stop,
            song_key: None,
            title: String::new(),
            artist: String::new(),
            album: None,
            elapsed: None,
            duration: None,
            cover: None,
        }
    }
}

/// Album art that has already been decoded and shrunk, as plain bytes so it can be produced on a worker thread.
/// The overlay turns it into a `Pixbuf`.
#[derive(Debug)]
pub struct CoverImage {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub pixels: Vec<u8>,
    /// Plain average color of the whole image, before any brightening.
    pub average: Rgb,
}

impl CoverImage {
    pub fn to_pixbuf(&self) -> Pixbuf {
        Pixbuf::from_bytes(
            &Bytes::from_owned(self.pixels.clone()),
            Colorspace::Rgb,
            self.has_alpha,
            8,
            self.width,
            self.height,
            self.rowstride,
        )
    }
}

struct CoverResult {
    key: String,
    cover: Option<Arc<CoverImage>>,
}

/// Remembers the cover of the current song and runs the background lookups.
pub struct CoverCache {
    key: Option<String>,
    cover: Option<Arc<CoverImage>>,
    sender: Sender<CoverResult>,
    receiver: Receiver<CoverResult>,
}

impl Default for CoverCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverCache {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            key: None,
            cover: None,
            sender,
            receiver,
        }
    }

    /// Sleeps for up to `timeout`, but wakes immediately when a cover finishes loading so it can be sent to the overlay without waiting for the next poll.
    pub fn wait(&mut self, timeout: Duration) {
        if let Ok(result) = self.receiver.recv_timeout(timeout) {
            self.apply(result);
        }
    }

    fn collect_finished(&mut self) {
        while let Ok(result) = self.receiver.try_recv() {
            self.apply(result);
        }
    }

    /// Results for a song that is no longer current are dropped.
    fn apply(&mut self, result: CoverResult) {
        if self.key.as_deref() == Some(result.key.as_str()) {
            self.cover = result.cover;
        }
    }

    fn start(&mut self, key: &str, song_file: &str, config: &MpdConfig) {
        self.key = Some(key.to_string());
        self.cover = None;
        if !config.use_cover {
            return;
        }

        let sender = self.sender.clone();
        let key = key.to_string();
        let song_file = song_file.to_string();
        let config = config.clone();
        let spawned = thread::Builder::new()
            .name("mpd-cover".to_string())
            .spawn(move || {
                let started = Instant::now();
                let bytes = fetch_cover_bytes(&config, &song_file, started + COVER_BUDGET);
                let fetched = started.elapsed();
                let cover = bytes.as_deref().and_then(decode_cover).map(Arc::new);

                if fetched >= SLOW_COVER_LOG || (cover.is_none() && bytes.is_some()) {
                    eprintln!(
                        "Cover art for {song_file}: {} in {} ms (decoded after {} ms)",
                        match (&bytes, &cover) {
                            (None, _) => "not found".to_string(),
                            (Some(_), None) => "could not be decoded".to_string(),
                            (Some(bytes), Some(_)) => format!("{} KiB", bytes.len() / 1024),
                        },
                        fetched.as_millis(),
                        started.elapsed().as_millis(),
                    );
                }
                let _ = sender.send(CoverResult { key, cover });
            });
        if let Err(error) = spawned {
            eprintln!("Could not start cover art lookup: {error}");
        }
    }
}

pub fn read_now_playing(
    client: &mut Client,
    config: &MpdConfig,
    cache: &mut CoverCache,
) -> mpd::error::Result<NowPlaying> {
    let status = client.status()?;
    if status.state == State::Stop {
        return Ok(NowPlaying::stopped());
    }

    let Some(song) = client.currentsong()? else {
        return Ok(NowPlaying::stopped());
    };

    // Never wait for art here: a new song starts its lookup on a worker thread and is reported right away; the cover is attached once the lookup has finished.
    let song_key = format!("{}#{:?}", song.file, song.place);
    cache.collect_finished();
    if cache.key.as_deref() != Some(song_key.as_str()) {
        cache.start(&song_key, &song.file, config);
    }
    let cover = cache.cover.clone();

    let artists = collect_artists(&song);
    let album = collect_tag(&song, "Album").filter(|value| !value.trim().is_empty());
    let title = song
        .title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| collect_tag(&song, "Title"))
        .unwrap_or_else(|| fallback_title(&song.file));

    let duration = status
        .duration
        .or_else(|| status.time.map(|(_, total)| total))
        .or(song.duration);
    let elapsed = status
        .elapsed
        .or_else(|| status.time.map(|(elapsed, _)| elapsed));

    Ok(NowPlaying {
        state: status.state,
        song_key: Some(song_key),
        title,
        artist: if artists.is_empty() {
            "Unknown Artist".to_string()
        } else {
            artists.join(", ")
        },
        album,
        elapsed,
        duration,
        cover,
    })
}

/// Finds the cover for `song_file`, in order: MPD's `albumart` (cover file next to the song), the picture embedded in the song, then image files in the album folder on disk.
/// Everything shares one connection and the same `deadline`.
fn fetch_cover_bytes(config: &MpdConfig, song_file: &str, deadline: Instant) -> Option<Vec<u8>> {
    let address = format!("{}:{}", config.ip, config.port);
    if let Some(mut connection) = CoverConnection::open(&address, deadline) {
        for command in ["albumart", "readpicture"] {
            match connection.fetch_binary(command, song_file) {
                Some(Some(bytes)) => return Some(bytes),
                // MPD has nothing for this song under that command; try the next.
                Some(None) => continue,
                // Timeout or protocol error: the connection can't be trusted any more.
                None => break,
            }
        }
    }

    find_cover_file(&config.music_dir, song_file).and_then(|path| read_cover_file(&path))
}

fn read_cover_file(path: &Path) -> Option<Vec<u8>> {
    let size = fs::metadata(path).ok()?.len();
    if size == 0 || size > MAX_COVER_BYTES as u64 {
        return None;
    }
    fs::read(path).ok()
}

fn remaining(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
}

/// A short-lived connection to MPD, separate from the polling one, so a slow transfer can never hold up status updates.
/// Every read and write is bounded by `COVER_IO_TIMEOUT` and by the overall deadline.
struct CoverConnection {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    deadline: Instant,
}

impl CoverConnection {
    fn open(address: &str, deadline: Instant) -> Option<Self> {
        let target = address.to_socket_addrs().ok()?.next()?;
        let connect_timeout = COVER_CONNECT_TIMEOUT.min(remaining(deadline)?);
        let stream = TcpStream::connect_timeout(&target, connect_timeout).ok()?;
        // Requests are single small writes; don't let Nagle hold them back.
        stream.set_nodelay(true).ok()?;
        let reader = BufReader::new(stream.try_clone().ok()?);

        let mut connection = Self {
            stream,
            reader,
            deadline,
        };
        let banner = connection.read_line()?;
        if !banner.starts_with("OK MPD") {
            return None;
        }

        // MPD older than 0.22.4 doesn't know this and answers ACK; the default chunk size still works, just with more round trips.
        connection.send(&format!("binarylimit {COVER_BINARY_LIMIT}"))?;
        loop {
            let line = connection.read_line()?;
            if line == "OK" || line.starts_with("ACK") {
                break;
            }
        }
        Some(connection)
    }

    fn arm_timeouts(&self) -> Option<()> {
        let timeout = COVER_IO_TIMEOUT.min(remaining(self.deadline)?);
        self.stream.set_read_timeout(Some(timeout)).ok()?;
        self.stream.set_write_timeout(Some(timeout)).ok()?;
        Some(())
    }

    /// Sends one command as a single write.
    fn send(&mut self, command: &str) -> Option<()> {
        self.arm_timeouts()?;
        let mut line = String::with_capacity(command.len() + 1);
        line.push_str(command);
        line.push('\n');
        self.stream.write_all(line.as_bytes()).ok()
    }

    fn read_line(&mut self) -> Option<String> {
        self.arm_timeouts()?;
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end().to_string()),
        }
    }

    /// Downloads a binary response (`albumart` / `readpicture`) piece by piece.
    ///
    /// `Some(Some(bytes))` is the picture, `Some(None)` means MPD cleanly has none for this song, and `None` means the transfer failed or timed out.
    fn fetch_binary(&mut self, command: &str, uri: &str) -> Option<Option<Vec<u8>>> {
        let escaped = escape_mpd_argument(uri);
        let mut data: Vec<u8> = Vec::new();
        let mut total_size: Option<usize> = None;

        loop {
            self.send(&format!("{command} \"{escaped}\" {}", data.len()))?;

            let chunk_size = loop {
                let line = self.read_line()?;
                // `ACK` is an error (no cover file, unknown command, ...).
                // A bare `OK` is MPD's way of saying "no embedded picture": nothing follows, so waiting for more would just run into the read timeout.
                if line == "OK" || line.starts_with("ACK") {
                    return Some(None);
                }
                if let Some(value) = line.strip_prefix("size: ") {
                    let size = value.trim().parse::<usize>().ok()?;
                    if size == 0 || size > MAX_COVER_BYTES {
                        return Some(None);
                    }
                    total_size = Some(size);
                } else if let Some(value) = line.strip_prefix("binary: ") {
                    break value.trim().parse::<usize>().ok()?;
                }
            };
            if data.len() + chunk_size > MAX_COVER_BYTES {
                return Some(None);
            }

            if chunk_size > 0 {
                let start = data.len();
                data.resize(start + chunk_size, 0);
                self.arm_timeouts()?;
                self.reader.read_exact(&mut data[start..]).ok()?;
            }

            // Every chunk is followed by a newline and the usual `OK`.
            let mut terminator = [0_u8; 1];
            self.arm_timeouts()?;
            self.reader.read_exact(&mut terminator).ok()?;
            if self.read_line()? != "OK" {
                return None;
            }

            if chunk_size == 0 || total_size.map_or(false, |size| data.len() >= size) {
                break;
            }
        }

        Some((!data.is_empty()).then_some(data))
    }
}

/// Decodes straight to a small size (JPEG can skip most of the work that way) and computes the average color here, so none of it happens on the GTK thread.
fn decode_cover(bytes: &[u8]) -> Option<CoverImage> {
    let loader = PixbufLoader::new();
    loader.connect_size_prepared(|loader, width, height| {
        let longest = width.max(height);
        if longest > COVER_MAX_PIXELS {
            let scale = COVER_MAX_PIXELS as f64 / longest as f64;
            loader.set_size(
                ((width as f64 * scale).round() as i32).max(1),
                ((height as f64 * scale).round() as i32).max(1),
            );
        }
    });
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let mut pixbuf = loader.pixbuf()?;

    // Not every image loader honors the size request; make sure of it.
    let longest = pixbuf.width().max(pixbuf.height());
    if longest > COVER_MAX_PIXELS {
        let scale = COVER_MAX_PIXELS as f64 / longest as f64;
        pixbuf = pixbuf.scale_simple(
            ((pixbuf.width() as f64 * scale).round() as i32).max(1),
            ((pixbuf.height() as f64 * scale).round() as i32).max(1),
            InterpType::Bilinear,
        )?;
    }

    let average = average_color(&pixbuf);
    let data = pixbuf.read_pixel_bytes();
    let pixels: &[u8] = data.as_ref();
    Some(CoverImage {
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: pixbuf.rowstride(),
        has_alpha: pixbuf.has_alpha(),
        pixels: pixels.to_vec(),
        average,
    })
}

fn escape_mpd_argument(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character == '\\' || character == '"' {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn collect_artists(song: &mpd::Song) -> Vec<String> {
    let mut artists = Vec::new();
    for (key, value) in &song.tags {
        if key.eq_ignore_ascii_case("artist") && !value.trim().is_empty() {
            let value = value.trim().to_string();
            if !artists.contains(&value) {
                artists.push(value);
            }
        }
    }

    if artists.is_empty() {
        if let Some(artist) = song.artist.as_deref().map(str::trim) {
            if !artist.is_empty() {
                artists.push(artist.to_string());
            }
        }
    }

    artists
}

fn collect_tag(song: &mpd::Song, name: &str) -> Option<String> {
    song.tags
        .iter()
        .find(|(key, value)| key.eq_ignore_ascii_case(name) && !value.trim().is_empty())
        .map(|(_, value)| value.trim().to_string())
}

fn fallback_title(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file)
        .to_string()
}

fn find_cover_file(music_dir: &str, song_file: &str) -> Option<PathBuf> {
    let music_dir = Path::new(music_dir);
    let song_path = music_dir.join(song_file);
    let album_dir = song_path.parent()?;

    const NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart"];
    const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

    for name in NAMES {
        for extension in EXTENSIONS {
            for candidate in [
                format!("{name}.{extension}"),
                format!("{}.{extension}", capitalize(name)),
            ] {
                let path = album_dir.join(candidate);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }

    for entry in WalkDir::new(album_dir).max_depth(2).into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };

        if NAMES.contains(&stem.to_ascii_lowercase().as_str())
            && EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        {
            return Some(path.to_path_buf());
        }
    }

    None
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn falls_back_to_file_stem_for_title() {
        assert_eq!(fallback_title("Artist/Album/01 - Song.flac"), "01 - Song");
    }

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(escape_mpd_argument(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    /// Minimal MPD stand-in.
    /// `albumart` / `readpicture` payloads of `None` are answered the way MPD does: `ACK` for albumart, a bare `OK` for readpicture.
    /// Pictures are served in 5-byte pieces to exercise the chunk loop.
    fn fake_mpd(albumart: Option<Vec<u8>>, picture: Option<Vec<u8>>) -> MpdConfig {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            writer.write_all(b"OK MPD 0.23.5\n").unwrap();

            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let command = line.trim_end();
                let offset = command
                    .rsplit(' ')
                    .next()
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);

                if command.starts_with("binarylimit") {
                    writer.write_all(b"OK\n").unwrap();
                } else if command.starts_with("albumart") || command.starts_with("readpicture") {
                    let is_albumart = command.starts_with("albumart");
                    let payload = if is_albumart { &albumart } else { &picture };
                    match payload {
                        None if is_albumart => {
                            writer
                                .write_all(b"ACK [50@0] {albumart} No file exists\n")
                                .unwrap();
                        }
                        None => writer.write_all(b"OK\n").unwrap(),
                        Some(bytes) => {
                            let end = (offset + 5).min(bytes.len());
                            let piece = &bytes[offset.min(end)..end];
                            let header = format!("size: {}\nbinary: {}\n", bytes.len(), piece.len());
                            writer.write_all(header.as_bytes()).unwrap();
                            writer.write_all(piece).unwrap();
                            writer.write_all(b"\nOK\n").unwrap();
                        }
                    }
                } else {
                    writer.write_all(b"ACK [5@0] {} unknown command\n").unwrap();
                }
            }
        });

        MpdConfig {
            music_dir: "/nonexistent".to_string(),
            ip: address.ip().to_string(),
            port: address.port().to_string(),
            use_cover: true,
        }
    }

    #[test]
    fn missing_embedded_picture_does_not_wait_for_a_timeout() {
        // The old code treated MPD's bare `OK` as "more data coming" and sat on it for the full 5 s read timeout before trying anything else.
        let config = fake_mpd(None, None);
        let started = Instant::now();
        let bytes = fetch_cover_bytes(&config, "Artist/Album/song.flac", started + COVER_BUDGET);
        assert!(bytes.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn reads_an_embedded_picture_in_several_chunks() {
        let picture: Vec<u8> = (0..23).collect();
        let config = fake_mpd(None, Some(picture.clone()));
        let bytes = fetch_cover_bytes(&config, "a.flac", Instant::now() + COVER_BUDGET);
        assert_eq!(bytes, Some(picture));
    }

    #[test]
    fn albumart_is_preferred_over_the_embedded_picture() {
        let config = fake_mpd(Some(vec![1, 2, 3, 4, 5, 6, 7]), Some(vec![9; 12]));
        let bytes = fetch_cover_bytes(&config, "a.flac", Instant::now() + COVER_BUDGET);
        assert_eq!(bytes, Some(vec![1, 2, 3, 4, 5, 6, 7]));
    }

    #[test]
    fn gives_up_when_the_budget_is_spent() {
        let config = fake_mpd(None, Some(vec![1; 40]));
        let spent = Instant::now();
        assert!(fetch_cover_bytes(&config, "a.flac", spent).is_none());
    }
}
