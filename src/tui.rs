use std::{fs, io, path::PathBuf, time::Duration};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldKind {
    Text,
    Bool,
    Integer,
}

struct Field {
    key: &'static str,
    label: &'static str,
    help: &'static str,
    section: &'static str,
    kind: FieldKind,
}

const FIELDS: &[Field] = &[
    // -- Connection ------------------------------------------------
    Field {
        key: "music_dir",
        label: "Music directory",
        help: "Directory used for filesystem cover lookups",
        section: "Connection",
        kind: FieldKind::Text,
    },
    Field {
        key: "ip",
        label: "MPD IP address",
        help: "MPD server IP address",
        section: "Connection",
        kind: FieldKind::Text,
    },
    Field {
        key: "port",
        label: "MPD port",
        help: "MPD server port",
        section: "Connection",
        kind: FieldKind::Text,
    },
    Field {
        key: "use_cover",
        label: "Load album covers",
        help: "Load album art and derive overlay colors from it",
        section: "Connection",
        kind: FieldKind::Bool,
    },
    // -- Visibility ------------------------------------------------
    Field {
        key: "anchor",
        label: "Anchor",
        help: "top-left / top-center / ... / bottom-right",
        section: "Visibility",
        kind: FieldKind::Text,
    },
    Field {
        key: "margin",
        label: "Margin",
        help: "Distance from the monitor edge in pixels",
        section: "Visibility",
        kind: FieldKind::Integer,
    },
    Field {
        key: "offset_x",
        label: "Offset X",
        help: "Horizontal offset from anchor in pixels",
        section: "Visibility",
        kind: FieldKind::Integer,
    },
    Field {
        key: "offset_y",
        label: "Offset Y",
        help: "Vertical offset from anchor in pixels",
        section: "Visibility",
        kind: FieldKind::Integer,
    },
    Field {
        key: "monitor",
        label: "Monitor",
        help: "primary or a zero-based monitor index",
        section: "Visibility",
        kind: FieldKind::Text,
    },
    Field {
        key: "timeout_ms",
        label: "Show timeout",
        help: "Milliseconds before auto-hide, 0 keeps it visible",
        section: "Visibility",
        kind: FieldKind::Integer,
    },
    Field {
        key: "show_on_pause",
        label: "Show on pause",
        help: "Show the overlay when playback pauses; off hides it while paused",
        section: "Visibility",
        kind: FieldKind::Bool,
    },
    Field {
        key: "show_on_progress",
        label: "Show on progress",
        help: "Track percentages to show at, e.g. 25,50,75; empty disables",
        section: "Visibility",
        kind: FieldKind::Text,
    },
    Field {
        key: "fade_ms",
        label: "Fade duration",
        help: "Fade animation duration in milliseconds",
        section: "Visibility",
        kind: FieldKind::Integer,
    },
    Field {
        key: "progress",
        label: "Progress style",
        help: "bar, compact, line, or none",
        section: "Visibility",
        kind: FieldKind::Text,
    },
    // -- Layout ----------------------------------------------------
    Field {
        key: "width",
        label: "Overlay width",
        help: "Card width in pixels",
        section: "Layout",
        kind: FieldKind::Integer,
    },
    Field {
        key: "height",
        label: "Overlay height",
        help: "Card height in pixels",
        section: "Layout",
        kind: FieldKind::Integer,
    },
    Field {
        key: "padding",
        label: "Padding",
        help: "Inner card padding in pixels",
        section: "Layout",
        kind: FieldKind::Integer,
    },
    Field {
        key: "corner_radius",
        label: "Corner radius",
        help: "Card outer corner radius in pixels",
        section: "Layout",
        kind: FieldKind::Integer,
    },
    Field {
        key: "cover_radius",
        label: "Cover radius",
        help: "Album cover corner radius in pixels",
        section: "Layout",
        kind: FieldKind::Integer,
    },
    // -- Color -----------------------------------------------------
    Field {
        key: "background",
        label: "Background",
        help: "progress, gradient, or hex",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "background_color",
        label: "Background color",
        help: "Color used when Background is set to hex",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "background_start_color",
        label: "Background start",
        help: "Background gradient start color",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "background_end_color",
        label: "Background end",
        help: "Background gradient end color",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "background_gradient_direction",
        label: "Gradient flow",
        help: "ltr, rtl, ttb, btt, diagonals, or 45deg (0deg = ltr, 90deg = ttb)",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "background_blend",
        label: "Background blend",
        help: "0 = hard edge, 100 = soft blend (about 25% of the flow)",
        section: "Color",
        kind: FieldKind::Integer,
    },
    Field {
        key: "progress_start_color",
        label: "Progress start",
        help: "Progress bar start color",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "progress_end_color",
        label: "Progress end",
        help: "Progress bar end color",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "progress_bar_blend",
        label: "Progress blend",
        help: "0 = hard edge, 100 = soft blend (about 25% of the width)",
        section: "Color",
        kind: FieldKind::Integer,
    },
    Field {
        key: "border",
        label: "Border",
        help: "Border color as #rrggbb",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "fallback_color",
        label: "Fallback color",
        help: "Color used when there is no album art",
        section: "Color",
        kind: FieldKind::Text,
    },
    Field {
        key: "title_matching_colors",
        label: "Title matching",
        help: "Match title color to album art",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "title_invert_colors",
        label: "Title invert",
        help: "Invert matched title color",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "artist_matching_colors",
        label: "Artist matching",
        help: "Match artist color to album art",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "artist_invert_colors",
        label: "Artist invert",
        help: "Invert matched artist color",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "timestamps_matching_colors",
        label: "Timestamps matching",
        help: "Match time color to album art",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "timestamps_invert_colors",
        label: "Timestamps invert",
        help: "Invert matched time color",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "progress_matching_colors",
        label: "Progress matching",
        help: "Match progress bar colors",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "progress_invert_colors",
        label: "Progress invert",
        help: "Invert matched progress colors",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "background_progress_matching_colors",
        label: "Background match",
        help: "Match background gradient to album art",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "background_progress_invert",
        label: "Background invert",
        help: "Invert matched background colors",
        section: "Color",
        kind: FieldKind::Bool,
    },
    Field {
        key: "always_brighten_colors",
        label: "Always brighten",
        help: "Always brighten derived colors",
        section: "Color",
        kind: FieldKind::Bool,
    },
    // -- Text ------------------------------------------------------
    Field {
        key: "center_text",
        label: "Center text",
        help: "Center title and artist text",
        section: "Text",
        kind: FieldKind::Bool,
    },
    Field {
        key: "title_color",
        label: "Title color",
        help: "Title color as #rrggbb when matching is disabled",
        section: "Text",
        kind: FieldKind::Text,
    },
    Field {
        key: "artist_color",
        label: "Artist color",
        help: "Artist color as #rrggbb when matching is disabled",
        section: "Text",
        kind: FieldKind::Text,
    },
    Field {
        key: "time_color",
        label: "Time color",
        help: "Time color as #rrggbb when matching is disabled",
        section: "Text",
        kind: FieldKind::Text,
    },
    Field {
        key: "shadow_color",
        label: "Shadow color",
        help: "Drop shadow drawn under text",
        section: "Text",
        kind: FieldKind::Text,
    },
    Field {
        key: "scroll_ms",
        label: "Marquee duration",
        help: "Marquee cycling duration in milliseconds",
        section: "Text",
        kind: FieldKind::Integer,
    },
    Field {
        key: "title_glow",
        label: "Title glow",
        help: "Glow behind the title, colored from album art",
        section: "Text",
        kind: FieldKind::Bool,
    },
    Field {
        key: "title_glow_invert_colors",
        label: "Title glow invert",
        help: "Invert title glow",
        section: "Text",
        kind: FieldKind::Bool,
    },
    Field {
        key: "artist_glow",
        label: "Artist glow",
        help: "Glow behind the artist, colored from album art",
        section: "Text",
        kind: FieldKind::Bool,
    },
    Field {
        key: "artist_glow_invert_colors",
        label: "Artist glow invert",
        help: "Invert artist glow",
        section: "Text",
        kind: FieldKind::Bool,
    },
    Field {
        key: "timestamps_glow",
        label: "Timestamps glow",
        help: "Glow behind the timestamps, colored from album art",
        section: "Text",
        kind: FieldKind::Bool,
    },
    Field {
        key: "timestamps_glow_invert_colors",
        label: "Timestamps glow invert",
        help: "Invert timestamps glow",
        section: "Text",
        kind: FieldKind::Bool,
    },
    // -- Opacity ---------------------------------------------------
    Field {
        key: "background_alpha",
        label: "Background alpha",
        help: "Background opacity from 0.0 to 1.0",
        section: "Opacity",
        kind: FieldKind::Text,
    },
    Field {
        key: "text_alpha",
        label: "Text alpha",
        help: "Text opacity from 0.0 to 1.0",
        section: "Opacity",
        kind: FieldKind::Text,
    },
    Field {
        key: "progress_alpha",
        label: "Progress alpha",
        help: "Progress opacity from 0.0 to 1.0",
        section: "Opacity",
        kind: FieldKind::Text,
    },
    Field {
        key: "cover_alpha",
        label: "Cover alpha",
        help: "Album cover opacity from 0.0 to 1.0",
        section: "Opacity",
        kind: FieldKind::Text,
    },
];

enum Entry {
    Section(&'static str),
    Field(usize),
}

enum Mode {
    Form,
    EditText(TextEdit),
    PickList(PickList),
}

struct TextEdit {
    key: &'static str,
    buffer: String,
    cursor: usize,
}

struct PickList {
    key: &'static str,
    choices: Vec<String>,
    selected: usize,
}

struct App {
    config: Config,
    path: PathBuf,
    selected: usize,
    mode: Mode,
    dirty: bool,
    status: String,
}

impl App {
    fn load(path: PathBuf) -> Self {
        let config = Config::from_file().unwrap_or_else(|_| Config::default());
        Self {
            config,
            path,
            selected: 0,
            mode: Mode::Form,
            dirty: false,
            status: "Up/Down select   Space toggle   Enter edit   s save   q quit   Ctrl+C quit"
                .to_string(),
        }
    }

    fn entries(&self) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut current_section = "";
        for (index, field) in FIELDS.iter().enumerate() {
            if field.section != current_section {
                entries.push(Entry::Section(field.section));
                current_section = field.section;
            }
            entries.push(Entry::Field(index));
        }
        entries
    }

    fn normalize_selection(&mut self) {
        let entries = self.entries();
        if entries.is_empty() {
            self.selected = 0;
            return;
        }
        if self.selected >= entries.len() {
            self.selected = entries.len().saturating_sub(1);
        }
        if matches!(entries.get(self.selected), Some(Entry::Section(_))) {
            if self.selected + 1 < entries.len() {
                self.selected += 1;
            } else {
                self.selected = self.selected.saturating_sub(1);
            }
        }
    }

    fn selected_field_index(&self) -> Option<usize> {
        let entries = self.entries();
        match entries.get(self.selected) {
            Some(Entry::Field(index)) => Some(*index),
            _ => None,
        }
    }

    fn field_key(&self) -> Option<&'static str> {
        self.selected_field_index().map(|index| FIELDS[index].key)
    }

    fn move_selection(&mut self, delta: isize) {
        let entries = self.entries();
        if entries.is_empty() {
            return;
        }
        let mut next = self.selected as isize + delta;
        while next >= 0 && next < entries.len() as isize {
            match entries[next as usize] {
                Entry::Field(_) => break,
                _ => next += if delta >= 0 { 1 } else { -1 },
            }
        }
        if next < 0 {
            next = 0;
            while next >= 0
                && (next as usize) < entries.len()
                && matches!(entries[next as usize], Entry::Section(_))
            {
                next += 1;
            }
        }
        if next > entries.len() as isize - 1 {
            next = entries.len() as isize - 1;
            while next >= 0 && matches!(entries[next as usize], Entry::Section(_)) {
                next -= 1;
            }
        }
        if next >= 0 && next < entries.len() as isize {
            self.selected = next as usize;
        }
    }

    fn jump_first(&mut self) {
        self.selected = 0;
        self.normalize_selection();
    }

    fn jump_last(&mut self) {
        let entries = self.entries();
        self.selected = entries.len().saturating_sub(1);
        self.normalize_selection();
    }

    fn cycle_choice(&mut self, delta: i64) {
        let key = match self.field_key() {
            Some(key) => key,
            None => return,
        };

        let kind = self
            .selected_field_index()
            .map(|index| FIELDS[index].kind)
            .unwrap_or(FieldKind::Text);

        match kind {
            FieldKind::Bool => self.toggle_bool(key),
            FieldKind::Integer => {
                let current = self.current_value_string(key).unwrap_or_default();
                if let Ok(mut value) = current.parse::<i64>() {
                    let step = match key {
                        "width" | "height" => 10,
                        "margin" => 5,
                        "timeout_ms" | "fade_ms" | "scroll_ms" => 50,
                        _ => 1,
                    };
                    // Offsets are the only integers that may be negative.
                    let floor = if matches!(key, "offset_x" | "offset_y") {
                        i64::MIN
                    } else {
                        0
                    };
                    value = (value + delta * step).max(floor);
                    let _ = self.apply_text(key, &value.to_string());
                }
            }
            FieldKind::Text => match key {
                "progress" => {
                    let choices = ["bar", "compact", "line", "none"];
                    let current = choices
                        .iter()
                        .position(|c| *c == self.config.overlay.progress)
                        .unwrap_or(0);
                    let next = ((current as i64 + delta).rem_euclid(choices.len() as i64)) as usize;
                    self.config.overlay.progress = choices[next].to_string();
                }
                "background" => {
                    let choices = ["progress", "gradient", "hex"];
                    let current = choices
                        .iter()
                        .position(|c| *c == self.config.overlay.background)
                        .unwrap_or(0);
                    let next = ((current as i64 + delta).rem_euclid(choices.len() as i64)) as usize;
                    self.config.overlay.background = choices[next].to_string();
                }
                "background_gradient_direction" => {
                    let choices = [
                        "ltr", "rtl", "ttb", "btt", "tl-br", "tr-bl", "bl-tr", "br-tl",
                    ];
                    let current = choices
                        .iter()
                        .position(|c| *c == self.config.overlay.background_gradient_direction)
                        .unwrap_or(0);
                    let next = ((current as i64 + delta).rem_euclid(choices.len() as i64)) as usize;
                    self.config.overlay.background_gradient_direction = choices[next].to_string();
                }
                "anchor" => {
                    let choices = [
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
                    let current = choices
                        .iter()
                        .position(|c| *c == self.config.overlay.anchor)
                        .unwrap_or(0);
                    let next = ((current as i64 + delta).rem_euclid(choices.len() as i64)) as usize;
                    self.config.overlay.anchor = choices[next].to_string();
                }
                "monitor" => {
                    let choices: Vec<String> = std::iter::once("primary".to_string())
                        .chain(detect_monitors())
                        .collect();
                    let current = choices
                        .iter()
                        .position(|c| c == &self.config.overlay.monitor)
                        .unwrap_or(0);
                    let next = ((current as i64 + delta).rem_euclid(choices.len() as i64)) as usize;
                    self.config.overlay.monitor = choices[next].clone();
                }
                "background_alpha" | "text_alpha" | "progress_alpha" | "cover_alpha" => {
                    let current = self.current_value_string(key).unwrap_or_default();
                    if let Ok(value) = current.parse::<f64>() {
                        // Step in tenths, then snap to two decimals so float error (0.1 + 0.2 = 0.30000000000000004) never reaches the config.
                        let stepped = (value + delta as f64 * 0.1).clamp(0.0, 1.0);
                        let value = (stepped * 100.0).round() / 100.0;
                        let _ = self.apply_text(key, &value.to_string());
                    }
                }
                _ => {}
            },
        }
        self.config.overlay = self.config.overlay.clone().sanitized();
        self.dirty = true;
        self.status = "Value updated.".to_string();
    }

    fn field_choices(&self, key: &'static str) -> Option<Vec<String>> {
        match key {
            "progress" => Some(vec![
                "bar".to_string(),
                "compact".to_string(),
                "line".to_string(),
                "none".to_string(),
            ]),
            "background" => Some(vec![
                "progress".to_string(),
                "gradient".to_string(),
                "hex".to_string(),
            ]),
            "anchor" => Some(vec![
                "top-left".to_string(),
                "top-center".to_string(),
                "top-right".to_string(),
                "center-left".to_string(),
                "center".to_string(),
                "center-right".to_string(),
                "bottom-left".to_string(),
                "bottom-center".to_string(),
                "bottom-right".to_string(),
            ]),
            "monitor" => Some(
                std::iter::once("primary".to_string())
                    .chain(detect_monitors())
                    .collect(),
            ),
            _ => None,
        }
    }

    fn activate(&mut self) {
        let key = match self.field_key() {
            Some(key) => key,
            None => return,
        };
        let field_index = match self.selected_field_index() {
            Some(index) => index,
            None => return,
        };

        if matches!(FIELDS[field_index].kind, FieldKind::Bool) {
            self.toggle_bool(key);
            self.dirty = true;
            self.status = "Value toggled.".to_string();
            return;
        }

        if let Some(choices) = self.field_choices(key) {
            let current = self.current_value_string(key).unwrap_or_default();
            let selected = choices.iter().position(|c| c == &current).unwrap_or(0);
            self.mode = Mode::PickList(PickList {
                key,
                choices,
                selected,
            });
            return;
        }

        let buffer = self.current_value_string(key).unwrap_or_default();
        self.mode = Mode::EditText(TextEdit {
            key: FIELDS[field_index].key,
            cursor: buffer.chars().count(),
            buffer,
        });
    }

    fn toggle_bool(&mut self, key: &str) {
        match key {
            "use_cover" => self.config.mpd.use_cover = !self.config.mpd.use_cover,
            "show_on_pause" => {
                self.config.overlay.show_on_pause = !self.config.overlay.show_on_pause
            }
            "center_text" => self.config.overlay.center_text = !self.config.overlay.center_text,
            "timestamps_matching_colors" => {
                self.config.overlay.timestamps_matching_colors =
                    !self.config.overlay.timestamps_matching_colors
            }
            "title_matching_colors" => {
                self.config.overlay.title_matching_colors =
                    !self.config.overlay.title_matching_colors
            }
            "title_glow_invert_colors" => {
                self.config.overlay.title_glow_invert_colors =
                    !self.config.overlay.title_glow_invert_colors
            }
            "title_glow" => self.config.overlay.title_glow = !self.config.overlay.title_glow,
            "title_invert_colors" => {
                self.config.overlay.title_invert_colors = !self.config.overlay.title_invert_colors
            }
            "artist_matching_colors" => {
                self.config.overlay.artist_matching_colors =
                    !self.config.overlay.artist_matching_colors
            }
            "artist_glow_invert_colors" => {
                self.config.overlay.artist_glow_invert_colors =
                    !self.config.overlay.artist_glow_invert_colors
            }
            "artist_glow" => self.config.overlay.artist_glow = !self.config.overlay.artist_glow,
            "artist_invert_colors" => {
                self.config.overlay.artist_invert_colors = !self.config.overlay.artist_invert_colors
            }
            "timestamps_invert_colors" => {
                self.config.overlay.timestamps_invert_colors =
                    !self.config.overlay.timestamps_invert_colors
            }
            "timestamps_glow_invert_colors" => {
                self.config.overlay.timestamps_glow_invert_colors =
                    !self.config.overlay.timestamps_glow_invert_colors
            }
            "timestamps_glow" => {
                self.config.overlay.timestamps_glow = !self.config.overlay.timestamps_glow
            }
            "background_progress_matching_colors" => {
                self.config.overlay.background_progress_matching_colors =
                    !self.config.overlay.background_progress_matching_colors
            }
            "background_progress_invert" => {
                self.config.overlay.background_progress_invert =
                    !self.config.overlay.background_progress_invert
            }
            "progress_invert_colors" => {
                self.config.overlay.progress_invert_colors =
                    !self.config.overlay.progress_invert_colors
            }
            "progress_matching_colors" => {
                self.config.overlay.progress_matching_colors =
                    !self.config.overlay.progress_matching_colors
            }
            "always_brighten_colors" => {
                self.config.overlay.always_brighten_colors =
                    !self.config.overlay.always_brighten_colors
            }
            _ => {}
        }
        self.config.overlay = self.config.overlay.clone().sanitized();
    }

    fn current_value_string(&self, key: &str) -> Option<String> {
        Some(match key {
            "music_dir" => self.config.mpd.music_dir.clone(),
            "ip" => self.config.mpd.ip.clone(),
            "port" => self.config.mpd.port.clone(),
            "use_cover" => self.config.mpd.use_cover.to_string(),
            "width" => self.config.overlay.width.to_string(),
            "height" => self.config.overlay.height.to_string(),
            "margin" => self.config.overlay.margin.to_string(),
            "padding" => self.config.overlay.padding.to_string(),
            "corner_radius" => self.config.overlay.corner_radius.to_string(),
            "cover_radius" => self.config.overlay.cover_radius.to_string(),
            "anchor" => self.config.overlay.anchor.clone(),
            "offset_x" => self.config.overlay.offset_x.to_string(),
            "offset_y" => self.config.overlay.offset_y.to_string(),
            "monitor" => self.config.overlay.monitor.clone(),
            "timeout_ms" => self.config.overlay.timeout_ms.to_string(),
            "show_on_pause" => self.config.overlay.show_on_pause.to_string(),
            "show_on_progress" => self.config.overlay.show_on_progress.clone(),
            "fade_ms" => self.config.overlay.fade_ms.to_string(),
            "scroll_ms" => self.config.overlay.scroll_ms.to_string(),
            "progress" => self.config.overlay.progress.clone(),
            "progress_start_color" => self.config.overlay.progress_start_color.clone(),
            "progress_end_color" => self.config.overlay.progress_end_color.clone(),
            "progress_bar_blend" => self.config.overlay.progress_bar_blend.to_string(),
            "center_text" => self.config.overlay.center_text.to_string(),
            "background" => self.config.overlay.background.clone(),
            "background_color" => self.config.overlay.background_color.clone(),
            "background_start_color" => self.config.overlay.background_start_color.clone(),
            "background_end_color" => self.config.overlay.background_end_color.clone(),
            "background_gradient_direction" => {
                self.config.overlay.background_gradient_direction.clone()
            }
            "background_blend" => self.config.overlay.background_blend.to_string(),
            "border" => self.config.overlay.border.clone(),
            "fallback_color" => self.config.overlay.fallback_color.clone(),
            "background_alpha" => self.config.overlay.background_alpha.to_string(),
            "text_alpha" => self.config.overlay.text_alpha.to_string(),
            "progress_alpha" => self.config.overlay.progress_alpha.to_string(),
            "cover_alpha" => self.config.overlay.cover_alpha.to_string(),
            "timestamps_matching_colors" => {
                self.config.overlay.timestamps_matching_colors.to_string()
            }
            "timestamps_invert_colors" => self.config.overlay.timestamps_invert_colors.to_string(),
            "timestamps_glow_invert_colors" => self
                .config
                .overlay
                .timestamps_glow_invert_colors
                .to_string(),
            "timestamps_glow" => self.config.overlay.timestamps_glow.to_string(),
            "title_matching_colors" => self.config.overlay.title_matching_colors.to_string(),
            "title_invert_colors" => self.config.overlay.title_invert_colors.to_string(),
            "title_glow_invert_colors" => self.config.overlay.title_glow_invert_colors.to_string(),
            "title_glow" => self.config.overlay.title_glow.to_string(),
            "title_color" => self.config.overlay.title_color.clone(),
            "artist_matching_colors" => self.config.overlay.artist_matching_colors.to_string(),
            "artist_invert_colors" => self.config.overlay.artist_invert_colors.to_string(),
            "artist_glow_invert_colors" => {
                self.config.overlay.artist_glow_invert_colors.to_string()
            }
            "artist_glow" => self.config.overlay.artist_glow.to_string(),
            "artist_color" => self.config.overlay.artist_color.clone(),
            "time_color" => self.config.overlay.time_color.clone(),
            "shadow_color" => self.config.overlay.shadow_color.clone(),
            "background_progress_matching_colors" => self
                .config
                .overlay
                .background_progress_matching_colors
                .to_string(),
            "background_progress_invert" => {
                self.config.overlay.background_progress_invert.to_string()
            }
            "progress_matching_colors" => self.config.overlay.progress_matching_colors.to_string(),
            "progress_invert_colors" => self.config.overlay.progress_invert_colors.to_string(),
            "always_brighten_colors" => self.config.overlay.always_brighten_colors.to_string(),
            _ => String::new(),
        })
    }

    fn apply_text(&mut self, key: &'static str, value: &str) -> Result<(), String> {
        match key {
            "music_dir" => self.config.mpd.music_dir = value.to_string(),
            "ip" => self.config.mpd.ip = value.to_string(),
            "port" => self.config.mpd.port = value.to_string(),
            "use_cover" => {
                self.config.mpd.use_cover = value.parse().map_err(|_| "boolean".to_string())?
            }
            "width" => {
                self.config.overlay.width = value.parse().map_err(|_| "number".to_string())?
            }
            "height" => {
                self.config.overlay.height = value.parse().map_err(|_| "number".to_string())?
            }
            "margin" => {
                self.config.overlay.margin = value.parse().map_err(|_| "number".to_string())?
            }
            "padding" => {
                self.config.overlay.padding = value.parse().map_err(|_| "number".to_string())?
            }
            "corner_radius" => {
                self.config.overlay.corner_radius =
                    value.parse().map_err(|_| "number".to_string())?
            }
            "cover_radius" => {
                self.config.overlay.cover_radius =
                    value.parse().map_err(|_| "number".to_string())?
            }
            "anchor" => self.config.overlay.anchor = value.to_string(),
            "offset_x" => {
                self.config.overlay.offset_x = value.parse().map_err(|_| "number".to_string())?
            }
            "offset_y" => {
                self.config.overlay.offset_y = value.parse().map_err(|_| "number".to_string())?
            }
            "monitor" => self.config.overlay.monitor = value.to_string(),
            "timeout_ms" => {
                self.config.overlay.timeout_ms = value.parse().map_err(|_| "number".to_string())?
            }
            "show_on_pause" => {
                self.config.overlay.show_on_pause =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "show_on_progress" => self.config.overlay.show_on_progress = value.to_string(),
            "fade_ms" => {
                self.config.overlay.fade_ms = value.parse().map_err(|_| "number".to_string())?
            }
            "scroll_ms" => {
                self.config.overlay.scroll_ms = value.parse().map_err(|_| "number".to_string())?
            }
            "progress" => self.config.overlay.progress = value.to_string(),
            "progress_start_color" => self.config.overlay.progress_start_color = value.to_string(),
            "progress_end_color" => self.config.overlay.progress_end_color = value.to_string(),
            "progress_bar_blend" => {
                self.config.overlay.progress_bar_blend =
                    value.parse().map_err(|_| "number".to_string())?
            }
            "center_text" => {
                self.config.overlay.center_text =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "background" => self.config.overlay.background = value.to_string(),
            "background_color" => self.config.overlay.background_color = value.to_string(),
            "background_start_color" => {
                self.config.overlay.background_start_color = value.to_string()
            }
            "background_end_color" => self.config.overlay.background_end_color = value.to_string(),
            "background_gradient_direction" => {
                self.config.overlay.background_gradient_direction = value.to_string()
            }
            "background_blend" => {
                self.config.overlay.background_blend =
                    value.parse().map_err(|_| "number".to_string())?
            }
            "border" => self.config.overlay.border = value.to_string(),
            "fallback_color" => self.config.overlay.fallback_color = value.to_string(),
            "background_alpha" => {
                self.config.overlay.background_alpha =
                    value.parse().map_err(|_| "number".to_string())?
            }
            "text_alpha" => {
                self.config.overlay.text_alpha = value.parse().map_err(|_| "number".to_string())?
            }
            "progress_alpha" => {
                self.config.overlay.progress_alpha =
                    value.parse().map_err(|_| "number".to_string())?
            }
            "cover_alpha" => {
                self.config.overlay.cover_alpha = value.parse().map_err(|_| "number".to_string())?
            }
            "timestamps_matching_colors" => {
                self.config.overlay.timestamps_matching_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "timestamps_invert_colors" => {
                self.config.overlay.timestamps_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "timestamps_glow_invert_colors" => {
                self.config.overlay.timestamps_glow_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "timestamps_glow" => {
                self.config.overlay.timestamps_glow =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "title_matching_colors" => {
                self.config.overlay.title_matching_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "title_invert_colors" => {
                self.config.overlay.title_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "title_glow_invert_colors" => {
                self.config.overlay.title_glow_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "title_glow" => {
                self.config.overlay.title_glow = value.parse().map_err(|_| "boolean".to_string())?
            }
            "title_color" => self.config.overlay.title_color = value.to_string(),
            "artist_matching_colors" => {
                self.config.overlay.artist_matching_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "artist_invert_colors" => {
                self.config.overlay.artist_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "artist_glow_invert_colors" => {
                self.config.overlay.artist_glow_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "artist_glow" => {
                self.config.overlay.artist_glow =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "artist_color" => self.config.overlay.artist_color = value.to_string(),
            "time_color" => self.config.overlay.time_color = value.to_string(),
            "shadow_color" => self.config.overlay.shadow_color = value.to_string(),
            "background_progress_matching_colors" => {
                self.config.overlay.background_progress_matching_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "background_progress_invert" => {
                self.config.overlay.background_progress_invert =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "progress_matching_colors" => {
                self.config.overlay.progress_matching_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "progress_invert_colors" => {
                self.config.overlay.progress_invert_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            "always_brighten_colors" => {
                self.config.overlay.always_brighten_colors =
                    value.parse().map_err(|_| "boolean".to_string())?
            }
            _ => return Err("unknown field".to_string()),
        }
        self.config.overlay = self.config.overlay.clone().sanitized();
        self.dirty = true;
        self.status = "Value updated.".to_string();
        Ok(())
    }

    fn save(&mut self) -> Result<(), String> {
        self.config.overlay = self.config.overlay.clone().sanitized();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = toml::to_string_pretty(&self.config).map_err(|error| error.to_string())?;
        fs::write(&self.path, text).map_err(|error| error.to_string())?;
        self.dirty = false;
        self.status = format!("Saved {}", self.path.display());
        Ok(())
    }

    fn value_span(&self, field: &Field) -> ratatui::text::Span<'static> {
        let key = field.key;
        match field.kind {
            FieldKind::Bool => {
                let value = match key {
                    "use_cover" => self.config.mpd.use_cover,
                    "show_on_pause" => self.config.overlay.show_on_pause,
                    "center_text" => self.config.overlay.center_text,
                    "timestamps_matching_colors" => self.config.overlay.timestamps_matching_colors,
                    "timestamps_invert_colors" => self.config.overlay.timestamps_invert_colors,
                    "timestamps_glow_invert_colors" => {
                        self.config.overlay.timestamps_glow_invert_colors
                    }
                    "timestamps_glow" => self.config.overlay.timestamps_glow,
                    "title_matching_colors" => self.config.overlay.title_matching_colors,
                    "title_invert_colors" => self.config.overlay.title_invert_colors,
                    "title_glow_invert_colors" => self.config.overlay.title_glow_invert_colors,
                    "title_glow" => self.config.overlay.title_glow,
                    "artist_matching_colors" => self.config.overlay.artist_matching_colors,
                    "artist_invert_colors" => self.config.overlay.artist_invert_colors,
                    "artist_glow_invert_colors" => self.config.overlay.artist_glow_invert_colors,
                    "artist_glow" => self.config.overlay.artist_glow,
                    "background_progress_matching_colors" => {
                        self.config.overlay.background_progress_matching_colors
                    }
                    "background_progress_invert" => self.config.overlay.background_progress_invert,
                    "progress_matching_colors" => self.config.overlay.progress_matching_colors,
                    "progress_invert_colors" => self.config.overlay.progress_invert_colors,
                    "always_brighten_colors" => self.config.overlay.always_brighten_colors,
                    _ => false,
                };
                if value {
                    Span::styled(
                        "on",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(
                        "off",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )
                }
            }
            FieldKind::Integer => Span::styled(
                self.current_value_string(key).unwrap_or_default(),
                Style::default().fg(Color::Cyan),
            ),
            FieldKind::Text => {
                let text = self.current_value_string(key).unwrap_or_default();
                if text.trim().is_empty() {
                    Span::styled("(empty)", Style::default().dim().fg(Color::DarkGray))
                } else {
                    Span::styled(text, Style::default().fg(Color::Cyan))
                }
            }
        }
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Length((area.height.saturating_sub(height)) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Length((area.width.saturating_sub(width)) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = Config::config_path()?;
    let mut app = App::load(path);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = ratatui::init();

    loop {
        terminal.draw(|frame| render(&mut app, frame))?;

        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if app.mode_picker_active() {
                app.handle_picker_key(key);
                continue;
            }
            if app.mode_editor_active() {
                app.handle_editor_key(key);
                continue;
            }

            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                break;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
                KeyCode::Left => app.cycle_choice(-1),
                KeyCode::Right => app.cycle_choice(1),
                KeyCode::PageUp => app.move_selection(-5),
                KeyCode::PageDown => app.move_selection(5),
                KeyCode::Home => app.jump_first(),
                KeyCode::End => app.jump_last(),
                KeyCode::Enter | KeyCode::Char(' ') => app.activate(),
                KeyCode::Char('s') => {
                    if let Err(error) = app.save() {
                        app.status = format!("Could not save: {error}");
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    ratatui::restore();
    Ok(())
}

impl App {
    fn mode_editor_active(&self) -> bool {
        matches!(self.mode, Mode::EditText(_))
    }

    fn mode_picker_active(&self) -> bool {
        matches!(self.mode, Mode::PickList(_))
    }

    fn handle_picker_key(&mut self, key: KeyEvent) {
        if let Mode::PickList(pick) = &mut self.mode {
            match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Form;
                    self.status = "Selection cancelled.".to_string();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    pick.selected = pick.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if pick.selected + 1 < pick.choices.len() {
                        pick.selected += 1;
                    }
                }
                KeyCode::Enter => {
                    let key = pick.key;
                    let value = pick.choices[pick.selected].clone();
                    self.mode = Mode::Form;
                    let _ = self.apply_text(key, &value);
                }
                _ => {}
            }
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        if let Mode::EditText(edit) = &mut self.mode {
            match key.code {
                KeyCode::Esc => {
                    self.cancel_edit();
                }
                KeyCode::Enter => {
                    self.commit_edit();
                }
                KeyCode::Backspace => {
                    if edit.cursor > 0 {
                        edit.cursor -= 1;
                        let byte = edit
                            .buffer
                            .char_indices()
                            .nth(edit.cursor)
                            .map(|x| x.0)
                            .unwrap_or(edit.buffer.len());
                        edit.buffer.remove(byte);
                    }
                }
                KeyCode::Delete => {
                    if edit.cursor < edit.buffer.chars().count() {
                        let byte = edit
                            .buffer
                            .char_indices()
                            .nth(edit.cursor)
                            .map(|x| x.0)
                            .unwrap_or(edit.buffer.len());
                        if byte < edit.buffer.len() {
                            edit.buffer.remove(byte);
                        }
                    }
                }
                KeyCode::Left => edit.cursor = edit.cursor.saturating_sub(1),
                KeyCode::Right => {
                    edit.cursor = (edit.cursor + 1).min(edit.buffer.chars().count());
                }
                KeyCode::Home => edit.cursor = 0,
                KeyCode::End => edit.cursor = edit.buffer.chars().count(),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let byte = edit
                        .buffer
                        .char_indices()
                        .nth(edit.cursor)
                        .map(|x| x.0)
                        .unwrap_or(edit.buffer.len());
                    edit.buffer.insert(byte, c);
                    edit.cursor += 1;
                }
                _ => {}
            }
        }
    }

    fn commit_edit(&mut self) {
        if let Mode::EditText(edit) = &self.mode {
            let key = edit.key;
            let buffer = edit.buffer.clone();
            if let Err(error) = self.apply_text(key, &buffer) {
                self.status = format!("Invalid value ({error})");
            }
            self.mode = Mode::Form;
        }
    }

    fn cancel_edit(&mut self) {
        self.mode = Mode::Form;
        self.status = "Edit cancelled.".to_string();
    }
}

fn render(app: &mut App, frame: &mut Frame) {
    app.normalize_selection();
    let footer_height = 6;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(footer_height),
        ])
        .split(frame.area());

    let modified = if app.dirty { " (modified)" } else { "" };
    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            "mpd-overlay configuration",
            Style::default().bold(),
        )),
        Line::from(format!("{}{}", app.path.display(), modified)).dim(),
    ])
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, chunks[0]);

    let entries = app.entries();
    let mut items = Vec::new();
    for entry in entries.iter() {
        match entry {
            Entry::Section(title) => {
                items.push(ListItem::new(
                    Line::from(format!("-- {title} --"))
                        .bold()
                        .fg(Color::Yellow),
                ));
            }
            Entry::Field(index) => {
                let field = &FIELDS[*index];
                let value = app.value_span(field);
                items.push(ListItem::new(Line::from(vec![
                    Span::raw(format!("{:<28}", field.label)),
                    Span::raw("  "),
                    value,
                    Span::raw("   "),
                    Span::styled(field.help, Style::default().fg(Color::DarkGray)),
                ])));
            }
        }
    }

    // Keep the highlighted row vertically centered; the list scrolls behind it.
    let list_area = chunks[1];
    let visible_rows = list_area.height.saturating_sub(2) as usize; // minus borders
    let half_view = visible_rows / 2;
    let max_offset = entries.len().saturating_sub(visible_rows);
    let offset = app.selected.saturating_sub(half_view).min(max_offset);

    let mut state = ListState::default();
    state.select(Some(app.selected));
    *state.offset_mut() = offset;

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Settings"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, list_area, &mut state);

    match &app.mode {
        Mode::EditText(edit) => render_editor(frame, edit, list_area),
        Mode::PickList(pick) => render_picklist(frame, pick, list_area),
        Mode::Form => {}
    }

    let mut footer_lines = Vec::new();
    if let Some(key) = app.field_key() {
        if key == "music_dir" || key == "ip" || key == "port" {
            footer_lines.push(Line::from(Span::styled(
                "Text field: Enter to edit, Enter inside the editor to commit, Esc to cancel.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    footer_lines.push(Line::from(
        "Up/Down or j/k select   PageUp/PageDown move   Space toggles   Enter edits or commits   s saves   q quits   Ctrl+C exits",
    ).dim());
    footer_lines.push(Line::from(app.status.clone()));
    let footer = Paragraph::new(footer_lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, chunks[2]);
}

fn render_editor(frame: &mut Frame, edit: &TextEdit, area: Rect) {
    let label = FIELDS
        .iter()
        .find(|f| f.key == edit.key)
        .map(|f| f.label)
        .unwrap_or(edit.key);

    let requested_width = (edit.buffer.chars().count() as u16 + 10).max(40);
    let width = requested_width.min(area.width.saturating_sub(4).max(10));
    let popup = centered_rect(area, width, 3);

    let prefix: String = edit.buffer.chars().take(edit.cursor).collect();
    let suffix: String = edit.buffer.chars().skip(edit.cursor).collect();
    let line = Line::from(vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan)),
        Span::styled(suffix, Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Editing: {label}")),
        ),
        popup,
    );
    let cursor_x = popup.x + 1 + (edit.cursor.min(u16::MAX as usize) as u16);
    let cursor_y = popup.y + 1;
    frame.set_cursor_position((cursor_x, cursor_y));
}

fn render_picklist(frame: &mut Frame, pick: &PickList, area: Rect) {
    let items: Vec<ListItem> = pick
        .choices
        .iter()
        .map(|choice| ListItem::new(Line::from(choice.clone())))
        .collect();

    let max_len = pick
        .choices
        .iter()
        .map(|c| c.chars().count())
        .max()
        .unwrap_or(20);
    let width = ((max_len + 12) as u16)
        .max(30)
        .min(area.width.saturating_sub(4).max(10));
    let height = ((pick.choices.len() as u16) + 2).min(area.height.saturating_sub(4).max(3));
    let popup = centered_rect(area, width, height);

    frame.render_widget(Clear, popup);

    let mut state = ListState::default();
    state.select(Some(pick.selected));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Pick a value (Enter to confirm, Esc to cancel)"),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, popup, &mut state);
}

fn detect_monitors() -> Vec<String> {
    let Ok(output) = std::process::Command::new("xrandr")
        .arg("--listmonitors")
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(name) = trimmed.split_whitespace().last() {
            names.push(name.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_toggle_bool_by_navigation() {
        let path = std::env::temp_dir().join("mpd-tui-nav-test.toml");
        let mut app = App::load(path);
        let original = app.config.overlay.title_matching_colors;
        let target = app
        .entries()
        .iter()
        .position(|entry| {
            matches!(entry, Entry::Field(i) if FIELDS[*i].key == "title_matching_colors")
        })
        .unwrap();
        while app.selected < target {
            app.move_selection(1);
        }
        app.normalize_selection();
        app.activate();
        assert_ne!(app.config.overlay.title_matching_colors, original);
    }

    #[test]
    fn tui_canonicalizes_show_on_progress() {
        let path = std::env::temp_dir().join("mpd-tui-progress-test.toml");
        let mut app = App::load(path);
        app.apply_text("show_on_progress", "25, 50, 75").unwrap();
        assert_eq!(
            app.current_value_string("show_on_progress").as_deref(),
            Some("25,50,75")
        );
    }
}
