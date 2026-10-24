use std::cell::{Cell, RefCell};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::gdk::prelude::GdkContextExt;
use gtk::{
    cairo::{Context, Format, ImageSurface, LinearGradient},
    gdk_pixbuf::Pixbuf,
    pango,
};
use pangocairo::functions as pangocairo;

use crate::{
    color::{parse_hex, Rgb},
    config::OverlayConfig,
    song_info::NowPlaying,
};

pub const SHADOW_PAD: f64 = 14.0;
const SHADOW_LAYERS: usize = 12;

/// Two-layer drop shadow underneath the text.
/// Kept intentionally small so it adds contrast without the dense halo that used to make text unreadable.
const TEXT_SHADOW_OFFSETS: [(f64, f64, f64); 2] = [(1.0, 1.0, 0.55), (2.0, 2.0, 0.30)];

/// Room kept above and below a scrolling text line so its glow isn't cut off flat.
const MARQUEE_FX_MARGIN: f64 = 16.0;

/// Width of the start to end color transition, as a fraction of the bar, at a blend setting of 100.
/// Smaller settings scale linearly down to 0 (a hard edge).
const MAX_BLEND_WIDTH: f64 = 0.25;

/// Tight black/white halo drawn between the glow and the text.
/// The original petarwidget.html stacks four 4-7px text-shadows in black or white on top of the wide glow so the glyph edges stay readable.
/// This is the same idea, kept slight.
const HALO_RADIUS: f64 = 4.5;
const HALO_SAMPLES: usize = 32;
const HALO_ALPHA: f64 = 0.40;

pub struct Frame<'a> {
    pub config: &'a OverlayConfig,
    pub now: Option<&'a NowPlaying>,
    pub cover: Option<&'a Pixbuf>,
    pub accent: Rgb,
    pub opacity: f64,
    pub elapsed: Option<Duration>,
    pub progress: f64,
    /// Goes from 0 to 1 while freshly arrived cover art fades in.
    pub cover_reveal: f64,
}

/// Side of the square cover slot, from the card size and padding.
pub fn cover_size(config: &OverlayConfig) -> f64 {
    let padding = config.padding as f64;
    (config.height as f64 - padding * 2.0)
        .min(config.width as f64 * 0.45)
        .max(1.0)
}

/// True when the last frame drew a scrolling title or artist, i.e. the overlay has to keep redrawing at a steady rate instead of only occasionally.
pub fn marquee_active() -> bool {
    MARQUEE_ACTIVE.with(|active| active.get())
}

/// Room around a text sprite for the glow (10px), halo (4.5px) and drop shadow.
const SPRITE_MARGIN: f64 = 16.0;
const SPRITE_CACHE_LIMIT: usize = 24;

/// A text line with its glow, halo and shadow already painted.
/// Building one costs about as much as a single frame used to; after that each frame is one blit.
struct Sprite {
    surface: ImageSurface,
    last_used: u64,
}

thread_local! {
    static MARQUEE_ACTIVE: Cell<bool> = Cell::new(false);
    static DRAW_COUNT: Cell<u64> = Cell::new(0);
    static SPRITES: RefCell<HashMap<u64, Sprite>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn right(self) -> f64 {
        self.x + self.width
    }
}

pub fn draw(cr: &Context, _width: i32, _height: i32, frame: &Frame) {
    MARQUEE_ACTIVE.with(|active| active.set(false));
    DRAW_COUNT.with(|count| count.set(count.get().wrapping_add(1)));

    cr.save().ok();
    cr.set_operator(gtk::cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cr.paint().ok();
    cr.restore().ok();

    if frame.opacity <= 0.001 {
        return;
    }

    let card = Rect {
        x: SHADOW_PAD,
        y: SHADOW_PAD,
        width: frame.config.width as f64,
        height: frame.config.height as f64,
    };
    let radius = frame.config.corner_radius as f64;
    let border = parse_hex(&frame.config.border, Rgb::BLACK);
    let fallback = parse_hex(&frame.config.fallback_color, Rgb::from_rgb8(44, 44, 44));

    cr.push_group();

    draw_shadow(cr, card, radius);
    cr.save().ok();
    rounded_rect(cr, card, radius);
    cr.clip();

    draw_background(cr, card, frame, fallback);
    draw_cover(cr, card, frame);
    if frame.config.shows_bottom_line() {
        draw_compact_progress(cr, card, frame);
    }

    cr.restore().ok();
    rounded_rect(cr, card, radius);
    cr.set_source_rgba(border.r, border.g, border.b, 0.95);
    cr.set_line_width(1.0);
    cr.stroke().ok();

    // Text goes on last and outside the card clip, over the border too, so small cards let glyphs and glow overflow the card edge instead of cutting them off.
    draw_info(cr, card, frame);

    cr.pop_group_to_source().ok();
    cr.paint_with_alpha(frame.opacity).ok();
}

fn draw_shadow(cr: &Context, card: Rect, radius: f64) {
    for layer in (0..SHADOW_LAYERS).rev() {
        let spread = 1.0 + layer as f64 * 1.1;
        rounded_rect(
            cr,
            Rect {
                x: card.x - spread,
                y: card.y - spread,
                width: card.width + spread * 2.0,
                height: card.height + spread * 2.0,
            },
            radius + spread,
        );
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.022);
        cr.fill().ok();
    }
}

fn draw_background(cr: &Context, card: Rect, frame: &Frame, fallback: Rgb) {
    let custom_gradient = frame.config.background.eq_ignore_ascii_case("gradient");
    let uses_progress_gradient = frame.config.background.eq_ignore_ascii_case("progress")
        && frame.config.background_progress_matching_colors
        && frame.cover.is_some();

    if !uses_progress_gradient && !custom_gradient {
        // Solid fill: `hex` uses `background_color`, `progress` falls back when there is no art to derive a colour from.
        let color = if frame.config.background.eq_ignore_ascii_case("hex") {
            parse_hex(&frame.config.background_color, fallback)
        } else {
            fallback
        };
        set_color(cr, color, 1.0);
        cr.paint_with_alpha(frame.config.background_alpha).ok();
        return;
    }

    let (color_start, color_end) = if uses_progress_gradient {
        let start = if frame.config.background_progress_invert {
            frame.accent.invert()
        } else {
            frame.accent
        };
        let end = if frame.config.background_progress_invert {
            frame.accent
        } else {
            frame.accent.invert()
        };
        (start, end)
    } else {
        (
            parse_hex(&frame.config.background_start_color, fallback),
            parse_hex(&frame.config.background_end_color, fallback),
        )
    };
    let blend = blend_width(frame.config.background_blend);
    let progress = frame.progress.clamp(0.0, 1.0);

    // With a hard edge, the ends of the track are a single flat color.
    // With a blend there is always a transition to draw, so don't shortcut.
    if blend <= 0.0001 {
        if progress <= 0.001 {
            set_color(cr, color_end, 1.0);
            cr.paint_with_alpha(frame.config.background_alpha).ok();
            return;
        }
        if progress >= 0.999 {
            set_color(cr, color_start, 1.0);
            cr.paint_with_alpha(frame.config.background_alpha).ok();
            return;
        }
    }

    let gradient = progress_gradient(card, color_start, color_end, 1.0, progress, blend);
    cr.set_source(&gradient).ok();
    cr.paint_with_alpha(frame.config.background_alpha).ok();
}

/// Width of the start to end transition as a fraction of the bar: 0 is a hard edge, 100 is `MAX_BLEND_WIDTH`, linear in between.
fn blend_width(setting: u32) -> f64 {
    setting.min(100) as f64 / 100.0 * MAX_BLEND_WIDTH
}

/// Horizontal gradient across `bar`: solid `start` up to `progress`, then fading to `end` over the next `blend` of the width.
/// Same as a CSS gradient with stops at `progress` and `progress + blend`; both are fractions of the bar width.
fn progress_gradient(
    bar: Rect,
    start: Rgb,
    end: Rgb,
    alpha: f64,
    progress: f64,
    blend: f64,
) -> LinearGradient {
    let progress = progress.clamp(0.0, 1.0);
    let gradient = LinearGradient::new(bar.x, bar.y, bar.right(), bar.y);
    gradient.add_color_stop_rgba(0.0, start.r, start.g, start.b, alpha);
    gradient.add_color_stop_rgba(progress, start.r, start.g, start.b, alpha);

    if progress + blend <= 1.0 {
        gradient.add_color_stop_rgba(progress + blend, end.r, end.g, end.b, alpha);
        gradient.add_color_stop_rgba(1.0, end.r, end.g, end.b, alpha);
    } else {
        // The transition runs past the right edge.
        // Cairo clamps stop offsets to 1.0, which would squeeze the blend into what is left of the bar.
        // Instead stop at the edge with the color the full-width transition has reached there.
        let mixed = start.mix(end, (1.0 - progress) / blend);
        gradient.add_color_stop_rgba(1.0, mixed.r, mixed.g, mixed.b, alpha);
    }
    gradient
}

fn draw_cover(cr: &Context, card: Rect, frame: &Frame) {
    let padding = frame.config.padding as f64;
    let cover_size = cover_size(frame.config);
    let cover = Rect {
        x: card.x + padding,
        y: card.y + padding,
        width: cover_size,
        height: cover_size,
    };
    let radius = frame.config.cover_radius as f64;

    cr.save().ok();
    rounded_rect(cr, cover, radius);
    cr.clip();

    match frame.cover {
        Some(pixbuf) => {
            // Art that arrives late fades in over the placeholder instead of popping.
            if frame.cover_reveal < 1.0 {
                draw_cover_placeholder(cr, cover);
            }
            draw_pixbuf_cover(
                cr,
                cover,
                pixbuf,
                frame.config.cover_alpha * frame.cover_reveal,
            );
        }
        None => draw_cover_placeholder(cr, cover),
    }

    cr.restore().ok();

    rounded_rect(cr, cover, radius);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
    cr.set_line_width(1.5);
    cr.stroke().ok();
}

fn draw_pixbuf_cover(cr: &Context, cover: Rect, pixbuf: &Pixbuf, cover_alpha: f64) {
    let pixbuf_width = pixbuf.width() as f64;
    let pixbuf_height = pixbuf.height() as f64;
    if pixbuf_width <= 0.0 || pixbuf_height <= 0.0 {
        return;
    }

    let scale = (cover.width / pixbuf_width).max(cover.height / pixbuf_height);
    let draw_width = pixbuf_width * scale;
    let draw_height = pixbuf_height * scale;
    let x = cover.x + (cover.width - draw_width) / 2.0;
    let y = cover.y + (cover.height - draw_height) / 2.0;

    cr.save().ok();
    cr.translate(x, y);
    cr.scale(scale, scale);
    cr.set_source_pixbuf(pixbuf, 0.0, 0.0);
    cr.paint_with_alpha(cover_alpha).ok();
    cr.restore().ok();
}

fn draw_cover_placeholder(cr: &Context, cover: Rect) {
    let gradient = LinearGradient::new(cover.x, cover.y, cover.x, cover.y + cover.height);
    gradient.add_color_stop_rgb(0.0, 0.14, 0.14, 0.14);
    gradient.add_color_stop_rgb(1.0, 0.22, 0.22, 0.22);
    cr.set_source(&gradient).ok();
    cr.paint().ok();

    let context = pangocairo::create_context(cr);
    let layout = make_layout(
        &context,
        "♫",
        (cover.height * 0.4).clamp(20.0, 96.0),
        pango::Weight::Heavy,
        cover.width,
        pango::Alignment::Center,
    );
    let (_, text_height) = layout.pixel_size();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
    cr.move_to(cover.x, cover.y + (cover.height - text_height as f64) / 2.0);
    pangocairo::show_layout(cr, &layout);
}

fn draw_info(cr: &Context, card: Rect, frame: &Frame) {
    let Some(now) = frame.now else {
        return;
    };

    let padding = frame.config.padding as f64;
    let cover_size = cover_size(frame.config);
    let info_x = card.x + padding * 2.0 + cover_size;
    let info_width = card.right() - padding - info_x;
    if info_width <= 16.0 {
        return;
    }

    let title_size = (card.height * 0.155).clamp(12.0, 64.0);
    let artist_size = (card.height * 0.115).clamp(10.0, 48.0);
    let time_size = (card.height * 0.07).clamp(9.0, 24.0);
    let alignment = if frame.config.center_text {
        pango::Alignment::Center
    } else {
        pango::Alignment::Left
    };
    let context = pangocairo::create_context(cr);

    let artist_text = match now.album.as_deref() {
        Some(album) if !album.is_empty() => format!("{} - {}", now.artist, album),
        _ => now.artist.clone(),
    };

    let title_layout = scrollable_layout(
        &context,
        &now.title,
        title_size,
        pango::Weight::Heavy,
        info_width,
        alignment,
    );
    let artist_layout = scrollable_layout(
        &context,
        &artist_text,
        artist_size,
        pango::Weight::Heavy,
        info_width,
        alignment,
    );

    let (_, title_height) = title_layout.pixel_size();
    let (_, artist_height) = artist_layout.pixel_size();
    let gap = (card.height * 0.015).clamp(2.0, 8.0);

    let bar_height = (card.height * 0.03).clamp(4.0, 8.0);
    let time_gap = 4.0;
    let timeline_height = if frame.config.shows_time_texts() {
        let time_layout = make_layout(
            &context,
            &format_duration(frame.elapsed.unwrap_or_default()),
            time_size,
            pango::Weight::Heavy,
            1.0,
            pango::Alignment::Left,
        );
        if frame.config.shows_progress_bar() {
            time_layout.pixel_size().1 as f64 + time_gap + bar_height
        } else {
            time_layout.pixel_size().1 as f64
        }
    } else {
        0.0
    };

    let total_height = title_height as f64
        + gap
        + artist_height as f64
        + if timeline_height > 0.0 {
            gap + timeline_height
        } else {
            0.0
        };
    let mut y = card.y + (card.height - total_height) / 2.0;
    let title_color = if frame.config.title_matching_colors && frame.cover.is_some() {
        if frame.config.title_invert_colors {
            frame.accent.invert()
        } else {
            frame.accent
        }
    } else {
        parse_hex(&frame.config.title_color, Rgb::WHITE)
    };
    let artist_color = if frame.config.artist_matching_colors && frame.cover.is_some() {
        if frame.config.artist_invert_colors {
            frame.accent.invert()
        } else {
            frame.accent
        }
    } else {
        parse_hex(&frame.config.artist_color, Rgb::WHITE)
    };
    let text_color = title_color;
    let title_glow = glow_color(
        frame,
        frame.config.title_glow,
        frame.config.title_glow_invert_colors,
    );
    let artist_glow = glow_color(
        frame,
        frame.config.artist_glow,
        frame.config.artist_glow_invert_colors,
    );
    let time_glow = glow_color(
        frame,
        frame.config.timestamps_glow,
        frame.config.timestamps_glow_invert_colors,
    );
    let shadow = parse_hex(&frame.config.shadow_color, Rgb::BLACK);
    let shadow_alpha =
        frame.config.text_alpha * crate::color::parse_color_alpha(&frame.config.shadow_color, 1.0);

    let text_alpha =
        frame.config.text_alpha * crate::color::parse_color_alpha(&frame.config.title_color, 1.0);
    draw_text(
        cr,
        &title_layout,
        info_x,
        y,
        info_width,
        title_height as f64,
        text_color,
        text_alpha,
        shadow,
        shadow_alpha,
        title_glow,
        frame.config.scroll_ms,
    );
    y += title_height as f64 + gap;
    draw_text(
        cr,
        &artist_layout,
        info_x,
        y,
        info_width,
        artist_height as f64,
        artist_color,
        text_alpha,
        shadow,
        shadow_alpha,
        artist_glow,
        frame.config.scroll_ms,
    );
    y += artist_height as f64;

    if frame.config.shows_time_texts() {
        y += gap;
        draw_timeline(
            cr,
            &context,
            info_x,
            y,
            info_width,
            time_size,
            bar_height,
            time_gap,
            frame,
            shadow,
            shadow_alpha,
            time_glow,
        );
    }
}

/// Glow color for a text element: the album-art accent (optionally inverted), or `None` when the glow is off or there is no cover to take a color from.
fn glow_color(frame: &Frame, enabled: bool, invert: bool) -> Option<Rgb> {
    if !enabled || frame.cover.is_none() {
        return None;
    }
    Some(if invert {
        frame.accent.invert()
    } else {
        frame.accent
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_timeline(
    cr: &Context,
    context: &pango::Context,
    x: f64,
    y: f64,
    width: f64,
    time_size: f64,
    bar_height: f64,
    _time_gap: f64,
    frame: &Frame,
    shadow: Rgb,
    shadow_alpha: f64,
    time_large_glow: Option<Rgb>,
) {
    let time_column = (width * 0.12).clamp(34.0, 84.0);
    let bar_gap = (width * 0.012).clamp(4.0, 10.0);
    let bar_x = x + time_column + bar_gap;
    let bar_width = (width - time_column * 2.0 - bar_gap * 2.0).max(1.0);
    let time_color = if frame.config.timestamps_matching_colors && frame.cover.is_some() {
        if frame.config.timestamps_invert_colors {
            frame.accent.invert()
        } else {
            frame.accent
        }
    } else {
        parse_hex(&frame.config.time_color, Rgb::WHITE)
    };

    // Labels are never truncated: each one stays anchored to its column edge nearest the bar and overflows the other way when it is wider than the column.
    let elapsed = label_layout(
        context,
        &format_duration(frame.elapsed.unwrap_or_default()),
        time_size,
        pango::Weight::Heavy,
    );
    let duration = label_layout(
        context,
        &format_duration(frame.now.and_then(|now| now.duration).unwrap_or_default()),
        time_size,
        pango::Weight::Heavy,
    );
    let (elapsed_width, time_height) = elapsed.pixel_size();
    let time_color_alpha =
        frame.config.text_alpha * crate::color::parse_color_alpha(&frame.config.time_color, 1.0);
    draw_layout_cached(
        cr,
        &elapsed,
        x + time_column - elapsed_width as f64,
        y,
        time_color,
        time_color_alpha,
        shadow,
        shadow_alpha,
        time_large_glow,
        true,
    );
    draw_layout_cached(
        cr,
        &duration,
        x + width - time_column,
        y,
        time_color,
        time_color_alpha,
        shadow,
        shadow_alpha,
        time_large_glow,
        true,
    );

    let bar_y = y + (time_height as f64 - bar_height) / 2.0;
    let matching_progress = frame.config.progress_matching_colors && frame.cover.is_some();
    if frame.config.shows_progress_bar() {
        rounded_rect(
            cr,
            Rect {
                x: bar_x,
                y: bar_y,
                width: bar_width,
                height: bar_height,
            },
            bar_height / 2.0,
        );
        let track_color = if matching_progress {
            frame.accent.invert()
        } else {
            Rgb::from_rgb8(96, 96, 96)
        };
        let fill_color = if matching_progress {
            if frame.config.progress_invert_colors {
                frame.accent.invert()
            } else {
                frame.accent
            }
        } else {
            parse_hex(&frame.config.progress_start_color, Rgb::WHITE)
        };

        set_color(cr, track_color, 0.45);
        cr.fill().ok();

        let filled = (bar_width * frame.progress.clamp(0.0, 1.0)).max(0.0);
        if filled > 0.0 {
            rounded_rect(
                cr,
                Rect {
                    x: bar_x,
                    y: bar_y,
                    width: filled,
                    height: bar_height,
                },
                bar_height / 2.0,
            );
            set_color(cr, fill_color, 0.95 * frame.config.progress_alpha);
            cr.fill().ok();
        }
    }
}

fn draw_compact_progress(cr: &Context, card: Rect, frame: &Frame) {
    let height = (card.height * 0.035).clamp(3.0, 6.0);
    let y = card.y + card.height - height;
    let progress = frame.progress.clamp(0.0, 1.0);
    let blend = blend_width(frame.config.progress_bar_blend);

    let matching = frame.config.progress_matching_colors && frame.cover.is_some();
    let (start_color, end_color) = if matching {
        if frame.config.progress_invert_colors {
            (frame.accent.invert(), frame.accent)
        } else {
            (frame.accent, frame.accent.invert())
        }
    } else {
        (
            parse_hex(&frame.config.progress_start_color, Rgb::WHITE),
            parse_hex(&frame.config.progress_end_color, Rgb::WHITE),
        )
    };

    let bar = Rect {
        x: card.x,
        y,
        width: card.width,
        height,
    };
    let gradient = progress_gradient(
        bar,
        start_color,
        end_color,
        frame.config.progress_alpha,
        progress,
        blend,
    );
    cr.rectangle(bar.x, bar.y, bar.width, bar.height);
    cr.set_source(&gradient).ok();
    cr.fill().ok();
}

fn scrollable_layout(
    context: &pango::Context,
    text: &str,
    pixel_size: f64,
    weight: pango::Weight,
    width: f64,
    alignment: pango::Alignment,
) -> pango::Layout {
    let layout = pango::Layout::new(context);
    let mut description = pango::FontDescription::new();
    description.set_family("Sans");
    description.set_weight(weight);
    description.set_size((pixel_size * 0.75 * pango::SCALE as f64).round() as i32);
    layout.set_font_description(Some(&description));
    layout.set_single_paragraph_mode(true);

    layout.set_width(-1);
    layout.set_ellipsize(pango::EllipsizeMode::None);
    layout.set_alignment(pango::Alignment::Left);
    layout.set_text(text);

    let text_width = layout.pixel_size().0 as f64;
    if text_width <= width + 1.0 {
        layout.set_width((width.max(1.0) * pango::SCALE as f64).round() as i32);
        layout.set_alignment(alignment);
        if width > 1.0 {
            layout.set_ellipsize(pango::EllipsizeMode::End);
        }
    }

    layout
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    cr: &Context,
    layout: &pango::Layout,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: Rgb,
    color_alpha: f64,
    shadow: Rgb,
    shadow_alpha: f64,
    large_glow: Option<Rgb>,
    scroll_ms: u64,
) {
    let (text_width, _) = layout.pixel_size();
    let text_width = text_width as f64;
    let overflows = text_width > width + 1.0;

    cr.save().ok();

    if overflows {
        MARQUEE_ACTIVE.with(|active| active.set(true));

        // A marquee needs a window to scroll through, so clip sideways to the text area.
        // Vertically keep room for the glow so it isn't cut off flat.
        // Text that fits isn't clipped at all.
        let window_y = y - MARQUEE_FX_MARGIN;
        let window_height = height + MARQUEE_FX_MARGIN * 2.0;
        cr.rectangle(x, window_y, width, window_height);
        cr.clip();

        let cycle = scroll_ms.max(500) as f64 / 1000.0;
        let unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let phase = (unix_seconds % cycle) / cycle;
        let distance = text_width + width;
        let offset = phase * distance - width;
        cr.push_group();

        draw_layout_cached(
            cr,
            layout,
            x - offset,
            y,
            color,
            color_alpha,
            shadow,
            shadow_alpha,
            large_glow,
            false,
        );

        cr.set_operator(gtk::cairo::Operator::DestIn);
        let fade = LinearGradient::new(x, y, x + width, y);
        fade.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
        fade.add_color_stop_rgba(0.12, 0.0, 0.0, 0.0, 1.0);
        fade.add_color_stop_rgba(0.88, 0.0, 0.0, 0.0, 1.0);
        fade.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
        cr.rectangle(x, window_y, width, window_height);
        cr.set_source(&fade).ok();
        cr.fill().ok();
        cr.set_operator(gtk::cairo::Operator::Over);

        cr.pop_group_to_source().ok();
        cr.paint_with_alpha(1.0).ok();
    } else {
        let (_, text_height) = layout.pixel_size();
        let y_offset = ((height - text_height as f64) / 2.0).max(0.0);
        draw_layout_cached(
            cr,
            layout,
            x,
            y + y_offset,
            color,
            color_alpha,
            shadow,
            shadow_alpha,
            large_glow,
            true,
        );
    }

    cr.restore().ok();
}

/// Draws everything that sits behind a text layout, bottom to top:
///
/// 1. the accent glow, when enabled;
/// 2. a small offset drop shadow (always);
/// 3. a tight readability halo, only when there is a glow. It is black for light text and white for dark text, like the original widget, and has to be painted over the glow: the glow runs right up to the glyph edges and would otherwise wash the text out.
#[allow(clippy::too_many_arguments)]
fn draw_text_decoration(
    cr: &Context,
    layout: &pango::Layout,
    x: f64,
    y: f64,
    text_color: Rgb,
    shadow: Rgb,
    shadow_alpha: f64,
    large_glow: Option<Rgb>,
    color_alpha: f64,
) {
    if let Some(color) = large_glow {
        let offsets = GLOW_LARGE.get_or_init(|| glow_offsets(10.0, 96, 0.22));
        cr.save().ok();
        for (dx, dy, alpha) in offsets {
            set_color(cr, color, *alpha * color_alpha);
            cr.move_to(x + dx, y + dy);
            pangocairo::show_layout(cr, layout);
        }
        cr.restore().ok();
    }

    cr.save().ok();
    for (dx, dy, alpha) in TEXT_SHADOW_OFFSETS {
        set_color(cr, shadow, shadow_alpha * alpha);
        cr.move_to(x + dx, y + dy);
        pangocairo::show_layout(cr, layout);
    }
    cr.restore().ok();

    if large_glow.is_some() {
        let halo = text_color.contrasting_extreme();
        let offsets =
            GLOW_HALO.get_or_init(|| glow_offsets(HALO_RADIUS, HALO_SAMPLES, HALO_ALPHA));
        cr.save().ok();
        for (dx, dy, alpha) in offsets {
            set_color(cr, halo, *alpha * color_alpha);
            cr.move_to(x + dx, y + dy);
            pangocairo::show_layout(cr, layout);
        }
        cr.restore().ok();
    }
}

fn glow_offsets(max_radius: f64, samples: usize, base_alpha: f64) -> Vec<(f64, f64, f64)> {
    (0..samples)
        .map(|index| {
            let t = (index as f64 + 0.5) / samples as f64;
            let radius = max_radius * t.sqrt();
            let angle = index as f64 * 2.399_963;
            let dx = radius * angle.cos();
            let dy = radius * angle.sin();
            let alpha = (1.0 - t).powf(1.5) * base_alpha;
            (dx, dy, alpha)
        })
        .collect()
}

static GLOW_LARGE: OnceLock<Vec<(f64, f64, f64)>> = OnceLock::new();
static GLOW_HALO: OnceLock<Vec<(f64, f64, f64)>> = OnceLock::new();

/// Paints a text line, using a cached sprite of its glow/halo/shadow/text when one exists and building it on first use.
/// `snap_x` rounds the horizontal position to a whole pixel (static text); scrolling text keeps sub-pixel positions to stay smooth.
#[allow(clippy::too_many_arguments)]
fn draw_layout_cached(
    cr: &Context,
    layout: &pango::Layout,
    x: f64,
    y: f64,
    color: Rgb,
    color_alpha: f64,
    shadow: Rgb,
    shadow_alpha: f64,
    glow: Option<Rgb>,
    snap_x: bool,
) {
    let key = sprite_key(layout, color, color_alpha, shadow, shadow_alpha, glow);
    let blit_x = if snap_x {
        (x - SPRITE_MARGIN).round()
    } else {
        x - SPRITE_MARGIN
    };
    let blit_y = (y - SPRITE_MARGIN).round();

    let drawn = SPRITES.with(|sprites| {
        let mut sprites = sprites.borrow_mut();
        let now = DRAW_COUNT.with(|count| count.get());

        if !sprites.contains_key(&key) {
            let Some(surface) = build_sprite(
                cr,
                layout,
                color,
                color_alpha,
                shadow,
                shadow_alpha,
                glow,
            ) else {
                return false;
            };
            if sprites.len() >= SPRITE_CACHE_LIMIT {
                // Drop sprites that haven't been drawn lately (old timestamps, the previous song's title) and keep the ones in use.
                sprites.retain(|_, sprite| sprite.last_used + 8 > now);
            }
            sprites.insert(
                key,
                Sprite {
                    surface,
                    last_used: now,
                },
            );
        }

        let Some(sprite) = sprites.get_mut(&key) else {
            return false;
        };
        sprite.last_used = now;

        cr.save().ok();
        let blitted = cr.set_source_surface(&sprite.surface, blit_x, blit_y).is_ok();
        if blitted {
            cr.paint().ok();
        }
        cr.restore().ok();
        blitted
    });

    if !drawn {
        draw_layout(
            cr,
            layout,
            x,
            y,
            color,
            color_alpha,
            shadow,
            shadow_alpha,
            glow,
        );
    }
}

/// Renders one text line with everything behind it into its own surface, at the target's device scale so HiDPI output stays sharp.
fn build_sprite(
    cr: &Context,
    layout: &pango::Layout,
    color: Rgb,
    color_alpha: f64,
    shadow: Rgb,
    shadow_alpha: f64,
    glow: Option<Rgb>,
) -> Option<ImageSurface> {
    let (text_width, text_height) = layout.pixel_size();
    let layout_width = layout.width().max(0) / pango::SCALE;
    let margin = SPRITE_MARGIN as i32;
    let width = text_width.max(layout_width) + margin * 2 + 2;
    let height = text_height + margin * 2 + 2;

    let (scale_x, scale_y) = cr.target().device_scale();
    let surface = ImageSurface::create(
        Format::ARgb32,
        (width as f64 * scale_x).ceil() as i32,
        (height as f64 * scale_y).ceil() as i32,
    )
    .ok()?;
    surface.set_device_scale(scale_x, scale_y);

    let sprite = Context::new(&surface).ok()?;
    draw_layout(
        &sprite,
        layout,
        SPRITE_MARGIN,
        SPRITE_MARGIN,
        color,
        color_alpha,
        shadow,
        shadow_alpha,
        glow,
    );
    drop(sprite);
    surface.flush();
    Some(surface)
}

/// Identifies a sprite by everything that changes how it looks.
fn sprite_key(
    layout: &pango::Layout,
    color: Rgb,
    color_alpha: f64,
    shadow: Rgb,
    shadow_alpha: f64,
    glow: Option<Rgb>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    layout.text().as_str().hash(&mut hasher);
    layout.pixel_size().hash(&mut hasher);
    layout.width().hash(&mut hasher);
    format!("{:?}{:?}", layout.alignment(), layout.ellipsize()).hash(&mut hasher);
    if let Some(description) = layout.font_description() {
        description.size().hash(&mut hasher);
        format!("{:?}", description.weight()).hash(&mut hasher);
    }
    for value in [
        color.r,
        color.g,
        color.b,
        color_alpha,
        shadow.r,
        shadow.g,
        shadow.b,
        shadow_alpha,
    ] {
        value.to_bits().hash(&mut hasher);
    }
    match glow {
        Some(glow) => {
            1_u8.hash(&mut hasher);
            for value in [glow.r, glow.g, glow.b] {
                value.to_bits().hash(&mut hasher);
            }
        }
        None => 0_u8.hash(&mut hasher),
    }
    hasher.finish()
}

/// Paints a text line directly, glow and all, with no caching.
#[allow(clippy::too_many_arguments)]
fn draw_layout(
    cr: &Context,
    layout: &pango::Layout,
    x: f64,
    y: f64,
    color: Rgb,
    color_alpha: f64,
    shadow: Rgb,
    shadow_alpha: f64,
    large_glow: Option<Rgb>,
) {
    draw_text_decoration(
        cr,
        layout,
        x,
        y,
        color,
        shadow,
        shadow_alpha,
        large_glow,
        color_alpha,
    );
    cr.save().ok();
    cr.set_source_rgba(color.r, color.g, color.b, color_alpha);
    cr.move_to(x, y);
    pangocairo::show_layout(cr, layout);
    cr.restore().ok();
}

/// Single-line layout with no width limit and no ellipsis, for labels that should overflow their column rather than be truncated.
/// Left-aligned at the draw position.
fn label_layout(
    context: &pango::Context,
    text: &str,
    pixel_size: f64,
    weight: pango::Weight,
) -> pango::Layout {
    let layout = pango::Layout::new(context);
    let mut description = pango::FontDescription::new();
    description.set_family("Sans");
    description.set_weight(weight);
    description.set_size((pixel_size * 0.75 * pango::SCALE as f64).round() as i32);
    layout.set_font_description(Some(&description));
    layout.set_single_paragraph_mode(true);
    layout.set_width(-1);
    layout.set_ellipsize(pango::EllipsizeMode::None);
    layout.set_text(text);
    layout
}

fn make_layout(
    context: &pango::Context,
    text: &str,
    pixel_size: f64,
    weight: pango::Weight,
    width: f64,
    alignment: pango::Alignment,
) -> pango::Layout {
    let layout = pango::Layout::new(context);
    let mut description = pango::FontDescription::new();
    description.set_family("Sans");
    description.set_weight(weight);
    description.set_size((pixel_size * 0.75 * pango::SCALE as f64).round() as i32);
    layout.set_font_description(Some(&description));
    layout.set_text(text);
    layout.set_width((width.max(1.0) * pango::SCALE as f64).round() as i32);
    layout.set_alignment(alignment);
    layout.set_ellipsize(pango::EllipsizeMode::End);
    layout.set_single_paragraph_mode(true);
    layout
}

fn rounded_rect(cr: &Context, rect: Rect, radius: f64) {
    let radius = radius.max(0.0).min(rect.width / 2.0).min(rect.height / 2.0);
    let right = rect.x + rect.width;
    let bottom = rect.y + rect.height;

    cr.new_sub_path();
    cr.arc(
        right - radius,
        rect.y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        right - radius,
        bottom - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        rect.x + radius,
        bottom - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        rect.x + radius,
        rect.y + radius,
        radius,
        std::f64::consts::PI,
        1.5 * std::f64::consts::PI,
    );
    cr.close_path();
}

fn set_color(cr: &Context, color: Rgb, alpha: f64) {
    cr.set_source_rgba(color.r, color.g, color.b, alpha);
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_width_scales_up_to_a_quarter() {
        assert_eq!(blend_width(0), 0.0);
        assert!((blend_width(50) - 0.125).abs() < 1e-9);
        assert!((blend_width(100) - 0.25).abs() < 1e-9);
        // Used to clamp at 10, which made every setting above it identical.
        assert!(blend_width(10) < blend_width(60));
        assert!(blend_width(60) < blend_width(100));
        assert_eq!(blend_width(1000), blend_width(100));
    }

    #[test]
    fn cover_slot_follows_card_height_and_padding() {
        // Default card: 500x100 with 15px padding gives a 70px slot (height limited).
        assert_eq!(cover_size(&OverlayConfig::default()), 70.0);

        let wide_and_tall = OverlayConfig {
            width: 400,
            height: 300,
            padding: 10,
            ..OverlayConfig::default()
        };
        // Width-limited: 45% of 400.
        assert_eq!(cover_size(&wide_and_tall), 180.0);
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0:00");
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_secs(3723)), "1:02:03");
    }
}
