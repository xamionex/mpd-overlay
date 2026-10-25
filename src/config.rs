use dirs;
use serde::Deserialize;
use std::{fs, path::PathBuf};

/// Track percentage used by `show_on_progress` when the option is left at its default.
const DEFAULT_SHOW_ON_PROGRESS: &str = "0";

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
    pub show_on_pause: bool,
    #[serde(deserialize_with = "deserialize_progress_percentages")]
    pub show_on_progress: String,
    pub fade_ms: u64,
    pub progress: String,
    pub center_text: bool,
    pub background: String,
    pub background_color: String,
    pub background_start_color: String,
    pub background_end_color: String,
    pub background_gradient_direction: String,
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
            show_on_pause: true,
            show_on_progress: DEFAULT_SHOW_ON_PROGRESS.to_string(),
            fade_ms: 500,
            progress: "bar".to_string(),
            center_text: true,
            background: "progress".to_string(),
            background_color: "#2c2c2c".to_string(),
            background_start_color: "#2c2c2c".to_string(),
            background_end_color: "#161616".to_string(),
            background_gradient_direction: "ltr".to_string(),
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

        self.background_gradient_direction =
            gradient_flow_name(parse_gradient_flow(&self.background_gradient_direction));
        self.show_on_progress = canonical_progress_percentages(&self.show_on_progress);
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

    pub fn background_gradient_flow(&self) -> GradientFlow {
        parse_gradient_flow(&self.background_gradient_direction)
    }

    pub fn show_on_progress_percentages(&self) -> Vec<u32> {
        parse_progress_percentages(&self.show_on_progress)
    }
}

/// Direction the background gradient flows in.
/// Named diagonals run corner to corner; `Angle` is in degrees where 0 is left to right and 90 is top to bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientFlow {
    Ltr,
    Rtl,
    Ttb,
    Btt,
    TlBr,
    TrBl,
    BlTr,
    BrTl,
    Angle(f64),
}

fn parse_gradient_flow(value: &str) -> GradientFlow {
    let trimmed = value.trim().to_ascii_lowercase();
    let normalized = trimmed.replace('_', "-").replace(' ', "-");

    match normalized.as_str() {
        "ltr" | "left-to-right" | "to-right" | "right" => GradientFlow::Ltr,
        "rtl" | "right-to-left" | "to-left" | "left" => GradientFlow::Rtl,
        "ttb" | "top-to-bottom" | "to-bottom" | "down" => GradientFlow::Ttb,
        "btt" | "bottom-to-top" | "to-top" | "up" => GradientFlow::Btt,
        "tl-br" | "top-left-to-bottom-right" | "to-bottom-right" => GradientFlow::TlBr,
        "tr-bl" | "top-right-to-bottom-left" | "to-bottom-left" => GradientFlow::TrBl,
        "bl-tr" | "bottom-left-to-top-right" | "to-top-right" => GradientFlow::BlTr,
        "br-tl" | "bottom-right-to-top-left" | "to-top-left" => GradientFlow::BrTl,
        _ => parse_gradient_angle(&trimmed)
            .or_else(|| parse_gradient_angle(&normalized))
            .map(|degrees| GradientFlow::Angle(normalize_angle(degrees)))
            .unwrap_or(GradientFlow::Ltr),
    }
}

fn parse_gradient_angle(value: &str) -> Option<f64> {
    let number = value.strip_suffix("deg").unwrap_or(value).trim();
    number
        .parse::<f64>()
        .ok()
        .filter(|degrees| degrees.is_finite())
}

/// Folds an angle into `0..360`, rounded to two decimals so float noise never reaches the config.
fn normalize_angle(degrees: f64) -> f64 {
    if !degrees.is_finite() {
        return 0.0;
    }
    let normalized = degrees.rem_euclid(360.0);
    let rounded = (normalized * 100.0).round() / 100.0;
    if rounded >= 360.0 {
        0.0
    } else {
        rounded
    }
}

fn gradient_flow_name(flow: GradientFlow) -> String {
    match flow {
        GradientFlow::Ltr => "ltr".to_string(),
        GradientFlow::Rtl => "rtl".to_string(),
        GradientFlow::Ttb => "ttb".to_string(),
        GradientFlow::Btt => "btt".to_string(),
        GradientFlow::TlBr => "tl-br".to_string(),
        GradientFlow::TrBl => "tr-bl".to_string(),
        GradientFlow::BlTr => "bl-tr".to_string(),
        GradientFlow::BrTl => "br-tl".to_string(),
        GradientFlow::Angle(degrees) => {
            let degrees = normalize_angle(degrees);
            if degrees == 0.0 {
                "ltr".to_string()
            } else if degrees == 90.0 {
                "ttb".to_string()
            } else if degrees == 180.0 {
                "rtl".to_string()
            } else if degrees == 270.0 {
                "btt".to_string()
            } else {
                format!("{degrees}deg")
            }
        }
    }
}

/// Track percentages at which the overlay should be shown.
/// Values may be separated by commas, semicolons, or whitespace, and are clamped to 0..=100.
fn parse_progress_percentages(value: &str) -> Vec<u32> {
    let mut percentages: Vec<u32> = value
        .split(|character: char| character == ',' || character == ';' || character.is_whitespace())
        .filter_map(parse_progress_percentage)
        .collect();
    percentages.sort_unstable();
    percentages.dedup();
    percentages
}

fn parse_progress_percentage(token: &str) -> Option<u32> {
    let token = token.trim().trim_end_matches('%').trim();
    if token.is_empty() {
        return None;
    }
    let value = token.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(value.round().clamp(0.0, 100.0) as u32)
}

fn canonical_progress_percentages(value: &str) -> String {
    let percentages = parse_progress_percentages(value);
    if percentages.is_empty() && !value.trim().is_empty() {
        // A typo should not silently turn the feature off.
        return DEFAULT_SHOW_ON_PROGRESS.to_string();
    }
    percentages
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Accepts either a delimited string or a TOML list of percentages, so both `"25, 50, 75"` and `[25, 50, 75]` work.
fn deserialize_progress_percentages<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct ProgressPercentagesVisitor;

    impl<'de> serde::de::Visitor<'de> for ProgressPercentagesVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a percentage string or a list of percentages")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(canonical_progress_percentages(value))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(canonical_progress_percentages(&value))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(canonical_progress_percentages(&value.to_string()))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(canonical_progress_percentages(&value.to_string()))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(canonical_progress_percentages(&value.to_string()))
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element::<f64>()? {
                values.push(value);
            }
            let joined = values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(",");
            Ok(canonical_progress_percentages(&joined))
        }
    }

    deserializer.deserialize_any(ProgressPercentagesVisitor)
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

    #[test]
    fn missing_new_options_use_defaults() {
        let config: Config = toml::from_str("[overlay]\nwidth = 640\n").unwrap();
        assert!(config.overlay.show_on_pause);
        assert_eq!(config.overlay.show_on_progress, "50");
        assert_eq!(config.overlay.background_gradient_direction, "ltr");
    }

    #[test]
    fn gradient_direction_is_normalized() {
        let sanitized = |value: &str| {
            OverlayConfig {
                background_gradient_direction: value.to_string(),
                ..OverlayConfig::default()
            }
            .sanitized()
            .background_gradient_direction
        };

        assert_eq!(sanitized("LTR"), "ltr");
        assert_eq!(sanitized(" Top_To_Bottom "), "ttb");
        assert_eq!(sanitized("bottom-right-to-top-left"), "br-tl");
        assert_eq!(sanitized("45deg"), "45deg");
        assert_eq!(sanitized("45 deg"), "45deg");
        assert_eq!(sanitized("-45"), "315deg");
        assert_eq!(sanitized("450"), "ttb");
        assert_eq!(sanitized("sideways"), "ltr");
    }

    #[test]
    fn gradient_direction_parses_into_flows() {
        let flow = |value: &str| {
            OverlayConfig {
                background_gradient_direction: value.to_string(),
                ..OverlayConfig::default()
            }
            .background_gradient_flow()
        };

        assert_eq!(flow("ltr"), GradientFlow::Ltr);
        assert_eq!(flow("rtl"), GradientFlow::Rtl);
        assert_eq!(flow("ttb"), GradientFlow::Ttb);
        assert_eq!(flow("btt"), GradientFlow::Btt);
        assert_eq!(flow("tl-br"), GradientFlow::TlBr);
        assert_eq!(flow("135deg"), GradientFlow::Angle(135.0));
        assert_eq!(flow("nonsense"), GradientFlow::Ltr);
    }

    #[test]
    fn show_on_pause_is_on_by_default() {
        assert!(OverlayConfig::default().show_on_pause);
    }

    #[test]
    fn progress_percentages_accept_common_separators() {
        let sanitized = |value: &str| {
            OverlayConfig {
                show_on_progress: value.to_string(),
                ..OverlayConfig::default()
            }
            .sanitized()
            .show_on_progress
        };

        assert_eq!(sanitized("25,50,75"), "25,50,75");
        assert_eq!(sanitized("25, 50, 75"), "25,50,75");
        assert_eq!(sanitized("25 50 75"), "25,50,75");
        assert_eq!(sanitized("75,25,50,50"), "25,50,75");
        assert_eq!(sanitized("25% 50% 75%"), "25,50,75");
        assert_eq!(sanitized("150,-5"), "0,100");
        assert_eq!(sanitized(""), "");
        assert_eq!(sanitized("nonsense"), "50");
    }

    #[test]
    fn progress_option_accepts_strings_lists_and_numbers() {
        let from_string: Config =
            toml::from_str("[overlay]\nshow_on_progress = \"25, 50, 75\"\n").unwrap();
        assert_eq!(from_string.overlay.show_on_progress, "25,50,75");

        let from_list: Config =
            toml::from_str("[overlay]\nshow_on_progress = [75, 25, 50]\n").unwrap();
        assert_eq!(from_list.overlay.show_on_progress, "25,50,75");

        let from_number: Config = toml::from_str("[overlay]\nshow_on_progress = 50\n").unwrap();
        assert_eq!(from_number.overlay.show_on_progress, "50");
    }

    #[test]
    fn show_on_progress_defaults_to_half() {
        let config = OverlayConfig::default();
        assert_eq!(config.show_on_progress, "50");
        assert_eq!(config.show_on_progress_percentages(), vec![50]);
    }
}
