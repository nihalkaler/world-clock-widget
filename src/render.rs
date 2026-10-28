//! Draws the whole widget into a pixel buffer. Platform independent, so the
//! same code renders the Windows widget and the PNG previews.

use crate::clock::{self, Reading};
use crate::config::{ColorField, Colors, Config, DialStyle, Display, Theme};
use crate::digital::{self, Flip};
use crate::text::{self, ellipsize, text_width, Align, Fonts, TextStyle};
use std::f32::consts::PI;
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap,
    Point, Rect, SpreadMode, Stroke, Transform,
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
/// Height of a digital clock's display (its width is the column less margins).
const DIGITAL_H: f32 = 72.0;
const TIME_PX: f32 = 22.0;
const TIME_GAP: f32 = 31.0;
const AMPM_PX: f32 = 12.0;
const DATE_PX: f32 = 12.5;
const DATE_GAP: f32 = 20.0;
const DIFF_PX: f32 = 12.0;
const DIFF_GAP: f32 = 18.0;
const BOTTOM: f32 = 5.0;
/// Stopwatch case radius as a share of the dial radius (the crown sits above).
const STOPWATCH: f32 = 0.84;

pub(crate) fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba8(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn rgb(c: (u8, u8, u8), a: f32) -> Color {
    rgba(c.0, c.1, c.2, a)
}

fn to_rgb(c: Color) -> (u8, u8, u8) {
    let u = c.to_color_u8();
    (u.red(), u.green(), u.blue())
}

pub(crate) fn with_alpha(c: Color, a: f32) -> Color {
    let mut c = c;
    c.set_alpha((c.alpha() * a).clamp(0.0, 1.0));
    c
}

/// Every colour and flourish the drawing code uses. Built from the theme,
/// then overridden by any custom colours.
#[derive(Debug, Clone)]
pub struct Palette {
    pub(crate) panel: Color,
    /// Bottom colour of a vertical background gradient.
    pub(crate) panel2: Option<Color>,
    pub(crate) border: Color,
    /// Right-hand colour of a horizontal border gradient.
    pub(crate) border2: Option<Color>,
    pub(crate) separator: Color,
    pub(crate) label: Color,
    pub(crate) time: Color,
    pub(crate) muted: Color,
    pub(crate) diff: Color,
    pub(crate) accent: Color,
    pub(crate) master: Color,
    pub(crate) master_bg: Color,
    pub(crate) error: Color,
    pub(crate) ring: Color,
    pub(crate) day_face: Color,
    pub(crate) day_ink: Color,
    pub(crate) night_face: Color,
    pub(crate) night_ink: Color,
    pub(crate) metal_hi: Color,
    pub(crate) metal_lo: Color,
    /// Neon halo around lines and text.
    pub(crate) glow: bool,
    /// Cut-corner panel, scanlines and capitalised, spaced-out labels.
    pub(crate) hud: bool,
    /// Field-kit details instead of neon ones: an evenly cut plate,
    /// diagonal hatching and corner brackets.
    pub(crate) tactical: bool,
}

impl Palette {
    pub fn for_config(cfg: &Config) -> Palette {
        let mut p = Palette::theme(cfg.theme, cfg.opacity);
        p.apply(&cfg.colors, cfg.opacity);
        p
    }

    fn theme(theme: Theme, opacity: f32) -> Palette {
        match theme {
            Theme::Cyberpunk => Palette {
                panel: rgba(10, 6, 20, opacity),
                panel2: Some(rgba(24, 10, 44, opacity)),
                border: rgba(255, 42, 109, 0.95),
                border2: Some(rgba(5, 217, 232, 0.95)),
                separator: rgba(5, 217, 232, 0.16),
                label: rgba(5, 217, 232, 1.0),
                time: rgba(242, 245, 255, 1.0),
                muted: rgba(150, 150, 214, 1.0),
                diff: rgba(255, 94, 150, 1.0),
                accent: rgba(255, 42, 109, 1.0),
                master: rgba(249, 240, 2, 1.0),
                master_bg: rgba(5, 217, 232, 0.07),
                error: rgba(255, 80, 80, 1.0),
                ring: rgba(5, 217, 232, 0.55),
                day_face: rgba(24, 16, 52, 1.0),
                day_ink: rgba(5, 217, 232, 1.0),
                night_face: rgba(8, 6, 17, 1.0),
                night_ink: rgba(197, 107, 255, 1.0),
                metal_hi: rgba(5, 217, 232, 1.0),
                metal_lo: rgba(106, 0, 255, 1.0),
                glow: true,
                hud: true,
                tactical: false,
            },
            Theme::Tactical => Palette {
                panel: rgba(28, 32, 21, opacity),
                panel2: Some(rgba(17, 20, 13, opacity)),
                border: rgba(181, 161, 107, 0.9),
                border2: None,
                separator: rgba(181, 161, 107, 0.16),
                label: rgba(216, 198, 154, 1.0),
                time: rgba(236, 226, 200, 1.0),
                muted: rgba(160, 150, 118, 1.0),
                diff: rgba(190, 172, 120, 1.0),
                accent: rgba(242, 160, 61, 1.0),
                master: rgba(242, 160, 61, 1.0),
                master_bg: rgba(242, 160, 61, 0.08),
                error: rgba(230, 90, 70, 1.0),
                ring: rgba(181, 161, 107, 0.45),
                day_face: rgba(205, 191, 147, 1.0),
                day_ink: rgba(36, 40, 26, 1.0),
                night_face: rgba(19, 23, 15, 1.0),
                night_ink: rgba(155, 229, 100, 1.0),
                metal_hi: rgba(196, 186, 152, 1.0),
                metal_lo: rgba(84, 88, 64, 1.0),
                glow: false,
                hud: true,
                tactical: true,
            },
            Theme::Dark => Palette {
                panel: rgba(17, 19, 25, opacity),
                panel2: None,
                border: rgba(255, 255, 255, 0.10),
                border2: None,
                separator: rgba(255, 255, 255, 0.07),
                label: rgba(236, 239, 244, 1.0),
                time: rgba(255, 255, 255, 1.0),
                muted: rgba(164, 171, 184, 1.0),
                diff: rgba(164, 171, 184, 1.0),
                accent: rgba(255, 138, 91, 1.0),
                master: rgba(255, 138, 91, 1.0),
                master_bg: rgba(255, 138, 91, 0.09),
                error: rgba(255, 107, 107, 1.0),
                ring: rgba(255, 255, 255, 0.16),
                day_face: rgba(244, 241, 234, 1.0),
                day_ink: rgba(30, 33, 41, 1.0),
                night_face: rgba(42, 47, 61, 1.0),
                night_ink: rgba(236, 239, 244, 1.0),
                metal_hi: rgba(236, 237, 241, 1.0),
                metal_lo: rgba(120, 124, 134, 1.0),
                glow: false,
                hud: false,
                tactical: false,
            },
            Theme::Light => Palette {
                panel: rgba(250, 250, 252, opacity),
                panel2: None,
                border: rgba(0, 0, 0, 0.08),
                border2: None,
                separator: rgba(0, 0, 0, 0.07),
                label: rgba(30, 34, 42, 1.0),
                time: rgba(17, 19, 25, 1.0),
                muted: rgba(95, 102, 116, 1.0),
                diff: rgba(95, 102, 116, 1.0),
                accent: rgba(226, 96, 52, 1.0),
                master: rgba(226, 96, 52, 1.0),
                master_bg: rgba(226, 96, 52, 0.08),
                error: rgba(208, 52, 52, 1.0),
                ring: rgba(0, 0, 0, 0.14),
                day_face: rgba(255, 255, 255, 1.0),
                day_ink: rgba(30, 33, 41, 1.0),
                night_face: rgba(43, 49, 64, 1.0),
                night_ink: rgba(236, 239, 244, 1.0),
                metal_hi: rgba(226, 228, 233, 1.0),
                metal_lo: rgba(110, 114, 124, 1.0),
                glow: false,
                hud: false,
                tactical: false,
            },
        }
    }

    fn apply(&mut self, colors: &Colors, opacity: f32) {
        if let Some(c) = colors.get(ColorField::Background) {
            self.panel = rgb(c, opacity);
            self.panel2 = None;
        }
        if let Some(c) = colors.get(ColorField::Accent) {
            self.accent = rgb(c, 1.0);
            self.master = rgb(c, 1.0);
            self.master_bg = rgb(c, 0.09);
        }
        if let Some(c) = colors.get(ColorField::Text) {
            self.label = rgb(c, 1.0);
            self.time = rgb(c, 1.0);
            self.muted = rgb(c, 0.66);
            self.diff = rgb(c, 0.66);
        }
        if let Some(c) = colors.get(ColorField::DayFace) {
            self.day_face = rgb(c, 1.0);
        }
        if let Some(c) = colors.get(ColorField::NightFace) {
            self.night_face = rgb(c, 1.0);
        }
        if let Some(c) = colors.get(ColorField::DayHands) {
            self.day_ink = rgb(c, 1.0);
        }
        if let Some(c) = colors.get(ColorField::NightHands) {
            self.night_ink = rgb(c, 1.0);
        }
    }

    /// The colour currently used for `field`, to start the colour picker on.
    pub fn color_of(&self, field: ColorField) -> (u8, u8, u8) {
        to_rgb(match field {
            ColorField::Background => self.panel,
            ColorField::Accent => self.accent,
            ColorField::Text => self.time,
            ColorField::DayFace => self.day_face,
            ColorField::NightFace => self.night_face,
            ColorField::DayHands => self.day_ink,
            ColorField::NightHands => self.night_ink,
        })
    }

    /// A handful of this theme's colours, to seed the picker's custom swatches.
    pub fn swatches(&self) -> Vec<(u8, u8, u8)> {
        [self.accent, self.label, self.master, self.night_ink, self.day_face, self.panel, self.time, self.muted]
            .into_iter()
            .map(to_rgb)
            .collect()
    }
}

/// How tall each part of a column is, which depends on analog or digital.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Height of the dial or digital display.
    pub display_h: f32,
    /// Whether the time is also written out under the display.
    pub time_line: bool,
}

impl Layout {
    pub fn for_config(cfg: &Config) -> Layout {
        match cfg.display {
            Display::Analog => Layout { display_h: DIAL_D, time_line: true },
            Display::Digital => Layout { display_h: DIGITAL_H, time_line: digital::shows_time_line(cfg.digital) },
        }
    }

    /// Baseline of the date line, measured from the top of the display.
    fn date_offset(&self) -> f32 {
        if self.time_line {
            self.display_h + TIME_GAP + DATE_GAP
        } else {
            self.display_h + DATE_GAP + 4.0
        }
    }
}

/// Pixel size of the widget for this many clocks.
pub fn panel_size(clocks: usize, scale: f32, layout: &Layout) -> (u32, u32) {
    let n = clocks.max(1) as f32;
    let w = (PAD_X * 2.0 + COL_W * n) * scale;
    let h = (PAD_Y + DIAL_TOP + layout.date_offset() + DIFF_GAP + BOTTOM + PAD_Y) * scale;
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

// ------------------------------------------------------------- primitives

pub(crate) fn paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}

pub(crate) fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
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

/// A rectangle with its top-left and bottom-right corners cut off.
fn chamfered_rect(x: f32, y: f32, w: f32, h: f32, big: f32, small: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x + big, y);
    pb.line_to(x + w - small, y);
    pb.line_to(x + w, y + small);
    pb.line_to(x + w, y + h - big);
    pb.line_to(x + w - big, y + h);
    pb.line_to(x + small, y + h);
    pb.line_to(x, y + h - small);
    pb.line_to(x, y + big);
    pb.close();
    pb.finish()
}

pub(crate) fn fill(pix: &mut Pixmap, path: &Path, color: Color) {
    pix.fill_path(path, &paint(color), FillRule::Winding, Transform::identity(), None);
}

/// Strokes a path, with a soft neon halo underneath when `glow` is on.
pub(crate) fn stroke(pix: &mut Pixmap, path: &Path, width: f32, color: Color, glow: bool) {
    if glow {
        for (spread, alpha) in [(4.2, 0.06), (2.8, 0.11), (1.8, 0.2)] {
            let s = Stroke { width: width * spread, line_cap: LineCap::Round, ..Stroke::default() };
            pix.stroke_path(path, &paint(with_alpha(color, alpha)), &s, Transform::identity(), None);
        }
    }
    let s = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
    pix.stroke_path(path, &paint(color), &s, Transform::identity(), None);
}

pub(crate) fn fill_circle(pix: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        fill(pix, &path, color);
    }
}

pub(crate) fn circle(pix: &mut Pixmap, cx: f32, cy: f32, r: f32, width: f32, color: Color, glow: bool) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        stroke(pix, &path, width, color, glow);
    }
}

/// Point at `radius` and `deg` degrees clockwise from 12 o'clock.
fn polar(cx: f32, cy: f32, deg: f32, radius: f32) -> (f32, f32) {
    let (s, c) = (deg * PI / 180.0).sin_cos();
    (cx + s * radius, cy - c * radius)
}

/// A line from radius `r0` to `r1` at `deg` degrees clockwise from 12 o'clock.
#[allow(clippy::too_many_arguments)]
fn radial(pix: &mut Pixmap, cx: f32, cy: f32, deg: f32, r0: f32, r1: f32, width: f32, color: Color, glow: bool) {
    let (x0, y0) = polar(cx, cy, deg, r0);
    let (x1, y1) = polar(cx, cy, deg, r1);
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y0);
    pb.line_to(x1, y1);
    if let Some(path) = pb.finish() {
        stroke(pix, &path, width, color, glow);
    }
}

/// An arc clockwise from 12 o'clock to `deg`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn arc(pix: &mut Pixmap, cx: f32, cy: f32, radius: f32, deg: f32, width: f32, color: Color, glow: bool) {
    if deg <= 0.0 {
        return;
    }
    let steps = ((deg / 3.0).ceil() as usize).max(2);
    let mut pb = PathBuilder::new();
    for i in 0..=steps {
        let (x, y) = polar(cx, cy, deg * i as f32 / steps as f32, radius);
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    if let Some(path) = pb.finish() {
        stroke(pix, &path, width, color, glow);
    }
}

/// A tapered hand: narrow tail, widest near the centre, pointed tip.
#[allow(clippy::too_many_arguments)]
fn pointer(pix: &mut Pixmap, cx: f32, cy: f32, deg: f32, tail: f32, len: f32, width: f32, color: Color, glow: bool) {
    let (s, c) = (deg * PI / 180.0).sin_cos();
    let at = |u: f32, v: f32| (cx + u * s + v * c, cy - u * c + v * s);
    let pts = [
        at(-tail, -width * 0.22),
        at(len * 0.18, -width / 2.0),
        at(len, 0.0),
        at(len * 0.18, width / 2.0),
        at(-tail, width * 0.22),
    ];
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for (x, y) in &pts[1..] {
        pb.line_to(*x, *y);
    }
    pb.close();
    if let Some(path) = pb.finish() {
        if glow {
            stroke(pix, &path, width * 0.4, with_alpha(color, 0.5), true);
        }
        fill(pix, &path, color);
    }
}

// ------------------------------------------------------------- dial faces

/// Where a dial's hands pivot and how long they can be.
#[derive(Clone, Copy)]
struct Geometry {
    cx: f32,
    cy: f32,
    /// Radius the hands are measured against.
    r: f32,
}

fn geometry(style: DialStyle, cx: f32, cy: f32, r: f32) -> Geometry {
    match style {
        // Smaller and lower, to leave room for the crown on top.
        DialStyle::Stopwatch => Geometry { cx, cy: cy + 0.12 * r, r: STOPWATCH * r * 0.9 },
        _ => Geometry { cx, cy, r },
    }
}

struct Ink {
    face: Color,
    ink: Color,
    glow: bool,
}

fn luminance(c: Color) -> f32 {
    0.2126 * c.red() + 0.7152 * c.green() + 0.0722 * c.blue()
}

fn ink_for(reading: &Reading, style: DialStyle, pal: &Palette) -> Ink {
    let (face, ink) = if reading.is_daytime() { (pal.day_face, pal.day_ink) } else { (pal.night_face, pal.night_ink) };
    if style == DialStyle::Neon {
        // Neon only reads on a dark face. By day it glows warm amber unless
        // the day colour is already bright enough to glow.
        let day_ink = if luminance(pal.day_ink) > 0.35 { pal.day_ink } else { rgba(255, 196, 92, 1.0) };
        let ink = if reading.is_daytime() { day_ink } else { pal.night_ink };
        let face = if luminance(pal.night_face) < 0.3 { pal.night_face } else { rgba(14, 14, 22, 1.0) };
        return Ink { face, ink, glow: true };
    }
    Ink { face, ink, glow: pal.glow }
}

/// Text centred on a point (for numerals on a dial).
fn centred(pix: &mut Pixmap, style: &TextStyle, text: &str, x: f32, y: f32) {
    text::draw(pix, style, text, x, y + style.size * 0.36);
}

#[allow(clippy::too_many_arguments)]
fn draw_face(
    pix: &mut Pixmap,
    fonts: &Fonts,
    cx: f32,
    cy: f32,
    r: f32,
    reading: &Reading,
    style: DialStyle,
    pal: &Palette,
    s: f32,
) {
    let Ink { face, ink, glow } = ink_for(reading, style, pal);
    let ring_color = if reading.is_master { pal.master } else { pal.ring };
    let ring_width = if reading.is_master { 2.0 * s } else { 1.5 * s };
    let numeral_font = if pal.hud { &fonts.display } else { &fonts.bold };

    match style {
        DialStyle::Classic => {
            fill_circle(pix, cx, cy, r, face);
            circle(pix, cx, cy, r - 0.75 * s, ring_width, ring_color, glow);
            if s >= 0.75 {
                for m in (0..60).filter(|m| m % 5 != 0) {
                    radial(pix, cx, cy, m as f32 * 6.0, r - 5.0 * s, r - 7.5 * s, 1.0 * s, with_alpha(ink, 0.35), false);
                }
            }
            for h in 0..12 {
                let cardinal = h % 3 == 0;
                let inner = if cardinal { r - 15.0 * s } else { r - 12.0 * s };
                let width = if cardinal { 2.6 * s } else { 1.8 * s };
                radial(pix, cx, cy, h as f32 * 30.0, r - 5.5 * s, inner, width, with_alpha(ink, 0.9), glow);
            }
        }
        DialStyle::Numbers => {
            fill_circle(pix, cx, cy, r, face);
            circle(pix, cx, cy, r - 0.75 * s, ring_width, ring_color, glow);
            for m in 0..60 {
                let hour = m % 5 == 0;
                let (inner, width, alpha) = if hour { (r - 9.0 * s, 1.6 * s, 0.8) } else { (r - 6.5 * s, 0.9 * s, 0.3) };
                if hour || s >= 0.75 {
                    radial(pix, cx, cy, m as f32 * 6.0, r - 4.5 * s, inner, width, with_alpha(ink, alpha), false);
                }
            }
            let style = TextStyle { glow: glow.then_some(ink), ..TextStyle::new(numeral_font, r * 0.25, ink) };
            for h in 1..=12 {
                let (x, y) = polar(cx, cy, h as f32 * 30.0, r * 0.69);
                centred(pix, &style, &h.to_string(), x, y);
            }
        }
        DialStyle::Roman => {
            fill_circle(pix, cx, cy, r, face);
            circle(pix, cx, cy, r - 0.75 * s, ring_width, ring_color, glow);
            // Railway-track minute ring.
            let (outer, inner) = (r - 4.5 * s, r - 10.0 * s);
            let track = with_alpha(ink, 0.55);
            circle(pix, cx, cy, outer, 0.8 * s, track, false);
            circle(pix, cx, cy, inner, 0.8 * s, track, false);
            for m in 0..60 {
                let width = if m % 5 == 0 { 1.6 * s } else { 0.8 * s };
                radial(pix, cx, cy, m as f32 * 6.0, outer, inner, width, track, false);
            }
            const ROMAN: [&str; 12] = ["XII", "I", "II", "III", "IIII", "V", "VI", "VII", "VIII", "IX", "X", "XI"];
            let style = TextStyle { glow: glow.then_some(ink), ..TextStyle::new(&fonts.serif, r * 0.185, ink) };
            for (h, numeral) in ROMAN.iter().enumerate() {
                let (x, y) = polar(cx, cy, h as f32 * 30.0, r * 0.67);
                centred(pix, &style, numeral, x, y);
            }
        }
        DialStyle::Minimal => {
            fill_circle(pix, cx, cy, r, face);
            if reading.is_master {
                circle(pix, cx, cy, r - 0.75 * s, ring_width, ring_color, glow);
            } else {
                circle(pix, cx, cy, r - 0.5 * s, 1.0 * s, with_alpha(pal.ring, 0.6), false);
            }
            for h in 0..12 {
                let (x, y) = polar(cx, cy, h as f32 * 30.0, r * 0.84);
                let dot = if h % 3 == 0 { r * 0.05 } else { r * 0.028 };
                fill_circle(pix, x, y, dot, with_alpha(ink, 0.9));
            }
        }
        DialStyle::Stopwatch => draw_stopwatch_face(pix, fonts, cx, cy, r, reading, pal, &Ink { face, ink, glow }, s),
        DialStyle::Neon => {
            fill_circle(pix, cx, cy, r, with_alpha(face, 0.85));
            let track = if reading.is_master { pal.master } else { ink };
            circle(pix, cx, cy, r - 3.0 * s, 1.2 * s, with_alpha(track, 0.35), true);
            circle(pix, cx, cy, r * 0.62, 0.8 * s, with_alpha(ink, 0.18), false);
            for h in 0..12 {
                let inner = if h % 3 == 0 { r - 18.0 * s } else { r - 14.0 * s };
                radial(pix, cx, cy, h as f32 * 30.0, r - 8.5 * s, inner, 2.2 * s, ink, true);
            }
        }
    }

    if reading.error.is_some() {
        // A "?" made of a line and a dot, for a clock whose zone couldn't be found.
        let g = geometry(style, cx, cy, r);
        let w = g.r * 0.12;
        radial(pix, g.cx, g.cy, 0.0, -0.35 * g.r, 0.25 * g.r, w, with_alpha(ink, 0.6), false);
        fill_circle(pix, g.cx, g.cy + 0.42 * g.r, w * 0.6, with_alpha(ink, 0.6));
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_stopwatch_face(
    pix: &mut Pixmap,
    fonts: &Fonts,
    cx: f32,
    cy: f32,
    r: f32,
    reading: &Reading,
    pal: &Palette,
    ink: &Ink,
    s: f32,
) {
    let cy = cy + 0.12 * r;
    let outer = STOPWATCH * r;
    let face_r = outer * 0.9;
    let metal = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let stops = vec![GradientStop::new(0.0, pal.metal_hi), GradientStop::new(1.0, pal.metal_lo)];
        let shader = LinearGradient::new(Point::from_xy(x0, y0), Point::from_xy(x1, y1), stops, SpreadMode::Pad, Transform::identity());
        let mut p = paint(pal.metal_hi);
        if let Some(shader) = shader {
            p.shader = shader;
        }
        p
    };

    // Crown: stem, knurled knob and the ring (bow) on top.
    let top = cy - outer;
    let stem_w = 0.10 * outer;
    if let Some(rect) = Rect::from_xywh(cx - stem_w / 2.0, top - 0.08 * outer, stem_w, 0.10 * outer) {
        pix.fill_rect(rect, &metal(cx - stem_w, 0.0, cx + stem_w, 0.0), Transform::identity(), None);
    }
    let (knob_w, knob_h) = (0.28 * outer, 0.11 * outer);
    let knob_y = top - 0.08 * outer - knob_h;
    if let Some(knob) = rounded_rect(cx - knob_w / 2.0, knob_y, knob_w, knob_h, 0.03 * outer) {
        pix.fill_path(&knob, &metal(cx - knob_w / 2.0, 0.0, cx + knob_w / 2.0, 0.0), FillRule::Winding, Transform::identity(), None);
        for i in 1..6 {
            let x = cx - knob_w / 2.0 + knob_w * i as f32 / 6.0;
            let mut pb = PathBuilder::new();
            pb.move_to(x, knob_y + 1.0 * s);
            pb.line_to(x, knob_y + knob_h - 1.0 * s);
            if let Some(line) = pb.finish() {
                stroke(pix, &line, 0.6 * s, with_alpha(pal.metal_lo, 0.9), false);
            }
        }
    }
    let bow_r = 0.10 * outer;
    circle(pix, cx, knob_y - bow_r * 0.9, bow_r, 0.045 * outer, pal.metal_hi, ink.glow);

    // Side pushers.
    for deg in [-40.0, 40.0] {
        radial(pix, cx, cy, deg, outer - 0.02 * outer, outer + 0.07 * outer, 0.09 * outer, pal.metal_lo, false);
    }

    // Bezel and face.
    if let Some(bezel) = PathBuilder::from_circle(cx, cy, outer) {
        let p = metal(cx - outer, cy - outer, cx + outer, cy + outer);
        pix.fill_path(&bezel, &p, FillRule::Winding, Transform::identity(), None);
    }
    if reading.is_master {
        circle(pix, cx, cy, outer - 0.5 * s, 2.0 * s, pal.master, ink.glow);
    }
    fill_circle(pix, cx, cy, face_r, ink.face);
    circle(pix, cx, cy, face_r, 0.8 * s, with_alpha(pal.metal_lo, 0.8), false);

    // Seconds track with 5-60 numbers.
    for m in 0..60 {
        let five = m % 5 == 0;
        let (inner, width) = if five { (face_r - 7.0 * s, 1.4 * s) } else { (face_r - 4.5 * s, 0.8 * s) };
        radial(pix, cx, cy, m as f32 * 6.0, face_r - 1.5 * s, inner, width, with_alpha(ink.ink, if five { 0.9 } else { 0.5 }), false);
    }
    let numbers = TextStyle { glow: ink.glow.then_some(ink.ink), ..TextStyle::new(&fonts.bold, face_r * 0.165, ink.ink) };
    for n in 1..=12 {
        let (x, y) = polar(cx, cy, n as f32 * 30.0, face_r * 0.71);
        centred(pix, &numbers, &(n * 5).to_string(), x, y);
    }

    // 24-hour sub-dial: one turn a day, so you can tell 9am from 9pm.
    let (sx, sy, sr) = (cx, cy + face_r * 0.33, face_r * 0.2);
    circle(pix, sx, sy, sr, 0.8 * s, with_alpha(ink.ink, 0.55), false);
    for q in 0..4 {
        radial(pix, sx, sy, q as f32 * 90.0, sr - 2.5 * s, sr, 0.8 * s, with_alpha(ink.ink, 0.7), false);
    }
    if reading.error.is_none() {
        let day_deg = (reading.hour as f32 + reading.minute as f32 / 60.0) / 24.0 * 360.0;
        radial(pix, sx, sy, day_deg, 0.0, sr * 0.8, 1.2 * s, pal.accent, ink.glow);
        fill_circle(pix, sx, sy, 1.6 * s, ink.ink);
    }
}

// ------------------------------------------------------------- hands

#[allow(clippy::too_many_arguments)]
fn draw_dial_hands(
    pix: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    reading: &Reading,
    style: DialStyle,
    second_hand: bool,
    pal: &Palette,
    s: f32,
) {
    if reading.error.is_some() {
        return;
    }
    let Ink { ink, glow, .. } = ink_for(reading, style, pal);
    let Geometry { cx, cy, r } = geometry(style, cx, cy, r);
    let (ha, ma, sa) = clock::hand_angles(reading.hour, reading.minute, reading.second);

    match style {
        DialStyle::Classic | DialStyle::Numbers => {
            let slim = if style == DialStyle::Numbers { 0.85 } else { 1.0 };
            radial(pix, cx, cy, ha, -0.08 * r, 0.52 * r, 4.4 * s * slim, ink, glow);
            radial(pix, cx, cy, ma, -0.08 * r, 0.78 * r, 3.0 * s * slim, ink, glow);
            fill_circle(pix, cx, cy, 3.6 * s, ink);
            if second_hand {
                radial(pix, cx, cy, sa, -0.18 * r, 0.86 * r, 1.3 * s, pal.accent, glow);
                fill_circle(pix, cx, cy, 2.2 * s, pal.accent);
            }
        }
        DialStyle::Roman => {
            pointer(pix, cx, cy, ha, 0.10 * r, 0.48 * r, 0.10 * r, ink, glow);
            pointer(pix, cx, cy, ma, 0.10 * r, 0.74 * r, 0.075 * r, ink, glow);
            fill_circle(pix, cx, cy, 3.4 * s, ink);
            if second_hand {
                radial(pix, cx, cy, sa, -0.18 * r, 0.82 * r, 1.0 * s, pal.accent, glow);
                fill_circle(pix, cx, cy, 1.8 * s, pal.accent);
            }
        }
        DialStyle::Minimal => {
            radial(pix, cx, cy, ha, 0.0, 0.50 * r, 2.6 * s, ink, glow);
            radial(pix, cx, cy, ma, 0.0, 0.76 * r, 1.8 * s, ink, glow);
            if second_hand {
                radial(pix, cx, cy, sa, -0.12 * r, 0.80 * r, 0.9 * s, pal.accent, glow);
            }
            fill_circle(pix, cx, cy, 2.6 * s, if second_hand { pal.accent } else { ink });
        }
        DialStyle::Stopwatch => {
            pointer(pix, cx, cy, ha, 0.08 * r, 0.50 * r, 0.075 * r, ink, glow);
            pointer(pix, cx, cy, ma, 0.08 * r, 0.80 * r, 0.06 * r, ink, glow);
            fill_circle(pix, cx, cy, 2.6 * s, ink);
            if second_hand {
                radial(pix, cx, cy, sa, -0.24 * r, 0.94 * r, 1.1 * s, pal.accent, glow);
                let (tx, ty) = polar(cx, cy, sa, -0.17 * r);
                circle(pix, tx, ty, 0.05 * r, 1.0 * s, pal.accent, false);
                fill_circle(pix, cx, cy, 2.0 * s, pal.accent);
            }
        }
        DialStyle::Neon => {
            radial(pix, cx, cy, ha, 0.0, 0.46 * r, 3.2 * s, ink, true);
            radial(pix, cx, cy, ma, 0.0, 0.70 * r, 2.4 * s, ink, true);
            circle(pix, cx, cy, 3.0 * s, 1.4 * s, ink, true);
            if second_hand {
                // The seconds sweep round the rim as a glowing arc.
                arc(pix, cx, cy, r - 3.0 * s, sa, 2.4 * s, pal.accent, true);
                let (tx, ty) = polar(cx, cy, sa, r - 3.0 * s);
                fill_circle(pix, tx, ty, 2.8 * s, pal.accent);
            }
        }
    }
}

// ------------------------------------------------------------- whole widget

/// Renders the widget. `scale` combines display DPI and the chosen size.
pub fn render(readings: &[Reading], cfg: &Config, scale: f32, fonts: &Fonts) -> Pixmap {
    let mut pix = render_base(readings, cfg, scale, fonts);
    draw_hands(&mut pix, readings, cfg, scale, fonts, &[]);
    pix
}

/// Centre of each clock's dial.
fn dial_centres(n: usize, s: f32) -> impl Iterator<Item = (f32, f32, f32)> {
    let col_w = COL_W * s;
    let r = DIAL_D * s / 2.0;
    let cy = PAD_Y * s + DIAL_TOP * s + r;
    (0..n).map(move |i| (PAD_X * s + col_w * (i as f32 + 0.5), cy, r))
}

/// Draws the parts that move every second onto a frame made by
/// [`render_base`]: the hands, or a digital clock's display. `flips` holds
/// any flip-card animation in progress, one entry per clock.
pub fn draw_hands(pix: &mut Pixmap, readings: &[Reading], cfg: &Config, scale: f32, fonts: &Fonts, flips: &[Flip]) {
    let pal = Palette::for_config(cfg);
    if cfg.display == Display::Digital {
        let s = scale;
        let (area_w, area_h) = ((COL_W - 20.0) * s, DIGITAL_H * s);
        for (i, reading) in readings.iter().enumerate() {
            let x = PAD_X * s + COL_W * s * i as f32 + 10.0 * s;
            let y = (PAD_Y + DIAL_TOP) * s;
            let area = digital::Area { x, y, w: area_w, h: area_h };
            digital::draw(pix, fonts, &area, reading, cfg, &pal, s, flips.get(i));
        }
        return;
    }
    for (i, (reading, (cx, cy, r))) in readings.iter().zip(dial_centres(readings.len(), scale)).enumerate() {
        draw_dial_hands(pix, cx, cy, r, reading, cfg.dial_for(i), cfg.second_hand, &pal, scale);
    }
}

fn draw_panel(pix: &mut Pixmap, pal: &Palette, w: f32, h: f32, s: f32) {
    let half = 0.5 * s;
    let shape = if pal.tactical {
        chamfered_rect(half, half, w - 2.0 * half, h - 2.0 * half, 10.0 * s, 10.0 * s)
    } else if pal.hud {
        chamfered_rect(half, half, w - 2.0 * half, h - 2.0 * half, 16.0 * s, 5.0 * s)
    } else {
        rounded_rect(half, half, w - 2.0 * half, h - 2.0 * half, CORNER * s)
    };
    let Some(bg) = shape else { return };

    let mut fill_paint = paint(pal.panel);
    if let Some(bottom) = pal.panel2 {
        let stops = vec![GradientStop::new(0.0, pal.panel), GradientStop::new(1.0, bottom)];
        if let Some(shader) =
            LinearGradient::new(Point::from_xy(0.0, 0.0), Point::from_xy(0.0, h), stops, SpreadMode::Pad, Transform::identity())
        {
            fill_paint.shader = shader;
        }
    }
    pix.fill_path(&bg, &fill_paint, FillRule::Winding, Transform::identity(), None);

    if pal.tactical {
        // Faint diagonal hatching, clipped to the panel.
        if let Some(mut mask) = Mask::new(pix.width(), pix.height()) {
            mask.fill_path(&bg, FillRule::Winding, true, Transform::identity());
            let mut pb = PathBuilder::new();
            let step = 7.0 * s.max(0.6);
            let mut x = -h;
            while x < w {
                pb.move_to(x, h);
                pb.line_to(x + h, 0.0);
                x += step;
            }
            if let Some(lines) = pb.finish() {
                let thin = Stroke { width: 1.0 * s.max(0.6), ..Stroke::default() };
                pix.stroke_path(&lines, &paint(rgba(216, 198, 154, 0.035)), &thin, Transform::identity(), Some(&mask));
            }
        }
    } else if pal.hud {
        // Faint scanlines, clipped to the panel.
        if let Some(mut mask) = Mask::new(pix.width(), pix.height()) {
            mask.fill_path(&bg, FillRule::Winding, true, Transform::identity());
            let line = paint(rgba(255, 255, 255, 0.028));
            let mut y = 2.0 * s;
            while y < h {
                if let Some(rect) = Rect::from_xywh(0.0, y, w, 1.0 * s.max(1.0)) {
                    pix.fill_rect(rect, &line, Transform::identity(), Some(&mask));
                }
                y += 3.0 * s.max(1.0);
            }
        }
    }

    let mut border_paint = paint(pal.border);
    if let Some(right) = pal.border2 {
        let stops = vec![GradientStop::new(0.0, pal.border), GradientStop::new(1.0, right)];
        if let Some(shader) =
            LinearGradient::new(Point::from_xy(0.0, 0.0), Point::from_xy(w, 0.0), stops, SpreadMode::Pad, Transform::identity())
        {
            border_paint.shader = shader;
        }
    }
    if pal.glow {
        let mut glow = border_paint.clone();
        for (spread, alpha) in [(4.0, 0.08), (2.2, 0.16)] {
            glow.shader = match pal.border2 {
                Some(right) => LinearGradient::new(
                    Point::from_xy(0.0, 0.0),
                    Point::from_xy(w, 0.0),
                    vec![
                        GradientStop::new(0.0, with_alpha(pal.border, alpha)),
                        GradientStop::new(1.0, with_alpha(right, alpha)),
                    ],
                    SpreadMode::Pad,
                    Transform::identity(),
                )
                .unwrap_or_else(|| paint(with_alpha(pal.border, alpha)).shader),
                None => paint(with_alpha(pal.border, alpha)).shader,
            };
            let stroke = Stroke { width: 1.2 * s * spread, ..Stroke::default() };
            pix.stroke_path(&bg, &glow, &stroke, Transform::identity(), None);
        }
    }
    let border_w = if pal.hud { 1.4 * s } else { 1.0 * s };
    pix.stroke_path(&bg, &border_paint, &Stroke { width: border_w, ..Stroke::default() }, Transform::identity(), None);

    if pal.tactical {
        // Corner brackets, like the marks on a rangefinder.
        let (inset, arm) = (6.0 * s, 11.0 * s);
        for (cx, cy, dx, dy) in [
            (inset, inset, 1.0, 1.0),
            (w - inset, inset, -1.0, 1.0),
            (inset, h - inset, 1.0, -1.0),
            (w - inset, h - inset, -1.0, -1.0),
        ] {
            let mut pb = PathBuilder::new();
            pb.move_to(cx, cy + arm * dy);
            pb.line_to(cx, cy);
            pb.line_to(cx + arm * dx, cy);
            if let Some(bracket) = pb.finish() {
                let firm = Stroke { width: 1.8 * s, ..Stroke::default() };
                pix.stroke_path(&bracket, &paint(pal.accent), &firm, Transform::identity(), None);
            }
        }
    } else if pal.hud {
        // Thick "tabs" along the top-left and bottom-right edges.
        for (x0, y, x1, color) in [
            (16.0 * s + 4.0 * s, 1.6 * s, 16.0 * s + 46.0 * s, pal.border),
            (w - 16.0 * s - 46.0 * s, h - 1.6 * s, w - 16.0 * s - 4.0 * s, pal.border2.unwrap_or(pal.border)),
        ] {
            let mut pb = PathBuilder::new();
            pb.move_to(x0, y);
            pb.line_to(x1, y);
            if let Some(tab) = pb.finish() {
                stroke(pix, &tab, 3.0 * s, color, pal.glow);
            }
        }
    }
}

/// Everything except the hands. It only changes once a minute, so the widget
/// caches it and just adds the hands every second.
pub fn render_base(readings: &[Reading], cfg: &Config, scale: f32, fonts: &Fonts) -> Pixmap {
    let s = scale;
    let n = readings.len().max(1);
    let layout = Layout::for_config(cfg);
    let (w, h) = panel_size(n, s, &layout);
    let mut pix = Pixmap::new(w, h).expect("non-zero widget size");
    let pal = Palette::for_config(cfg);
    let (wf, hf) = (w as f32, h as f32);

    draw_panel(&mut pix, &pal, wf, hf, s);

    let col_w = COL_W * s;
    let top = PAD_Y * s;
    let tracking = if pal.hud { 0.14 } else { 0.0 };
    let glow_of = |c: Color| pal.glow.then_some(with_alpha(c, 0.9));
    for (i, reading) in readings.iter().enumerate() {
        let x0 = PAD_X * s + col_w * i as f32;
        let cx = x0 + col_w / 2.0;

        if reading.is_master && readings.len() > 1 {
            let (bx, by, bw, bh) = (x0 + 3.0 * s, top - 6.0 * s, col_w - 6.0 * s, hf - top * 2.0 + 12.0 * s);
            let shape = if pal.hud { chamfered_rect(bx, by, bw, bh, 8.0 * s, 3.0 * s) } else { rounded_rect(bx, by, bw, bh, 11.0 * s) };
            if let Some(bg) = shape {
                fill(&mut pix, &bg, pal.master_bg);
            }
        }
        if i > 0 {
            let mut pb = PathBuilder::new();
            pb.move_to(x0, top + 8.0 * s);
            pb.line_to(x0, hf - top - 8.0 * s);
            if let Some(line) = pb.finish() {
                let butt = Stroke { width: 1.0 * s, ..Stroke::default() };
                pix.stroke_path(&line, &paint(pal.separator), &butt, Transform::identity(), None);
            }
        }

        // City name.
        let label_text = if pal.hud { reading.label.to_uppercase() } else { reading.label.clone() };
        let label = ellipsize(&fonts.bold, LABEL_PX * s, &label_text, col_w - 14.0 * s, tracking);
        let label_style = TextStyle {
            tracking,
            glow: glow_of(pal.label),
            ..TextStyle::new(&fonts.bold, LABEL_PX * s, pal.label)
        };
        text::draw(&mut pix, &label_style, &label, cx, top + LABEL_BASE * s);

        // Analog dial (face and marks; hands come later). A digital display
        // is drawn with the moving parts instead.
        if cfg.display == Display::Analog {
            let r = DIAL_D * s / 2.0;
            let cy = top + DIAL_TOP * s + r;
            draw_face(&mut pix, fonts, cx, cy, r, reading, cfg.dial_for(i), &pal, s);
        }

        // The time written out, with a smaller AM/PM.
        let time_base = top + (DIAL_TOP + layout.display_h + TIME_GAP) * s;
        let time_font = if pal.hud { &fonts.display } else { &fonts.bold };
        let time_px = if pal.hud { TIME_PX * 1.06 } else { TIME_PX } * s;
        let (time, ampm) = clock::format_time(reading.hour, reading.minute, cfg.use_24_hour);
        if !layout.time_line {
            // Digital displays already show the time.
        } else if reading.error.is_some() {
            text::draw_text(&mut pix, time_font, time_px, "--:--", cx, time_base, Align::Center, pal.muted);
        } else {
            let main = TextStyle { align: Align::Left, glow: glow_of(pal.label), ..TextStyle::new(time_font, time_px, pal.time) };
            let small = TextStyle { align: Align::Left, ..TextStyle::new(time_font, AMPM_PX * s, pal.muted) };
            let main_w = text_width(time_font, time_px, &time, 0.0);
            let gap = 3.0 * s;
            let ampm_w = ampm.map_or(0.0, |t| gap + text_width(time_font, AMPM_PX * s, t, 0.0));
            let left = cx - (main_w + ampm_w) / 2.0;
            text::draw(&mut pix, &main, &time, left, time_base);
            if let Some(t) = ampm {
                text::draw(&mut pix, &small, t, left + main_w + gap, time_base);
            }
        }

        // Day and date.
        let date_base = top + (DIAL_TOP + layout.date_offset()) * s;
        let date = if reading.error.is_some() { "Check settings".to_string() } else { clock::format_date(reading) };
        let date = if pal.hud { date.to_uppercase() } else { date };
        let date_style = TextStyle { tracking: tracking * 0.6, ..TextStyle::new(&fonts.regular, DATE_PX * s, pal.muted) };
        text::draw(&mut pix, &date_style, &date, cx, date_base);

        // Difference from the master clock.
        let diff_base = date_base + DIFF_GAP * s;
        let (diff, font, color) = if reading.error.is_some() {
            ("Unknown zone".to_string(), &fonts.bold, pal.error)
        } else if reading.is_master {
            (clock::diff_line(reading), &fonts.bold, pal.master)
        } else {
            (clock::diff_line(reading), &fonts.regular, pal.diff)
        };
        let diff = if pal.hud && reading.is_master { diff.to_uppercase() } else { diff };
        let diff_tracking = if pal.hud && reading.is_master { tracking } else { 0.0 };
        let diff = ellipsize(font, DIFF_PX * s, &diff, col_w - 8.0 * s, diff_tracking);
        let diff_style = TextStyle {
            tracking: diff_tracking,
            glow: if reading.is_master { glow_of(color) } else { None },
            ..TextStyle::new(font, DIFF_PX * s, color)
        };
        text::draw(&mut pix, &diff_style, &diff, cx, diff_base);
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
    radial(&mut pix, cx, cy, 305.0, 0.0, r * 0.5, 1.7 * s, ink, false);
    radial(&mut pix, cx, cy, 60.0, 0.0, r * 0.72, 1.4 * s, ink, false);
    fill_circle(&mut pix, cx, cy, 1.2 * s, rgba(255, 138, 91, 1.0));
    pix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClockConfig;

    #[test]
    fn size_grows_with_clocks_and_scale() {
        let analog = Layout::for_config(&Config::default());
        let (w1, h1) = panel_size(1, 1.0, &analog);
        let (w5, h5) = panel_size(5, 1.0, &analog);
        assert_eq!(h1, h5);
        assert!(w5 > w1 * 4);
        let (w5_big, _) = panel_size(5, 1.5, &analog);
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

    #[test]
    fn custom_colours_override_the_theme() {
        let mut cfg = Config::default();
        let before = Palette::for_config(&cfg);
        assert_eq!(before.color_of(ColorField::Accent), (255, 138, 91));
        cfg.colors.set(ColorField::Accent, (0, 200, 83));
        cfg.colors.set(ColorField::Background, (12, 34, 56));
        let after = Palette::for_config(&cfg);
        assert_eq!(after.color_of(ColorField::Accent), (0, 200, 83));
        assert_eq!(after.color_of(ColorField::Background), (12, 34, 56));
        // Untouched colours still come from the theme.
        assert_eq!(after.color_of(ColorField::DayHands), before.color_of(ColorField::DayHands));
    }

    fn render_with(cfg: &Config) -> Pixmap {
        let fonts = Fonts::load_system().expect("a font for tests");
        let clocks = crate::resolve_clocks(cfg);
        let now: jiff::Timestamp = "2026-10-28T01:47:30Z".parse().unwrap();
        crate::render_now(now, cfg, &clocks, 1.0, &fonts)
    }

    #[test]
    fn every_theme_and_dial_style_renders() {
        for theme in Theme::ALL {
            for dial in DialStyle::ALL {
                let cfg = Config { theme, dial, ..Config::default() };
                let pix = render_with(&cfg);
                // Centre of the first dial is drawn on (not transparent).
                let (cx, cy) = ((PAD_X + COL_W / 2.0) as u32, (PAD_Y + DIAL_TOP + DIAL_D / 2.0) as u32);
                assert_eq!(pix.pixel(cx, cy).unwrap().alpha(), 255, "{theme:?} {dial:?}");
            }
        }
    }

    #[test]
    fn every_digital_layout_renders_in_every_theme() {
        use crate::config::DigitalStyle;
        for theme in Theme::ALL {
            for digital in DigitalStyle::ALL {
                let cfg = Config { theme, display: Display::Digital, digital, ..Config::default() };
                let pix = render_with(&cfg);
                let layout = Layout::for_config(&cfg);
                assert_eq!((pix.width(), pix.height()), panel_size(4, 1.0, &layout));
                // Inside the first flip card, the LED screen, the ring and the binary screen alike.
                let (cx, cy) = ((PAD_X + 10.0 + 26.0) as u32, (PAD_Y + DIAL_TOP + DIGITAL_H / 2.0) as u32);
                assert_eq!(pix.pixel(cx, cy).unwrap().alpha(), 255, "{theme:?} {digital:?}");
            }
        }
    }

    #[test]
    fn digital_clocks_are_shorter_except_binary() {
        use crate::config::DigitalStyle;
        let height = |cfg: &Config| panel_size(1, 1.0, &Layout::for_config(cfg)).1;
        let analog = height(&Config::default());
        let flip = height(&Config { display: Display::Digital, ..Config::default() });
        let binary = height(&Config { display: Display::Digital, digital: DigitalStyle::Binary, ..Config::default() });
        assert!(flip < analog && binary < analog && flip < binary, "{flip} {binary} {analog}");
    }

    #[test]
    fn a_flip_in_progress_draws_differently_from_a_still_card() {
        let cfg = Config { display: Display::Digital, ..Config::default() };
        let fonts = Fonts::load_system().expect("a font for tests");
        let clocks = crate::resolve_clocks(&cfg);
        let now: jiff::Timestamp = "2026-10-28T01:47:30Z".parse().unwrap();
        let readings = clock::read_all(now, &clocks, 0);
        let frame = |flips: &[Flip]| {
            let mut pix = render_base(&readings, &cfg, 1.0, &fonts);
            draw_hands(&mut pix, &readings, &cfg, 1.0, &fonts, flips);
            pix
        };
        let still = frame(&[]);
        let mid = frame(&[Flip { from: [None, Some("46".into())], progress: [1.0, 0.3] }]);
        let done = frame(&[Flip { from: [None, Some("46".into())], progress: [1.0, 1.0] }]);
        assert_ne!(still.data(), mid.data());
        assert_eq!(still.data(), done.data(), "a finished flip looks like a still card");
    }

    #[test]
    fn smallest_and_largest_sizes_render() {
        use crate::config::Size;
        let fonts = Fonts::load_system().expect("a font for tests");
        let now: jiff::Timestamp = "2026-10-28T01:47:30Z".parse().unwrap();
        for theme in Theme::ALL {
            let cfg = Config { theme, dial: DialStyle::Stopwatch, ..Config::default() };
            let clocks = crate::resolve_clocks(&cfg);
            let tiny = crate::render_now(now, &cfg, &clocks, Size::new(1).factor(), &fonts);
            let huge = crate::render_now(now, &cfg, &clocks, Size::new(100).factor(), &fonts);
            let layout = Layout::for_config(&cfg);
            assert_eq!(panel_size(4, 0.25, &layout), (tiny.width(), tiny.height()));
            assert_eq!(huge.width(), panel_size(4, 2.0, &layout).0);
            assert!(huge.width() > tiny.width() * 7);
        }
    }

    #[test]
    fn a_per_clock_dial_changes_only_that_clock() {
        let base = Config { clocks: vec![ClockConfig::new("UTC", "UTC"), ClockConfig::new("UTC", "UTC")], ..Config::default() };
        let mut mixed = base.clone();
        mixed.clocks[1].dial = Some(DialStyle::Stopwatch);
        let (a, b) = (render_with(&base), render_with(&mixed));
        let col = (COL_W * 1.0) as usize;
        let left = |p: &Pixmap| p.data()[..].chunks(p.width() as usize * 4).map(|row| row[..(PAD_X as usize + col) * 4].to_vec()).collect::<Vec<_>>();
        assert_eq!(left(&a), left(&b), "first clock should be unchanged");
        assert_ne!(a.data(), b.data(), "second clock should differ");
    }
}
