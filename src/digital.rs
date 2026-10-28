//! Digital clock layouts: flip cards, LED segments, a seconds ring and binary.

use crate::clock::Reading;
use crate::config::{Config, DigitalStyle};
use crate::render::{arc, circle, fill, fill_circle, paint, rgba, rounded_rect, stroke, with_alpha, Palette};
use crate::text::{self, text_width, Align, Fonts, TextStyle};
use ab_glyph::FontVec;
use std::f32::consts::PI;
use tiny_skia::{
    Color, FillRule, FilterQuality, Mask, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform,
};

/// Where one clock's digital display goes, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A flip animation in progress for one clock: what each card (hours,
/// minutes) showed before it started flipping, and how far through the flip
/// it is, from 0 to 1.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Flip {
    pub from: [Option<String>; 2],
    pub progress: [f32; 2],
}

/// How long one card takes to flip over.
pub const FLIP_SECONDS: f32 = 0.45;

/// Binary is hard to read at a glance, so it keeps the written-out time.
pub fn shows_time_line(style: DigitalStyle) -> bool {
    style == DigitalStyle::Binary
}

/// Hours as a digital clock shows them: "4" in 12-hour time, "04" in 24-hour.
pub fn hour_text(hour: u8, use_24_hour: bool) -> String {
    if use_24_hour {
        format!("{hour:02}")
    } else {
        match hour % 12 {
            0 => "12".to_string(),
            h => h.to_string(),
        }
    }
}

/// What the two flip cards show: hours, then minutes.
pub fn card_texts(reading: &Reading, use_24_hour: bool) -> [String; 2] {
    if reading.error.is_some() {
        return ["--".into(), "--".into()];
    }
    [hour_text(reading.hour, use_24_hour), format!("{:02}", reading.minute)]
}

fn display_hour(reading: &Reading, use_24_hour: bool) -> u8 {
    if use_24_hour {
        reading.hour
    } else {
        match reading.hour % 12 {
            0 => 12,
            h => h,
        }
    }
}

fn am_pm(reading: &Reading, cfg: &Config) -> Option<&'static str> {
    if cfg.use_24_hour || reading.error.is_some() {
        None
    } else {
        Some(if reading.hour < 12 { "AM" } else { "PM" })
    }
}

struct Look {
    face: Color,
    ink: Color,
    frame: Color,
    frame_w: f32,
    glow: bool,
}

fn look(reading: &Reading, pal: &Palette, s: f32) -> Look {
    let (face, ink) = if reading.is_daytime() { (pal.day_face, pal.day_ink) } else { (pal.night_face, pal.night_ink) };
    let (frame, frame_w) = if reading.is_master { (pal.master, 2.0 * s) } else { (pal.ring, 1.2 * s) };
    Look { face, ink, frame, frame_w, glow: pal.glow }
}

fn digit_font<'a>(fonts: &'a Fonts, pal: &Palette) -> &'a FontVec {
    if pal.hud {
        &fonts.display
    } else {
        &fonts.bold
    }
}

/// Draws one clock's digital display into `area`.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pix: &mut Pixmap,
    fonts: &Fonts,
    area: &Area,
    reading: &Reading,
    cfg: &Config,
    pal: &Palette,
    s: f32,
    flip: Option<&Flip>,
) {
    let look = look(reading, pal, s);
    match cfg.digital {
        DigitalStyle::Flip => draw_flip(pix, fonts, area, reading, cfg, pal, &look, s, flip),
        DigitalStyle::Segment => draw_segments(pix, fonts, area, reading, cfg, pal, &look, s),
        DigitalStyle::Ring => draw_ring(pix, fonts, area, reading, cfg, pal, &look, s),
        DigitalStyle::Binary => draw_binary(pix, area, reading, cfg, &look, s),
    }
}

/// The rounded "screen" behind the LED and binary layouts.
fn screen(pix: &mut Pixmap, area: &Area, look: &Look, s: f32) -> Area {
    let inner = Area { x: area.x, y: area.y + 3.0 * s, w: area.w, h: area.h - 6.0 * s };
    if let Some(shape) = rounded_rect(inner.x, inner.y, inner.w, inner.h, 8.0 * s) {
        fill(pix, &shape, look.face);
        stroke(pix, &shape, look.frame_w, look.frame, false);
    }
    inner
}

// ------------------------------------------------------------- flip clock

/// One flip card as its own picture, so it can be squashed while flipping.
#[allow(clippy::too_many_arguments)]
fn card(fonts: &Fonts, pal: &Palette, look: &Look, label: &str, ampm: Option<&str>, w: f32, h: f32, s: f32) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32)?;
    let shape = rounded_rect(0.5, 0.5, w - 1.0, h - 1.0, 6.0 * s)?;
    fill(&mut pm, &shape, look.face);

    // The lower flap sits slightly in shadow, like a real flip clock.
    if let (Some(mut mask), Some(lower)) = (Mask::new(pm.width(), pm.height()), Rect::from_xywh(0.0, h / 2.0, w, h / 2.0)) {
        mask.fill_path(&shape, FillRule::Winding, true, Transform::identity());
        pm.fill_rect(lower, &paint(rgba(0, 0, 0, 0.08)), Transform::identity(), Some(&mask));
    }

    let font = digit_font(fonts, pal);
    let mut size = h * 0.66;
    let fit = w * 0.86;
    let width = text_width(font, size, label, 0.0);
    if width > fit {
        size *= fit / width;
    }
    let digits = TextStyle { glow: look.glow.then_some(look.ink), ..TextStyle::new(font, size, look.ink) };
    text::draw(&mut pm, &digits, label, w / 2.0, h / 2.0 + size * 0.36);
    if let Some(t) = ampm {
        let small = TextStyle { align: Align::Left, ..TextStyle::new(&fonts.bold, h * 0.14, with_alpha(look.ink, 0.6)) };
        text::draw(&mut pm, &small, t, 4.0 * s, h - 4.0 * s);
    }

    // The split between the two flaps, and the hinge pins at each side.
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, h / 2.0);
    pb.line_to(w, h / 2.0);
    if let Some(split) = pb.finish() {
        let thin = Stroke { width: 1.4 * s, ..Stroke::default() };
        pm.stroke_path(&split, &paint(rgba(0, 0, 0, 0.45)), &thin, Transform::identity(), None);
    }
    for x in [0.0, w - 2.0 * s] {
        if let Some(pin) = Rect::from_xywh(x, h / 2.0 - 3.5 * s, 2.0 * s, 7.0 * s) {
            pm.fill_rect(pin, &paint(rgba(0, 0, 0, 0.45)), Transform::identity(), None);
        }
    }
    stroke(&mut pm, &shape, look.frame_w, look.frame, false);
    Some(pm)
}

fn draw_card(pix: &mut Pixmap, card: &Pixmap, transform: Transform, mask: Option<&Mask>) {
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    pix.draw_pixmap(0, 0, card.as_ref(), &paint, transform, mask);
}

fn rect_mask(pix: &Pixmap, x: f32, y: f32, w: f32, h: f32) -> Option<Mask> {
    let mut mask = Mask::new(pix.width(), pix.height())?;
    let path = PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?);
    mask.fill_path(&path, FillRule::Winding, false, Transform::identity());
    Some(mask)
}

/// Draws a card part-way through flipping from `old` to `new`.
/// First the top half of the old card folds down to the middle, then the
/// bottom half of the new card folds down from the middle.
#[allow(clippy::too_many_arguments)]
fn draw_flipping(pix: &mut Pixmap, old: &Pixmap, new: &Pixmap, x: f32, y: f32, w: f32, h: f32, t: f32) {
    let mid = y + h / 2.0;
    let (Some(top), Some(bottom)) =
        (rect_mask(pix, x - 2.0, y - 2.0, w + 4.0, h / 2.0 + 2.0), rect_mask(pix, x - 2.0, mid, w + 4.0, h / 2.0 + 2.0))
    else {
        draw_card(pix, new, Transform::from_translate(x, y), None);
        return;
    };
    // Behind the moving flap: the new top half and the old bottom half.
    draw_card(pix, new, Transform::from_translate(x, y), Some(&top));
    draw_card(pix, old, Transform::from_translate(x, y), Some(&bottom));

    let phase = t.clamp(0.0, 1.0) * PI;
    let (card, k, mask, shade) = if t < 0.5 {
        let k = phase.cos().max(0.0);
        (old, k, &top, (1.0 - k) * 0.35)
    } else {
        let k = (-phase.cos()).max(0.0);
        (new, k, &bottom, (1.0 - k) * 0.25)
    };
    if k > 0.01 {
        // Squash the card towards the middle line to fake the 3D turn.
        let squash = Transform::from_row(1.0, 0.0, 0.0, k, x, mid - h / 2.0 * k);
        draw_card(pix, card, squash, Some(mask));
        let flap_y = if t < 0.5 { mid - h / 2.0 * k } else { mid };
        if let Some(r) = Rect::from_xywh(x, flap_y, w, h / 2.0 * k) {
            pix.fill_rect(r, &paint(rgba(0, 0, 0, shade)), Transform::identity(), Some(mask));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_flip(
    pix: &mut Pixmap,
    fonts: &Fonts,
    area: &Area,
    reading: &Reading,
    cfg: &Config,
    pal: &Palette,
    look: &Look,
    s: f32,
    flip: Option<&Flip>,
) {
    let seconds = cfg.second_hand && reading.error.is_none();
    let gap = 8.0 * s;
    let card_w = (area.w - gap) / 2.0;
    let card_h = area.h - if seconds { 11.0 * s } else { 4.0 * s };
    let y = area.y + 1.0 * s;
    let texts = card_texts(reading, cfg.use_24_hour);
    let ampm = am_pm(reading, cfg);

    for (i, label) in texts.iter().enumerate() {
        let x = area.x + i as f32 * (card_w + gap);
        let corner = if i == 0 { ampm } else { None };
        let Some(now) = card(fonts, pal, look, label, corner, card_w, card_h, s) else { continue };
        let moving = flip.and_then(|f| {
            let from = f.from[i].as_ref()?;
            (f.progress[i] < 1.0 && from != label).then_some((from, f.progress[i]))
        });
        match moving.and_then(|(from, t)| Some((card(fonts, pal, look, from, corner, card_w, card_h, s)?, t))) {
            Some((old, t)) => draw_flipping(pix, &old, &now, x, y, card_w, card_h, t),
            None => draw_card(pix, &now, Transform::from_translate(x, y), None),
        }
    }

    // A thin bar that fills up over each minute.
    if seconds {
        let bar_y = y + card_h + 5.0 * s;
        let bar_h = 3.0 * s;
        if let Some(track) = rounded_rect(area.x, bar_y, area.w, bar_h, bar_h / 2.0) {
            fill(pix, &track, with_alpha(look.ink, 0.15));
        }
        let done = area.w * (reading.second as f32 + 1.0) / 60.0;
        if let Some(bar) = rounded_rect(area.x, bar_y, done.max(bar_h), bar_h, bar_h / 2.0) {
            fill(pix, &bar, pal.accent);
        }
    }
}

// ------------------------------------------------------------- LED segments

/// Which of the seven segments (a to g, bits 0 to 6) light up for each digit.
pub const SEGMENTS: [u8; 10] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F];
const DASH: u8 = 0x40;

/// Draws a seven-segment digit. Unlit segments stay faintly visible, like a real LED display.
#[allow(clippy::too_many_arguments)]
fn seven_segment(pix: &mut Pixmap, ox: f32, oy: f32, w: f32, h: f32, bits: u8, on: Color, off: Color, glow: bool) {
    let t = w * 0.2;
    let g = t * 0.2;
    let (left, right) = (ox + t / 2.0, ox + w - t / 2.0);
    let (top, mid, bottom) = (oy + t / 2.0, oy + h / 2.0, oy + h - t / 2.0);
    let slant = |x: f32, y: f32| (x + (oy + h / 2.0 - y) * 0.08, y);
    let horizontal = |x0: f32, x1: f32, y: f32| {
        vec![(x0, y), (x0 + t / 2.0, y - t / 2.0), (x1 - t / 2.0, y - t / 2.0), (x1, y), (x1 - t / 2.0, y + t / 2.0), (x0 + t / 2.0, y + t / 2.0)]
    };
    let vertical = |x: f32, y0: f32, y1: f32| {
        vec![(x, y0), (x + t / 2.0, y0 + t / 2.0), (x + t / 2.0, y1 - t / 2.0), (x, y1), (x - t / 2.0, y1 - t / 2.0), (x - t / 2.0, y0 + t / 2.0)]
    };
    let segments = [
        horizontal(left + g, right - g, top),   // a
        vertical(right, top + g, mid - g),      // b
        vertical(right, mid + g, bottom - g),   // c
        horizontal(left + g, right - g, bottom), // d
        vertical(left, mid + g, bottom - g),    // e
        vertical(left, top + g, mid - g),       // f
        horizontal(left + g, right - g, mid),   // g
    ];
    for (i, points) in segments.iter().enumerate() {
        let lit = bits >> i & 1 == 1;
        let mut pb = PathBuilder::new();
        for (j, (x, y)) in points.iter().enumerate() {
            let (x, y) = slant(*x, *y);
            if j == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
        pb.close();
        if let Some(path) = pb.finish() {
            fill(pix, &path, if lit { on } else { off });
            if lit && glow {
                stroke(pix, &path, t * 0.3, with_alpha(on, 0.5), true);
            }
        }
    }
}

fn digit_bits(digit: Option<u8>) -> u8 {
    digit.map_or(0, |d| SEGMENTS[(d % 10) as usize])
}

#[allow(clippy::too_many_arguments)]
fn draw_segments(pix: &mut Pixmap, fonts: &Fonts, area: &Area, reading: &Reading, cfg: &Config, pal: &Palette, look: &Look, s: f32) {
    let inner = screen(pix, area, look, s);
    let seconds = cfg.second_hand && reading.error.is_none();
    let (on, off) = (look.ink, with_alpha(look.ink, 0.07));

    // Sizes at 100%, scaled to fit: four big digits, a colon, and small seconds.
    let (dw, dh, gap, colon_w) = (15.0 * s, 32.0 * s, 4.0 * s, 11.0 * s);
    let (sw, sh, sgap) = (7.0 * s, 14.0 * s, 2.0 * s);
    let main_w = dw * 4.0 + gap * 2.0 + colon_w;
    let total = main_w + if seconds { 4.0 * s + sw * 2.0 + sgap } else { 0.0 };
    let x0 = inner.x + (inner.w - total) / 2.0;
    let y0 = inner.y + (inner.h - dh) / 2.0;

    let (big, small): ([u8; 4], [u8; 2]) = if reading.error.is_some() {
        ([DASH; 4], [0; 2])
    } else {
        let h = display_hour(reading, cfg.use_24_hour);
        let tens = if h < 10 && !cfg.use_24_hour { 0 } else { digit_bits(Some(h / 10)) };
        (
            [tens, digit_bits(Some(h % 10)), digit_bits(Some(reading.minute / 10)), digit_bits(Some(reading.minute % 10))],
            [digit_bits(Some(reading.second / 10)), digit_bits(Some(reading.second % 10))],
        )
    };
    let xs = [x0, x0 + dw + gap, x0 + dw * 2.0 + gap + colon_w, x0 + dw * 3.0 + gap * 2.0 + colon_w];
    for (bits, x) in big.iter().zip(xs) {
        seven_segment(pix, x, y0, dw, dh, *bits, on, off, look.glow);
    }

    // The colon blinks with the seconds when they're shown.
    let colon_on = reading.error.is_none() && (!seconds || reading.second.is_multiple_of(2));
    let cx = x0 + dw * 2.0 + gap + colon_w / 2.0;
    for fy in [0.32, 0.68] {
        let y = y0 + dh * fy;
        fill_circle(pix, cx + (0.5 - fy) * dh * 0.08, y, 1.9 * s, if colon_on { on } else { off });
    }

    if seconds {
        let sx = x0 + main_w + 4.0 * s;
        let sy = y0 + dh - sh;
        seven_segment(pix, sx, sy, sw, sh, small[0], on, off, look.glow);
        seven_segment(pix, sx + sw + sgap, sy, sw, sh, small[1], on, off, look.glow);
    }
    if let Some(t) = am_pm(reading, cfg) {
        let x = if seconds { x0 + main_w + 4.0 * s } else { x0 + main_w + 2.0 * s };
        let label = TextStyle { align: Align::Left, ..TextStyle::new(&fonts.bold, 8.0 * s, with_alpha(on, 0.8)) };
        let x = if seconds { x } else { x.min(inner.x + inner.w - 14.0 * s) };
        text::draw(pix, &label, t, x, y0 + 7.0 * s);
    }
    let _ = pal;
}

// ------------------------------------------------------------- ring

#[allow(clippy::too_many_arguments)]
fn draw_ring(pix: &mut Pixmap, fonts: &Fonts, area: &Area, reading: &Reading, cfg: &Config, pal: &Palette, look: &Look, s: f32) {
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    let r = area.h / 2.0 - 2.0 * s;
    fill_circle(pix, cx, cy, r, look.face);
    let track = if reading.is_master { pal.master } else { with_alpha(look.ink, 0.18) };
    circle(pix, cx, cy, r - 2.0 * s, 2.6 * s, track, look.glow && reading.is_master);
    if cfg.second_hand && reading.error.is_none() {
        // Fills clockwise from 12 o'clock over each minute.
        arc(pix, cx, cy, r - 2.0 * s, (reading.second as f32 + 1.0) * 6.0, 2.6 * s, pal.accent, look.glow);
    }

    let font = digit_font(fonts, pal);
    let time = if reading.error.is_some() {
        "--:--".to_string()
    } else {
        format!("{}:{:02}", hour_text(reading.hour, cfg.use_24_hour), reading.minute)
    };
    let mut size = r * 0.6;
    let fit = r * 1.62;
    let width = text_width(font, size, &time, 0.0);
    if width > fit {
        size *= fit / width;
    }
    let ampm = am_pm(reading, cfg);
    let lift = if ampm.is_some() { r * 0.1 } else { 0.0 };
    let digits = TextStyle { glow: look.glow.then_some(look.ink), ..TextStyle::new(font, size, look.ink) };
    text::draw(pix, &digits, &time, cx, cy + size * 0.36 - lift);
    if let Some(t) = ampm {
        let small = TextStyle::new(&fonts.bold, r * 0.24, with_alpha(look.ink, 0.65));
        text::draw(pix, &small, t, cx, cy + r * 0.56);
    }
}

// ------------------------------------------------------------- binary

/// Bits needed for each column: hour tens, hour ones, minute tens and ones,
/// second tens and ones.
const BINARY_BITS: [u8; 6] = [2, 4, 3, 4, 3, 4];

fn draw_binary(pix: &mut Pixmap, area: &Area, reading: &Reading, cfg: &Config, look: &Look, s: f32) {
    let inner = screen(pix, area, look, s);
    let seconds = cfg.second_hand;
    let columns = if seconds { 6 } else { 4 };
    let h = display_hour(reading, cfg.use_24_hour);
    let digits = [h / 10, h % 10, reading.minute / 10, reading.minute % 10, reading.second / 10, reading.second % 10];

    let (col_step, group_gap, row_step, dot) = (13.0 * s, 8.0 * s, 12.0 * s, 4.2 * s);
    let groups = columns / 2;
    let total = (columns - 1) as f32 * col_step + (groups - 1) as f32 * group_gap;
    let x0 = inner.x + (inner.w - total) / 2.0;
    let y0 = inner.y + inner.h / 2.0 - row_step * 1.5;
    for col in 0..columns {
        let x = x0 + col as f32 * col_step + (col / 2) as f32 * group_gap;
        let bits = BINARY_BITS[col];
        for row in (4 - bits as usize)..4 {
            let value = 1u8 << (3 - row);
            let lit = reading.error.is_none() && digits[col] & value != 0;
            let y = y0 + row as f32 * row_step;
            if lit {
                fill_circle(pix, x, y, dot, look.ink);
                if look.glow {
                    circle(pix, x, y, dot * 0.6, dot * 0.6, with_alpha(look.ink, 0.6), true);
                }
            } else {
                fill_circle(pix, x, y, dot, with_alpha(look.ink, 0.13));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_table_matches_the_digits() {
        let lit = |d: usize| SEGMENTS[d].count_ones();
        // How many segments each digit lights on a real display.
        assert_eq!((0..10).map(lit).collect::<Vec<_>>(), [6, 2, 5, 5, 4, 5, 6, 3, 7, 6]);
        assert_eq!(SEGMENTS[1], 0b0000110, "1 is the two right-hand segments");
        assert_eq!(SEGMENTS[8], 0x7F, "8 lights everything");
    }

    #[test]
    fn hours_on_a_digital_clock() {
        assert_eq!(hour_text(0, false), "12");
        assert_eq!(hour_text(9, false), "9");
        assert_eq!(hour_text(13, false), "1");
        assert_eq!(hour_text(9, true), "09");
        assert_eq!(hour_text(23, true), "23");
    }

    #[test]
    fn binary_columns_hold_every_digit() {
        // Largest digit each column can show: 2, 9, 5, 9, 5, 9.
        for (bits, max) in BINARY_BITS.iter().zip([2u8, 9, 5, 9, 5, 9]) {
            assert!(max < 1 << bits);
        }
    }
}
