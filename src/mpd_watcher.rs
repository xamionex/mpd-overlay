use mpd::Client;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    thread,
    time::Duration,
};

use crate::{
    config::MpdConfig,
    song_info::{read_now_playing, CoverCache, NowPlaying},
};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum Update {
    NowPlaying(NowPlaying),
    Disconnected,
}

#[derive(Clone)]
pub struct WatcherHandle {
    stop: Arc<AtomicBool>,
}

impl WatcherHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn spawn(config: MpdConfig, sender: Sender<Update>) -> WatcherHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let watcher_stop = Arc::clone(&stop);

    let _ = thread::Builder::new()
        .name("mpd-watcher".to_string())
        .spawn(move || run(config, sender, watcher_stop));

    WatcherHandle { stop }
}

fn run(config: MpdConfig, sender: Sender<Update>, stop: Arc<AtomicBool>) {
    let address = format!("{}:{}", config.ip, config.port);
    let mut cache = CoverCache::new();

    while !stop.load(Ordering::SeqCst) {
        match Client::connect(&address) {
            Ok(mut client) => {
                eprintln!("Connected to MPD at {address}");
                let mut last_error = String::new();

                loop {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }

                    match read_now_playing(&mut client, &config, &mut cache) {
                        Ok(now_playing) => {
                            last_error.clear();
                            if sender.send(Update::NowPlaying(now_playing)).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            let message = error.to_string();
                            if message != last_error {
                                eprintln!("Lost MPD connection: {message}");
                                last_error = message;
                            }
                            let _ = sender.send(Update::Disconnected);
                            break;
                        }
                    }

                    // Covers load on their own thread.
                    // Waiting here (instead of a plain sleep) means a finished cover is reported immediately.
                    cache.wait(POLL_INTERVAL);
                }
            }
            Err(error) => {
                eprintln!("Could not connect to MPD at {address}: {error}");
            }
        }

        if stop.load(Ordering::SeqCst) {
            return;
        }
        thread::sleep(RECONNECT_INTERVAL);
    }
}
