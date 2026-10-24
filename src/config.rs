use dirs;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Config {
    #[serde(default)]
    pub mpd: MpdConfig,
    #[serde(default)]
    pub overlay: OverlayConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MpdConfig {
    pub music_dir: String,
    pub ip: String,
    pub port: String,
    #[serde(default = "default_true")]
    pub use_cover: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub width: i32,
    pub height: i32,
    pub padding: i32,
    pub corner_radius: i32,
    pub cover_radius: i32,
    pub anchor: String,
    pub margin: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub monitor: String,
    pub timeout_ms: u64,
    pub fade_ms: u64,
    pub progress: String,
    pub center_text: bool,
    pub background: String,
    pub background_color: String,
    pub background_start_color: String,
    pub background_end_color: String,
    pub background_blend: u32,
    pub border: String,
    #[serde(alias = "matching_colors")]
    pub timestamps_matching_colors: bool,
    pub title_matching_colors: bool,
    pub title_invert_colors: bool,
    pub title_glow: bool,
    pub title_glow_invert_colors: bool,
    pub artist_matching_colors: bool,
    pub artist_invert_colors: bool,
    pub artist_glow: bool,
    pub artist_glow_invert_colors: bool,
    pub timestamps_invert_colors: bool,
    pub timestamps_glow: bool,
    pub timestamps_glow_invert_colors: bool,
    pub background_progress_matching_colors: bool,
    pub background_progress_invert: bool,
    pub progress_matching_colors: bool,
    pub progress_invert_colors: bool,
    pub always_brighten_colors: bool,
    pub title_color: String,
    pub artist_color: String,
    pub time_color: String,
    pub shadow_color: String,
    pub fallback_color: String,
    pub progress_start_color: String,
    pub progress_mid_color: String,
    pub progress_blend_color: String,
    pub progress_end_color: String,
    pub scroll_ms: u64,
    pub progress_bar_blend: u32,
    pub background_alpha: f64,
    pub text_alpha: f64,
    pub progress_alpha: f64,
    pub cover_alpha: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mpd: MpdConfig::default(),
            overlay: OverlayConfig::default(),
        }
    }
}

impl MpdConfig {
    pub fn sanitized(self) -> Self {
        self
    }
}

impl Default for MpdConfig {
    fn default() -> Self {
        let home = dirs::home_dir().map(|path| path.join("Music"));
        Self {
            music_dir: home
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/home/petar/Music".to_string()),
            ip: "127.0.0.1".to_string(),
            port: "6600".to_string(),
            use_cover: true,
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            width: 500,
            height: 100,
            padding: 5,
            corner_radius: 2,
            cover_radius: 15,
            anchor: "top-right".to_string(),
            margin: 20,
            offset_x: 0,
            offset_y: 0,
            monitor: "primary".to_string(),
            timeout_ms: 0,
            fade_ms: 500,
            progress: "bar".to_string(),
            center_text: true,
            background: "progress".to_string(),
            background_color: "#2c2c2c".to_string(),
            background_start_color: "#2c2c2c".to_string(),
            background_end_color: "#161616".to_string(),
            background_blend: 10,
            border: "#161616".to_string(),
            timestamps_matching_colors: true,
            title_matching_colors: true,
            title_invert_colors: false,
            title_glow: true,
            title_glow_invert_colors: true,
            artist_matching_colors: true,
            artist_invert_colors: false,
            artist_glow: true,
            artist_glow_invert_colors: true,
            timestamps_invert_colors: false,
            timestamps_glow: true,
            timestamps_glow_invert_colors: true,
            background_progress_matching_colors: true,
            background_progress_invert: false,
            progress_matching_colors: false,
            progress_invert_colors: false,
            always_brighten_colors: false,
            title_color: "#ffffff".to_string(),
            artist_color: "#ffffff".to_string(),
            time_color: "#ffffff".to_string(),
            shadow_color: "#000000".to_string(),
            fallback_color: "#2c2c2c".to_string(),
            progress_start_color: "#ffffff".to_string(),
            progress_mid_color: "#ffffff".to_string(),
            progress_blend_color: "#000000".to_string(),
            progress_end_color: "#000000".to_string(),
            scroll_ms: 8000,
            progress_bar_blend: 0,
            background_alpha: 1.0,
            text_alpha: 1.0,
            progress_alpha: 1.0,
            cover_alpha: 1.0,
        }
    }
}

impl OverlayConfig {
    pub fn sanitized(mut self) -> Self {
        self.width = self.width.clamp(240, 1600);
        self.height = self.height.clamp(90, 600);
        self.margin = self.margin.clamp(0, 400);
        self.padding = self.padding.max(0);
        self.corner_radius = self.corner_radius.max(0);
        self.cover_radius = self.cover_radius.max(0);
        self.timeout_ms = self.timeout_ms.min(3600000);
        self.fade_ms = self.fade_ms.min(5000);
        self.scroll_ms = self.scroll_ms.max(500).min(100000);
        self.background_alpha = self.background_alpha.clamp(0.0, 1.0);
        self.text_alpha = self.text_alpha.clamp(0.0, 1.0);
        self.progress_alpha = self.progress_alpha.clamp(0.0, 1.0);
        self.cover_alpha = self.cover_alpha.clamp(0.0, 1.0);
        self.progress_bar_blend = self.progress_bar_blend.clamp(0, 100);
        self.background_blend = self.background_blend.clamp(0, 100);

        self.anchor = self.anchor.to_ascii_lowercase().replace(" ", "-");
        let valid_anchors = [
            "top-left",
            "top-center",
            "top-right",
            "center-left",
            "center",
            "center-right",
            "bottom-left",
            "bottom-center",
            "bottom-right",
        ];
        if !valid_anchors.contains(&self.anchor.as_str()) {
            self.anchor = "bottom-center".to_string();
        }

        self.progress = self.progress.to_ascii_lowercase();
        let valid_progress = ["bar", "compact", "line", "none"];
        if !valid_progress.contains(&self.progress.as_str()) {
            self.progress = "bar".to_string();
        }

        self.background = self.background.to_ascii_lowercase();
        let valid_background = ["progress", "gradient", "hex"];
        if !valid_background.contains(&self.background.as_str()) {
            let looks_hex = self.background.starts_with('#') && self.background.len() == 7;
            if looks_hex {
                self.background_color = self.background.clone();
                self.background = "hex".to_string();
            } else {
                self.background = "progress".to_string();
            }
        }
        self
    }

    pub fn is_top(&self) -> bool {
        self.anchor.starts_with("top")
    }

    pub fn shows_progress_bar(&self) -> bool {
        self.progress == "bar"
    }

    pub fn shows_time_texts(&self) -> bool {
        self.progress == "bar" || self.progress == "compact"
    }

    pub fn shows_bottom_line(&self) -> bool {
        self.progress == "line"
    }

    pub fn shows_compact_progress(&self) -> bool {
        self.progress == "compact" || self.progress == "line"
    }
}

impl Config {
    pub fn from_file() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = get_config_path()?;
        ensure_default_config(&config_path)?;

        let config_contents = fs::read_to_string(&config_path)?;
        let mut raw: toml::Value = toml::from_str(&config_contents)?;
        migrate_legacy_glow_keys(&mut raw);
        let mut config: Config = raw.try_into()?;
        config.mpd = config.mpd.sanitized();
        config.overlay = config.overlay.sanitized();

        Ok(config)
    }

    pub fn config_dir() -> Result<PathBuf, std::io::Error> {
        let home_dir = dirs::home_dir().ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Home directory not found",
        ))?;
        Ok(home_dir.join(".config/mpd-overlay"))
    }

    pub fn config_path() -> Result<PathBuf, std::io::Error> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("config.toml"))
    }

    pub fn default_string() -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(&Config::default())
    }
}

fn default_true() -> bool {
    true
}

/// Older configs had an on/off switch such as `title_glow` next to a matching key such as `title_glow_matching_colors`.
/// The switch is gone and the matching key is now simply `title_glow`, and the same goes for `artist_glow` and `timestamps_glow`.
/// Fold the two together so a config written by an older version keeps the glow behaviour it actually had (glow on only if both were on).
fn migrate_legacy_glow_keys(raw: &mut toml::Value) {
    let Some(overlay) = raw
        .get_mut("overlay")
        .and_then(|overlay| overlay.as_table_mut())
    else {
        return;
    };

    for element in ["title", "artist", "timestamps"] {
        let legacy_key = format!("{element}_glow_matching_colors");
        let Some(matching) = overlay.remove(&legacy_key) else {
            continue;
        };

        let glow_key = format!("{element}_glow");
        let matching = matching.as_bool().unwrap_or(true);
        let old_switch = overlay
            .get(&glow_key)
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        overlay.insert(glow_key, toml::Value::Boolean(matching && old_switch));
    }
}

fn get_config_path() -> Result<PathBuf, std::io::Error> {
    Config::config_path()
}

fn ensure_default_config(config_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if config_path.exists() {
        return Ok(());
    }

    let config_dir = config_path.parent().ok_or_else(|| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config directory has no parent",
        ))
    })?;
    fs::create_dir_all(config_dir).map_err(|error| error.to_string())?;
    let contents = Config::default_string()?;
    fs::write(config_path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips_through_toml() {
        let text = Config::default_string().unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn legacy_glow_keys_are_folded_into_one_setting() {
        let mut raw: toml::Value = toml::from_str(
            "[overlay]\n\
             title_glow = true\n\
             title_glow_matching_colors = false\n\
             artist_glow = false\n\
             artist_glow_matching_colors = true\n\
             timestamps_glow_matching_colors = true\n",
        )
        .unwrap();
        migrate_legacy_glow_keys(&mut raw);

        let overlay = raw.get("overlay").unwrap();
        let flag = |key: &str| overlay.get(key).and_then(|value| value.as_bool());
        assert_eq!(flag("title_glow"), Some(false));
        assert_eq!(flag("artist_glow"), Some(false));
        assert_eq!(flag("timestamps_glow"), Some(true));
        assert_eq!(flag("title_glow_matching_colors"), None);
    }
}
