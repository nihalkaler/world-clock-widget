//! Draws the whole widget into a pixel buffer. Platform independent, so the
//! same code renders the Windows widget and the PNG previews.

use crate::clock::{self, Reading};
use crate::config::{Config, Theme};
use crate::text::{draw_text, ellipsize, text_width, Align, Fonts};
use std::f32::consts::PI;
use tiny_skia::{
    Color, FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Stroke, Transform,
};

// Layout at 100% scaling, medium size. Everything is multiplied by `scale`.
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 14.0;
const COL_W: f32 = 132.0;
const CORNER: f32 = 16.0;
const LABEL_PX: f32 = 13.0;
const LABEL_BASE: f32 = 13.0;
const DIAL_TOP: f32 = 24.0;
const DIAL_D: f32 = 100.0;
const TIME_PX: f32 = 22.0;
const TIME_GAP: f32 = 31.0;
const AMPM_PX: f32 = 12.0;
const DATE_PX: f32 = 12.5;
const DATE_GAP: f32 = 20.0;
const DIFF_PX: f32 = 12.0;
const DIFF_GAP: f32 = 18.0;
const BOTTOM: f32 = 5.0;

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba8(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

struct Palette {
    panel: Color,
    border: Color,
    separator: Color,
    label: Color,
    time: Color,
    muted: Color,
    accent: Color,
    master_bg: Color,
    error: Color,
    ring: Color,
    day_face: Color,
    day_ink: Color,
    night_face: Color,
    night_ink: Color,
}

impl Palette {
    fn new(theme: Theme, opacity: f32) -> Palette {
        match theme {
            Theme::Dark => Palette {
                panel: rgba(17, 19, 25, opacity),
                border: rgba(255, 255, 255, 0.10),
                separator: rgba(255, 255, 255, 0.07),
                label: rgba(236, 239, 244, 1.0),
                time: rgba(255, 255, 255, 1.0),
                muted: rgba(164, 171, 184, 1.0),
                accent: rgba(255, 138, 91, 1.0),
                master_bg: rgba(255, 138, 91, 0.09),
                error: rgba(255, 107, 107, 1.0),
                ring: rgba(255, 255, 255, 0.16),
                day_face: rgba(244, 241, 234, 1.0),
                day_ink: rgba(30, 33, 41, 1.0),
                night_face: rgba(42, 47, 61, 1.0),
                night_ink: rgba(236, 239, 244, 1.0),
            },
            Theme::Light => Palette {
                panel: rgba(250, 250, 252, opacity),
                border: rgba(0, 0, 0, 0.08),
                separator: rgba(0, 0, 0, 0.07),
                label: rgba(30, 34, 42, 1.0),
                time: rgba(17, 19, 25, 1.0),
                muted: rgba(95, 102, 116, 1.0),
                accent: rgba(226, 96, 52, 1.0),
                master_bg: rgba(226, 96, 52, 0.08),
                error: rgba(208, 52, 52, 1.0),
                ring: rgba(0, 0, 0, 0.14),
                day_face: rgba(255, 255, 255, 1.0),
                day_ink: rgba(30, 33, 41, 1.0),
                night_face: rgba(43, 49, 64, 1.0),
                night_ink: rgba(236, 239, 244, 1.0),
            },
        }
    }
}

/// Pixel size of the widget for this many clocks.
pub fn panel_size(clocks: usize, scale: f32) -> (u32, u32) {
    let n = clocks.max(1) as f32;
    let w = (PAD_X * 2.0 + COL_W * n) * scale;
    let h = (PAD_Y + DIAL_TOP + DIAL_D + TIME_GAP + DATE_GAP + DIFF_GAP + BOTTOM + PAD_Y) * scale;
    (w.ceil() as u32, h.ceil() as u32)
}

/// Which clock column (if any) is under widget-relative x.
pub fn column_at(x: f32, clocks: usize, scale: f32) -> Option<usize> {
    let col = ((x / scale - PAD_X) / COL_W).floor();
    if col >= 0.0 && (col as usize) < clocks {
        Some(col as usize)
    } else {
        None
    }
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

fn paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}

fn fill_circle(pix: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        pix.fill_path(&path, &paint(color), FillRule::Winding, Transform::identity(), None);
    }
}

fn stroke_circle(pix: &mut Pixmap, cx: f32, cy: f32, r: f32, width: f32, color: Color) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        let stroke = Stroke { width, ..Stroke::default() };
        pix.stroke_path(&path, &paint(color), &stroke, Transform::identity(), None);
    }
}

/// A line from radius `r0` to `r1` at `deg` degrees clockwise from 12 o'clock.
#[allow(clippy::too_many_arguments)]
fn radial_line(pix: &mut Pixmap, cx: f32, cy: f32, deg: f32, r0: f32, r1: f32, width: f32, color: Color) {
    let a = deg * PI / 180.0;
    let (s, c) = a.sin_cos();
    let mut pb = PathBuilder::new();
    pb.move_to(cx + s * r0, cy - c * r0);
    pb.line_to(cx + s * r1, cy - c * r1);
    if let Some(path) = pb.finish() {
        let stroke = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
        pix.stroke_path(&path, &paint(color), &stroke, Transform::identity(), None);
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    let mut c = c;
    c.set_alpha(c.alpha() * a);
    c
}

fn draw_dial(pix: &mut Pixmap, cx: f32, cy: f32, r: f32, reading: &Reading, pal: &Palette, s: f32) {
    let day = reading.is_daytime();
    let (face, ink) = if day { (pal.day_face, pal.day_ink) } else { (pal.night_face, pal.night_ink) };

    fill_circle(pix, cx, cy, r, face);
    let ring = if reading.is_master { pal.accent } else { pal.ring };
    stroke_circle(pix, cx, cy, r - 0.75 * s, if reading.is_master { 2.0 * s } else { 1.5 * s }, ring);

    // Minute ticks, then hour ticks (12, 3, 6 and 9 are longer).
    if s >= 0.75 {
        for m in 0..60 {
            if m % 5 != 0 {
                radial_line(pix, cx, cy, m as f32 * 6.0, r - 5.0 * s, r - 7.5 * s, 1.0 * s, with_alpha(ink, 0.35));
            }
        }
    }
    for h in 0..12 {
        let cardinal = h % 3 == 0;
        let inner = if cardinal { r - 15.0 * s } else { r - 12.0 * s };
        let width = if cardinal { 2.6 * s } else { 1.8 * s };
        radial_line(pix, cx, cy, h as f32 * 30.0, r - 5.5 * s, inner, width, with_alpha(ink, 0.9));
    }

    if reading.error.is_some() {
        draw_text_centered_q(pix, cx, cy, r, ink);
    }
}

/// The moving parts of a dial, drawn over the cached face each second.
#[allow(clippy::too_many_arguments)]
fn draw_dial_hands(
    pix: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    reading: &Reading,
    second_hand: bool,
    pal: &Palette,
    s: f32,
) {
    if reading.error.is_some() {
        return;
    }
    let ink = if reading.is_daytime() { pal.day_ink } else { pal.night_ink };
    let (ha, ma, sa) = clock::hand_angles(reading.hour, reading.minute, reading.second);
    radial_line(pix, cx, cy, ha, -0.08 * r, 0.52 * r, 4.4 * s, ink);
    radial_line(pix, cx, cy, ma, -0.08 * r, 0.78 * r, 3.0 * s, ink);
    fill_circle(pix, cx, cy, 3.6 * s, ink);
    if second_hand {
        radial_line(pix, cx, cy, sa, -0.18 * r, 0.86 * r, 1.3 * s, pal.accent);
        fill_circle(pix, cx, cy, 2.2 * s, pal.accent);
    }
}

fn draw_text_centered_q(pix: &mut Pixmap, cx: f32, cy: f32, r: f32, ink: Color) {
    // Placeholder glyph-free mark for a clock whose zone couldn't be found.
    let w = r * 0.12;
    radial_line(pix, cx, cy, 0.0, -0.35 * r, 0.25 * r, w, with_alpha(ink, 0.6));
    fill_circle(pix, cx, cy + 0.42 * r, w * 0.6, with_alpha(ink, 0.6));
}

/// Renders the widget. `scale` combines display DPI and the chosen size.
pub fn render(readings: &[Reading], cfg: &Config, scale: f32, fonts: &Fonts) -> Pixmap {
    let mut pix = render_base(readings, cfg, scale, fonts);
    draw_hands(&mut pix, readings, cfg, scale);
    pix
}

/// Centre of each clock's dial.
fn dial_centres(n: usize, s: f32) -> impl Iterator<Item = (f32, f32, f32)> {
    let col_w = COL_W * s;
    let r = DIAL_D * s / 2.0;
    let cy = PAD_Y * s + DIAL_TOP * s + r;
    (0..n).map(move |i| (PAD_X * s + col_w * (i as f32 + 0.5), cy, r))
}

/// Draws the clock hands onto a frame made by [`render_base`].
pub fn draw_hands(pix: &mut Pixmap, readings: &[Reading], cfg: &Config, scale: f32) {
    let pal = Palette::new(cfg.theme, cfg.opacity);
    for (reading, (cx, cy, r)) in readings.iter().zip(dial_centres(readings.len(), scale)) {
        draw_dial_hands(pix, cx, cy, r, reading, cfg.second_hand, &pal, scale);
    }
}

/// Everything except the hands. It only changes once a minute, so the widget
/// caches it and just adds the hands every second.
pub fn render_base(readings: &[Reading], cfg: &Config, scale: f32, fonts: &Fonts) -> Pixmap {
    let s = scale;
    let n = readings.len().max(1);
    let (w, h) = panel_size(n, s);
    let mut pix = Pixmap::new(w, h).expect("non-zero widget size");
    let pal = Palette::new(cfg.theme, cfg.opacity);

    // Background panel.
    let half = 0.5 * s;
    if let Some(bg) = rounded_rect(half, half, w as f32 - 2.0 * half, h as f32 - 2.0 * half, CORNER * s) {
        pix.fill_path(&bg, &paint(pal.panel), FillRule::Winding, Transform::identity(), None);
        let stroke = Stroke { width: 1.0 * s, ..Stroke::default() };
        pix.stroke_path(&bg, &paint(pal.border), &stroke, Transform::identity(), None);
    }

    let col_w = COL_W * s;
    let top = PAD_Y * s;
    for (i, reading) in readings.iter().enumerate() {
        let x0 = PAD_X * s + col_w * i as f32;
        let cx = x0 + col_w / 2.0;

        if reading.is_master && readings.len() > 1 {
            if let Some(bg) = rounded_rect(x0 + 3.0 * s, top - 6.0 * s, col_w - 6.0 * s, h as f32 - top * 2.0 + 12.0 * s, 11.0 * s) {
                pix.fill_path(&bg, &paint(pal.master_bg), FillRule::Winding, Transform::identity(), None);
            }
        }
        if i > 0 {
            let mut pb = PathBuilder::new();
            pb.move_to(x0, top + 8.0 * s);
            pb.line_to(x0, h as f32 - top - 8.0 * s);
            if let Some(line) = pb.finish() {
                let stroke = Stroke { width: 1.0 * s, ..Stroke::default() };
                pix.stroke_path(&line, &paint(pal.separator), &stroke, Transform::identity(), None);
            }
        }

        // City name.
        let label = ellipsize(&fonts.bold, LABEL_PX * s, &reading.label, col_w - 14.0 * s);
        draw_text(&mut pix, &fonts.bold, LABEL_PX * s, &label, cx, top + LABEL_BASE * s, Align::Center, pal.label);

        // Analog dial (face and ticks; hands come later).
        let r = DIAL_D * s / 2.0;
        let cy = top + DIAL_TOP * s + r;
        draw_dial(&mut pix, cx, cy, r, reading, &pal, s);

        // Digital time, with a smaller AM/PM.
        let time_base = top + (DIAL_TOP + DIAL_D + TIME_GAP) * s;
        let (time, ampm) = clock::format_time(reading.hour, reading.minute, cfg.use_24_hour);
        if reading.error.is_some() {
            draw_text(&mut pix, &fonts.bold, TIME_PX * s, "--:--", cx, time_base, Align::Center, pal.muted);
        } else {
            let main_w = text_width(&fonts.bold, TIME_PX * s, &time);
            let gap = 3.0 * s;
            let ampm_w = ampm.map_or(0.0, |t| gap + text_width(&fonts.bold, AMPM_PX * s, t));
            let left = cx - (main_w + ampm_w) / 2.0;
            draw_text(&mut pix, &fonts.bold, TIME_PX * s, &time, left, time_base, Align::Left, pal.time);
            if let Some(t) = ampm {
                draw_text(&mut pix, &fonts.bold, AMPM_PX * s, t, left + main_w + gap, time_base, Align::Left, pal.muted);
            }
        }

        // Day and date.
        let date_base = time_base + DATE_GAP * s;
        let date = if reading.error.is_some() { "Check settings".to_string() } else { clock::format_date(reading) };
        draw_text(&mut pix, &fonts.regular, DATE_PX * s, &date, cx, date_base, Align::Center, pal.muted);

        // Difference from the master clock.
        let diff_base = date_base + DIFF_GAP * s;
        let (diff, font, color) = if reading.error.is_some() {
            ("Unknown zone".to_string(), &fonts.bold, pal.error)
        } else if reading.is_master {
            (clock::diff_line(reading), &fonts.bold, pal.accent)
        } else {
            (clock::diff_line(reading), &fonts.regular, pal.muted)
        };
        let diff = ellipsize(font, DIFF_PX * s, &diff, col_w - 8.0 * s);
        draw_text(&mut pix, font, DIFF_PX * s, &diff, cx, diff_base, Align::Center, color);
    }
    pix
}

/// A small clock face for the tray icon (shows 10:10, like a watch advert).
pub fn render_icon(size: u32) -> Pixmap {
    let mut pix = Pixmap::new(size.max(1), size.max(1)).expect("icon size");
    let s = size as f32 / 16.0;
    let (cx, cy, r) = (size as f32 / 2.0, size as f32 / 2.0, size as f32 / 2.0 - 0.5 * s);
    fill_circle(&mut pix, cx, cy, r, rgba(42, 47, 61, 1.0));
    fill_circle(&mut pix, cx, cy, r - 1.6 * s, rgba(244, 241, 234, 1.0));
    let ink = rgba(30, 33, 41, 1.0);
    radial_line(&mut pix, cx, cy, 305.0, 0.0, r * 0.5, 1.7 * s, ink);
    radial_line(&mut pix, cx, cy, 60.0, 0.0, r * 0.72, 1.4 * s, ink);
    fill_circle(&mut pix, cx, cy, 1.2 * s, rgba(255, 138, 91, 1.0));
    pix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_grows_with_clocks_and_scale() {
        let (w1, h1) = panel_size(1, 1.0);
        let (w5, h5) = panel_size(5, 1.0);
        assert_eq!(h1, h5);
        assert!(w5 > w1 * 4);
        let (w5_big, _) = panel_size(5, 1.5);
        assert_eq!(w5_big, (w5 as f32 * 1.5).ceil() as u32);
    }

    #[test]
    fn column_hit_testing() {
        assert_eq!(column_at(5.0, 3, 1.0), None);
        assert_eq!(column_at(PAD_X + 1.0, 3, 1.0), Some(0));
        assert_eq!(column_at(PAD_X + COL_W * 2.5, 3, 1.0), Some(2));
        assert_eq!(column_at(PAD_X + COL_W * 3.2, 3, 1.0), None);
        assert_eq!(column_at((PAD_X + COL_W * 1.5) * 2.0, 3, 2.0), Some(1));
    }

    #[test]
    fn icon_has_transparent_corners_and_solid_centre() {
        let icon = render_icon(32);
        assert_eq!(icon.pixel(0, 0).unwrap().alpha(), 0);
        assert_eq!(icon.pixel(16, 22).unwrap().alpha(), 255);
    }
}
