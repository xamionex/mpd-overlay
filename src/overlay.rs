use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gtk::prelude::*;
use gtk::{cairo, gdk, gdk_pixbuf::Pixbuf, glib};

use crate::{
    color::{self, parse_hex, Rgb},
    config::OverlayConfig,
    mpd_watcher::Update,
    render::{self, Frame},
    song_info::NowPlaying,
};

const TICK_INTERVAL: Duration = Duration::from_millis(16);

/// Redraw interval while nothing moves quickly: only the progress bar and clock.
const IDLE_REDRAW: Duration = Duration::from_millis(200);
/// Redraw interval while a long title or artist scrolls.
const MARQUEE_REDRAW: Duration = Duration::from_millis(33);
/// Cover art fades in over this long when it arrives after the card is already up.
const COVER_REVEAL: Duration = Duration::from_millis(200);

struct Inner {
    config: OverlayConfig,
    window: gtk::Window,
    area: gtk::DrawingArea,
    now: Option<NowPlaying>,
    /// The cover scaled once to the size it is drawn at (see `scale_for_cover`).
    cover: Option<Pixbuf>,
    /// The decoded cover before that scaling, kept so a config change can rescale it.
    cover_source: Option<Pixbuf>,
    cover_shown_at: Option<Instant>,
    cover_ptr: Option<usize>,
    last_draw: Instant,
    accent: Rgb,
    /// Plain average of the current cover, before any brightening, so the accent can be re-derived when the config changes.
    average_color: Option<Rgb>,
    fallback: Rgb,
    opacity: f64,
    fade_from: f64,
    fade_to: f64,
    fade_started: Instant,
    visible_until: Option<Instant>,
    received_at: Option<Instant>,
    ticker_running: bool,
    initialized: bool,
    last_key: Option<String>,
    last_state: Option<mpd::State>,
    pending_now: Option<NowPlaying>,
    transitioning: bool,
}

#[derive(Clone)]
pub struct Overlay {
    inner: Rc<RefCell<Inner>>,
}

impl Overlay {
    pub fn new(config: OverlayConfig) -> Self {
        let x11 = std::env::var_os("DISPLAY").is_some();
        if !x11 {
            eprintln!(
                "DISPLAY is not set. The overlay needs X11 or XWayland to stack above fullscreen windows."
            );
        }

        let fallback = parse_hex(&config.fallback_color, Rgb::from_rgb8(44, 44, 44));
        let (window_width, window_height) = window_size(&config);

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("mpd-overlay");
        window.set_widget_name("mpd-overlay");
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.set_accept_focus(false);
        window.set_focus_on_map(false);
        window.set_keep_above(true);
        window.set_type_hint(gdk::WindowTypeHint::Notification);
        window.set_app_paintable(true);
        window.set_gravity(gdk::Gravity::NorthWest);
        window.set_default_size(window_width, window_height);
        window.set_size_request(window_width, window_height);
        window.stick();

        if let Some(screen) = gdk::Screen::default() {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            } else {
                eprintln!("No RGBA visual is available. The overlay may have a solid background.");
            }

            let provider = gtk::CssProvider::new();
            if let Err(error) = provider.load_from_data(
                b"#mpd-overlay, #mpd-overlay drawingarea { background-color: transparent; background-image: none; }",
            ) {
                eprintln!("Could not load overlay CSS: {error}");
            }
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let area = gtk::DrawingArea::new();
        area.set_widget_name("mpd-overlay-area");
        area.set_app_paintable(true);
        area.set_can_focus(false);
        area.set_size_request(window_width, window_height);
        window.add(&area);

        let inner = Rc::new(RefCell::new(Inner {
            config: config.clone(),
                                         window: window.clone(),
                                         area: area.clone(),
                                         now: None,
                                         cover: None,
                                         cover_source: None,
                                         cover_shown_at: None,
                                         cover_ptr: None,
                                         last_draw: Instant::now(),
                                         accent: fallback,
                                         average_color: None,
                                         fallback,
                                         opacity: 0.0,
                                         fade_from: 0.0,
                                         fade_to: 0.0,
                                         fade_started: Instant::now(),
                                         visible_until: None,
                                         received_at: None,
                                         ticker_running: false,
                                         initialized: false,
                                         last_key: None,
                                         last_state: None,
                                         pending_now: None,
                                         transitioning: false,
        }));

        {
            let inner = Rc::clone(&inner);
            area.connect_draw(move |_, cr| {
                let inner = inner.borrow();
                let elapsed = inner
                .now
                .as_ref()
                .and_then(|now| smooth_elapsed(now, inner.received_at));
                let progress = progress_for(inner.now.as_ref(), elapsed);
                let frame = Frame {
                    config: &inner.config,
                    now: inner.now.as_ref(),
                              cover: inner.cover.as_ref(),
                              accent: inner.accent,
                              opacity: inner.opacity,
                              elapsed,
                              progress,
                              cover_reveal: cover_reveal(inner.cover_shown_at),
                };
                render::draw(cr, inner.config.width, inner.config.height, &frame);
                glib::Propagation::Stop
            });
        }

        {
            let x11 = x11;
            window.connect_realize(move |window| configure_x11(window, x11));
        }
        {
            let config = config.clone();
            let x11 = x11;
            window.connect_map(move |window| {
                configure_x11(window, x11);
                position_window(window, &config);
            });
        }

        window.realize();
        position_window(&window, &config);

        Self { inner }
    }

    pub fn update_config(&self, config: crate::config::OverlayConfig) {
        let (window_width, window_height) = window_size(&config);
        let fallback = crate::color::parse_hex(
            &config.fallback_color,
            crate::color::Rgb::from_rgb8(44, 44, 44),
        );

        let mut inner = self.inner.borrow_mut();
        inner.fallback = fallback;
        inner.config = config.clone();
        // The accent depends on the config (`always_brighten_colors`), so re-derive it from the stored cover average.
        // Otherwise a change would only show up once the next cover is loaded.
        inner.accent = match inner.average_color {
            Some(average) => accent_from(average, &inner.config),
            None => fallback,
        };
        let target = cover_target_size(&inner.window, &inner.config);
        inner.cover = inner
            .cover_source
            .as_ref()
            .map(|source| scale_for_cover(source, target));
        inner.window.set_default_size(window_width, window_height);
        inner.window.set_size_request(window_width, window_height);
        inner.area.set_size_request(window_width, window_height);
        inner.window.resize(window_width, window_height);
        drop(inner);

        position_window(&self.inner.borrow().window, &config);
        self.inner.borrow().area.queue_draw();
    }

    pub fn handle(&self, update: Update) {
        match update {
            Update::NowPlaying(now_playing) => self.apply_now_playing(now_playing),
            Update::Disconnected => {
                {
                    let mut inner = self.inner.borrow_mut();
                    inner.initialized = false;
                    inner.last_key = None;
                    inner.last_state = None;
                    inner.pending_now = None;
                    inner.transitioning = false;
                }
                self.hide();
            }
        }
    }

    fn apply_now_playing(&self, now: NowPlaying) {
        let mut inner = self.inner.borrow_mut();
        let was_initialized = inner.initialized;
        let previous_key = inner.last_key.clone();
        let previous_state = inner.last_state;

        if now.state == mpd::State::Stop {
            inner.initialized = true;
            inner.last_key = now.song_key.clone();
            inner.last_state = Some(mpd::State::Stop);
            drop(inner);
            self.hide();
            return;
        }

        let song_changed = previous_key != now.song_key;
        let state_changed = previous_state != Some(now.state);
        let visible = inner.opacity > 0.0001 || inner.fade_to > 0.0;

        if !was_initialized {
            if now.state == mpd::State::Play {
                update_cover(&mut inner, &now);
                inner.now = Some(now.clone());
                inner.received_at = Some(Instant::now());
                inner.last_key = now.song_key.clone();
                inner.last_state = Some(now.state);
                inner.pending_now = None;
                inner.transitioning = false;
                inner.initialized = true;
                drop(inner);
                self.show();
            } else {
                inner.now = if now.state == mpd::State::Stop {
                    None
                } else {
                    Some(now.clone())
                };
                inner.received_at = Some(Instant::now());
                inner.last_key = now.song_key.clone();
                inner.last_state = Some(now.state);
                inner.initialized = true;
            }
            return;
        }

        if song_changed && now.state == mpd::State::Play && visible {
            inner.pending_now = Some(now.clone());
            inner.transitioning = true;
            inner.last_state = Some(now.state);
            inner.visible_until = None;

            if inner.fade_to > 0.0 {
                inner.fade_from = inner.opacity;
                inner.fade_to = 0.0;
                inner.fade_started = Instant::now();
                if inner.config.fade_ms == 0 {
                    inner.opacity = 0.0;
                }
            }
            drop(inner);
            self.ensure_ticker();
            return;
        }

        update_cover(&mut inner, &now);
        inner.now = Some(now.clone());
        inner.received_at = Some(Instant::now());
        inner.last_key = now.song_key.clone();
        inner.last_state = Some(now.state);
        inner.pending_now = None;
        inner.transitioning = false;
        inner.initialized = true;

        if song_changed || state_changed {
            drop(inner);
            self.show();
        } else if visible {
            inner.area.queue_draw();
        }
    }

    fn show(&self) {
        let mut inner = self.inner.borrow_mut();
        if inner.now.is_none() {
            return;
        }

        position_window(&inner.window, &inner.config);
        if !inner.window.is_visible() {
            inner.window.show_all();
            // GTK places a window afresh every time it is mapped, so a position set while the window was hidden can be lost.
            // Apply it again now that the window is on screen.
            position_window(&inner.window, &inner.config);
        }

        let now = Instant::now();
        inner.visible_until = if inner.config.timeout_ms == 0 {
            None
        } else {
            Some(now + Duration::from_millis(inner.config.timeout_ms))
        };
        inner.fade_from = inner.opacity;
        inner.fade_to = 1.0;
        inner.fade_started = now;
        if inner.config.fade_ms == 0 {
            inner.opacity = 1.0;
        }
        inner.area.queue_draw();

        drop(inner);
        self.ensure_ticker();
    }

    fn hide(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.visible_until = None;

        if inner.fade_to <= 0.0 {
            if inner.opacity <= 0.0001 {
                inner.window.hide();
            }
            return;
        }

        inner.fade_from = inner.opacity;
        inner.fade_to = 0.0;
        inner.fade_started = Instant::now();
        if inner.config.fade_ms == 0 {
            inner.opacity = 0.0;
            inner.window.hide();
            return;
        }

        drop(inner);
        self.ensure_ticker();
    }

    fn ensure_ticker(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.ticker_running {
                return;
            }
            inner.ticker_running = true;
        }

        let weak = Rc::downgrade(&self.inner);
        glib::timeout_add_local(TICK_INTERVAL, move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let overlay = Overlay { inner };
            if overlay.tick() {
                glib::ControlFlow::Continue
            } else {
                overlay.inner.borrow_mut().ticker_running = false;
                glib::ControlFlow::Break
            }
        });
    }

    fn tick(&self) -> bool {
        let mut inner = self.inner.borrow_mut();
        let now = Instant::now();
        let mut needs_draw = false;

        if inner.transitioning
            && inner.fade_to <= 0.0
            && inner.pending_now.is_some()
            && (inner.opacity - 0.0).abs() <= 0.001
            {
                if let Some(next) = inner.pending_now.take() {
                    let missed_key = inner.last_key != next.song_key;
                    update_cover(&mut inner, &next);
                    inner.now = Some(next.clone());
                    inner.received_at = Some(now);
                    inner.last_key = next.song_key.clone();
                    inner.last_state = Some(next.state);
                    inner.initialized = true;

                    inner.visible_until = if inner.config.timeout_ms == 0 {
                        None
                    } else {
                        Some(now + Duration::from_millis(inner.config.timeout_ms))
                    };

                    if inner.config.fade_ms == 0 {
                        inner.opacity = 1.0;
                        inner.fade_from = 1.0;
                        inner.fade_to = 1.0;
                        inner.transitioning = false;
                    } else {
                        inner.fade_from = 0.0;
                        inner.fade_to = 1.0;
                        inner.fade_started = now;
                        inner.transitioning = true;
                    }
                    needs_draw = true;
                    let _ = missed_key;
                }
            }

            if inner
                .visible_until
                .map(|until| now >= until)
                .unwrap_or(false)
                && inner.fade_to > 0.0
                {
                    inner.visible_until = None;
                    start_fade(&mut inner, 0.0, now);
                }

                if (inner.opacity - inner.fade_to).abs() > 0.0001 {
                    let duration = inner.config.fade_ms as f64 / 1000.0;
                    let t = if duration <= 0.0 {
                        1.0
                    } else {
                        ((now - inner.fade_started).as_secs_f64() / duration).clamp(0.0, 1.0)
                    };
                    let eased = t * t * (3.0 - 2.0 * t);
                    inner.opacity = inner.fade_from + (inner.fade_to - inner.fade_from) * eased;
                    needs_draw = true;
                } else if (inner.opacity - inner.fade_to).abs() > f64::EPSILON {
                    inner.opacity = inner.fade_to;
                    needs_draw = true;
                }

                if inner.opacity <= 0.0001 && inner.fade_to <= 0.0 && !inner.transitioning {
                    inner.opacity = 0.0;
                    inner.window.hide();
                    return false;
                }

                if inner.transitioning && (inner.opacity - 1.0).abs() <= 0.001 && inner.fade_to >= 0.999 {
                    inner.transitioning = false;
                    inner.pending_now = None;
                    needs_draw = true;
                }

                // Fades always redraw every tick (handled above).
                // Otherwise redraw only as often as something visibly changes: quickly while a title scrolls or a cover fades in, slowly for the progress bar and clock.
                if inner.opacity > 0.0 {
                    let revealing = inner
                        .cover_shown_at
                        .map_or(false, |shown| shown.elapsed() < COVER_REVEAL);
                    let every = if render::marquee_active() {
                        MARQUEE_REDRAW
                    } else {
                        IDLE_REDRAW
                    };
                    if revealing || inner.last_draw.elapsed() >= every {
                        needs_draw = true;
                    }
                }
                if needs_draw {
                    inner.last_draw = Instant::now();
                    inner.area.queue_draw();
                }

                true
    }
}

fn start_fade(inner: &mut Inner, target: f64, now: Instant) {
    inner.fade_from = inner.opacity;
    inner.fade_to = target;
    inner.fade_started = now;
    if inner.config.fade_ms == 0 {
        inner.opacity = target;
    }
}

fn configure_x11(window: &gtk::Window, x11: bool) {
    if !x11 {
        return;
    }

    let Some(gdk_window) = window.window() else {
        return;
    };
    gdk_window.set_override_redirect(true);
    let empty = cairo::Region::create();
    gdk_window.input_shape_combine_region(&empty, 0, 0);
    gdk_window.set_opaque_region(Some(&empty));
}

fn position_window(window: &gtk::Window, config: &OverlayConfig) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let monitor = if config.monitor.eq_ignore_ascii_case("primary") {
        display.primary_monitor()
    } else {
        config
            .monitor
            .parse::<i32>()
            .ok()
            .and_then(|index| display.monitor(index))
    }
    .or_else(|| display.primary_monitor())
    .or_else(|| display.monitor(0));

    let Some(monitor) = monitor else {
        return;
    };

    let geometry = monitor.geometry();
    let (x, y) = window_origin(
        (
            geometry.x(),
            geometry.y(),
            geometry.width(),
            geometry.height(),
        ),
        config,
    );

    window.move_(x, y);
    // Also move the X window itself.
    // The overlay is override-redirect, so nothing else places it, and this works whether or not GTK has caught up yet.
    if let Some(gdk_window) = window.window() {
        gdk_window.move_(x, y);
    }
}

/// Top-left corner of the window on a monitor given as `(x, y, width, height)`.
///
/// The window is the card plus a transparent `SHADOW_PAD` border on every side, but `margin` is the distance from the monitor edge to the card, so the window sits that much further out.
/// Only the card has to stay on the monitor: the transparent border may hang over the edge.
/// (Clamping the whole window instead pushes the card back in, which made every margin below the shadow size look the same.)
fn window_origin(monitor: (i32, i32, i32, i32), config: &OverlayConfig) -> (i32, i32) {
    let (monitor_x, monitor_y, monitor_width, monitor_height) = monitor;
    let (width, height) = window_size(config);
    let pad = render::SHADOW_PAD.round() as i32;
    let anchor = config.anchor.as_str();

    // Horizontal: `-left` / `-right` suffixes pin to an edge, otherwise center.
    let base_x = if anchor.ends_with("left") {
        monitor_x + config.margin - pad
    } else if anchor.ends_with("right") {
        monitor_x + monitor_width - width - config.margin + pad
    } else {
        monitor_x + (monitor_width - width) / 2
    };

    // Vertical: `top` / `bottom` prefixes pin to an edge, `center` centers.
    let base_y = if anchor.starts_with("top") {
        monitor_y + config.margin - pad
    } else if anchor.starts_with("bottom") {
        monitor_y + monitor_height - height - config.margin + pad
    } else {
        monitor_y + (monitor_height - height) / 2
    };

    let x = base_x + config.offset_x;
    let y = base_y + config.offset_y;

    let min_x = monitor_x - pad;
    let max_x = (monitor_x + monitor_width - width + pad).max(min_x);
    let min_y = monitor_y - pad;
    let max_y = (monitor_y + monitor_height - height + pad).max(min_y);

    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
}

fn window_size(config: &OverlayConfig) -> (i32, i32) {
    let shadow_pad = (render::SHADOW_PAD * 2.0).round() as i32;
    (config.width + shadow_pad, config.height + shadow_pad)
}

fn smooth_elapsed(now: &NowPlaying, received_at: Option<Instant>) -> Option<Duration> {
    let elapsed = now.elapsed?;
    if now.state != mpd::State::Play {
        return Some(elapsed);
    }

    let extra = received_at
    .map(|received_at| received_at.elapsed())
    .unwrap_or_default();
    let total = elapsed + extra;

    Some(match now.duration {
        Some(duration) if !duration.is_zero() => total.min(duration),
         _ => total,
    })
}

fn progress_for(now: Option<&NowPlaying>, elapsed: Option<Duration>) -> f64 {
    let Some(duration) = now.and_then(|now| now.duration) else {
        return 0.0;
    };
    if duration.is_zero() {
        return 0.0;
    }

    elapsed
    .map(|elapsed| (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0))
    .unwrap_or(0.0)
}

fn update_cover(inner: &mut Inner, now: &NowPlaying) {
    let cover_ptr = now.cover.as_ref().map(|cover| Arc::as_ptr(cover) as usize);
    if inner.cover_ptr == cover_ptr {
        return;
    }
    inner.cover_ptr = cover_ptr;

    // The image was decoded and shrunk on the watcher's worker thread; all that is left here is wrapping it and scaling it once to the size it will be drawn at.
    let Some(image) = now.cover.as_deref() else {
        inner.cover = None;
        inner.cover_source = None;
        inner.cover_shown_at = None;
        inner.average_color = None;
        inner.accent = inner.fallback;
        return;
    };

    let source = image.to_pixbuf();
    let target = cover_target_size(&inner.window, &inner.config);
    inner.cover = Some(scale_for_cover(&source, target));
    inner.cover_source = Some(source);
    inner.cover_shown_at = Some(Instant::now());
    inner.average_color = Some(image.average);
    inner.accent = accent_from(image.average, &inner.config);
}

/// Accent color for a cover with this average color.
/// With `always_brighten_colors` every cover is brightened; otherwise only mid-brightness ones are.
fn accent_from(average: Rgb, config: &OverlayConfig) -> Rgb {
    if config.always_brighten_colors {
        color::brighten(average)
    } else {
        color::brighten_for_overlay(average)
    }
}

/// Side of the cover slot in device pixels (the window may be scaled for HiDPI).
fn cover_target_size(window: &gtk::Window, config: &OverlayConfig) -> f64 {
    render::cover_size(config) * window.scale_factor().max(1) as f64
}

/// Scales `source` down once so its shorter side matches the cover slot (the card crops the longer side).
/// Drawing a multi-megapixel pixbuf every frame is what made the overlay expensive; a slot-sized one costs next to nothing.
fn scale_for_cover(source: &Pixbuf, size: f64) -> Pixbuf {
    let width = source.width() as f64;
    let height = source.height() as f64;
    if width <= 0.0 || height <= 0.0 {
        return source.clone();
    }

    let scale = (size / width).max(size / height);
    if scale >= 1.0 {
        return source.clone();
    }
    source
        .scale_simple(
            ((width * scale).round() as i32).max(1),
            ((height * scale).round() as i32).max(1),
            gtk::gdk_pixbuf::InterpType::Bilinear,
        )
        .unwrap_or_else(|| source.clone())
}

/// Goes from 0 to 1 over `COVER_REVEAL` after a cover is applied, so art that arrives late fades in rather than popping.
fn cover_reveal(shown_at: Option<Instant>) -> f64 {
    match shown_at {
        Some(shown_at) => {
            (shown_at.elapsed().as_secs_f64() / COVER_REVEAL.as_secs_f64()).clamp(0.0, 1.0)
        }
        None => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_brighten_changes_the_accent_even_for_dark_art() {
        let dark = Rgb::from_rgb8(20, 20, 20);

        let off = OverlayConfig::default();
        assert_eq!(accent_from(dark, &off), dark);

        let on = OverlayConfig {
            always_brighten_colors: true,
            ..OverlayConfig::default()
        };
        assert!(accent_from(dark, &on).brightness() > dark.brightness());
    }

    const MONITOR: (i32, i32, i32, i32) = (0, 0, 1920, 1080);
    const SECOND_MONITOR: (i32, i32, i32, i32) = (1920, 0, 2560, 1440);

    fn anchored(anchor: &str, margin: i32) -> OverlayConfig {
        OverlayConfig {
            anchor: anchor.to_string(),
            margin,
            ..OverlayConfig::default()
        }
    }

    /// Where the card itself (not the window around it) ends up: `(x, y, w, h)`.
    fn card_on(monitor: (i32, i32, i32, i32), config: &OverlayConfig) -> (i32, i32, i32, i32) {
        let (x, y) = window_origin(monitor, config);
        let pad = render::SHADOW_PAD.round() as i32;
        (x + pad, y + pad, config.width, config.height)
    }

    #[test]
    fn zero_margin_touches_the_anchored_edges() {
        let (x, y, _, _) = card_on(MONITOR, &anchored("top-left", 0));
        assert_eq!((x, y), (0, 0));

        let (x, y, width, _) = card_on(MONITOR, &anchored("top-right", 0));
        assert_eq!((x + width, y), (1920, 0));

        let (x, y, _, height) = card_on(MONITOR, &anchored("bottom-left", 0));
        assert_eq!((x, y + height), (0, 1080));

        let (x, y, width, height) = card_on(MONITOR, &anchored("bottom-right", 0));
        assert_eq!((x + width, y + height), (1920, 1080));
    }

    #[test]
    fn every_margin_is_honored_including_ones_below_the_shadow_size() {
        // Margins under the 14px shadow border used to all collapse to 14.
        for margin in 0..=40 {
            let (x, y, _, _) = card_on(MONITOR, &anchored("top-left", margin));
            assert_eq!((x, y), (margin, margin));

            let (x, y, width, height) = card_on(MONITOR, &anchored("bottom-right", margin));
            assert_eq!((x + width, y + height), (1920 - margin, 1080 - margin));
        }
    }

    #[test]
    fn margin_is_measured_from_the_chosen_monitor() {
        let (x, y, _, _) = card_on(SECOND_MONITOR, &anchored("top-left", 7));
        assert_eq!((x, y), (1927, 7));
    }

    #[test]
    fn offsets_move_the_card_but_never_off_the_monitor() {
        let config = OverlayConfig {
            offset_x: -500,
            offset_y: -500,
            ..anchored("top-left", 10)
        };
        let (x, y, _, _) = card_on(MONITOR, &config);
        assert_eq!((x, y), (0, 0));
    }
}
