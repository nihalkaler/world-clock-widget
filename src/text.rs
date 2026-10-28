//! Text drawing: glyph outlines from ab_glyph filled as anti-aliased tiny-skia paths.
//!
//! The widget borrows fonts Windows already has (Segoe UI, Bahnschrift,
//! Georgia), so nothing is bundled into the exe.

use ab_glyph::{Font, FontVec, OutlineCurve, Point, PxScale, ScaleFont, VariableFont};
use std::path::{Path, PathBuf};
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct Fonts {
    /// Body text (Segoe UI).
    pub regular: FontVec,
    /// Labels and times (Segoe UI Semibold).
    pub bold: FontVec,
    /// Techy digits for the Cyberpunk theme (Bahnschrift).
    pub display: FontVec,
    /// Roman numerals (Georgia Bold).
    pub serif: FontVec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
}

fn load(path: &Path) -> Option<FontVec> {
    let data = std::fs::read(path).ok()?;
    FontVec::try_from_vec(data).ok()
}

fn first_loadable(dirs: &[PathBuf], names: &[&str]) -> Option<FontVec> {
    names
        .iter()
        .flat_map(|name| dirs.iter().map(move |dir| dir.join(name)))
        .find_map(|path| load(&path))
}

fn copy(font: &FontVec) -> FontVec {
    FontVec::try_from_vec(font.as_slice().to_vec()).expect("already parsed once")
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(windir) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
    }
    // Stand-ins used when rendering previews on Linux.
    for dir in ["opentype/inter", "truetype/dejavu", "truetype/liberation"] {
        dirs.push(PathBuf::from("/usr/share/fonts").join(dir));
    }
    dirs
}

impl Fonts {
    /// Windows' own fonts; Inter, DejaVu or Liberation elsewhere (for previews).
    pub fn load_system() -> Result<Fonts, String> {
        let dirs = font_dirs();
        let regular = first_loadable(
            &dirs,
            &["segoeui.ttf", "Inter-Regular.otf", "arial.ttf", "tahoma.ttf", "DejaVuSans.ttf"],
        )
        .ok_or("Couldn't find a usable font (looked for Segoe UI and Arial).")?;
        let bold = first_loadable(
            &dirs,
            &["seguisb.ttf", "segoeuib.ttf", "Inter-SemiBold.otf", "arialbd.ttf", "tahomabd.ttf", "DejaVuSans-Bold.ttf"],
        )
        .unwrap_or_else(|| copy(&regular));
        let mut display = first_loadable(&dirs, &["bahnschrift.ttf", "DejaVuSansCondensed-Bold.ttf"])
            .unwrap_or_else(|| copy(&bold));
        // Bahnschrift is a variable font; pick its semibold weight.
        display.set_variation(b"wght", 600.0);
        let serif = first_loadable(&dirs, &["georgiab.ttf", "timesbd.ttf", "LiberationSerif-Bold.ttf", "DejaVuSerif-Bold.ttf"])
            .unwrap_or_else(|| copy(&bold));
        Ok(Fonts { regular, bold, display, serif })
    }

    pub fn from_files(regular: &Path, bold: &Path) -> Result<Fonts, String> {
        let regular = load(regular).ok_or_else(|| format!("Can't load font {}", regular.display()))?;
        let bold = load(bold).ok_or_else(|| format!("Can't load font {}", bold.display()))?;
        let (display, serif) = (copy(&bold), copy(&bold));
        Ok(Fonts { regular, bold, display, serif })
    }
}

/// ab_glyph's PxScale is the ascent-to-descent height; convert from an em size
/// (like CSS font-size) so different fonts come out the same visual size.
fn px_scale(font: &FontVec, em_px: f32) -> PxScale {
    let upem = font.units_per_em().unwrap_or(1000.0);
    let height = font.ascent_unscaled() - font.descent_unscaled();
    PxScale::from(em_px * height / upem)
}

/// Width of `text`. `tracking` is extra space between letters, in ems.
pub fn text_width(font: &FontVec, em_px: f32, text: &str, tracking: f32) -> f32 {
    let scaled = font.as_scaled(px_scale(font, em_px));
    let mut width = 0.0;
    let mut prev = None;
    let mut count = 0;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            width += scaled.kern(p, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
        count += 1;
    }
    width + tracking * em_px * (count.max(1) - 1) as f32
}

/// Shortens text with "…" until it fits in `max_width`.
pub fn ellipsize(font: &FontVec, em_px: f32, text: &str, max_width: f32, tracking: f32) -> String {
    if text_width(font, em_px, text, tracking) <= max_width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if text_width(font, em_px, &candidate, tracking) <= max_width {
            return candidate;
        }
    }
    "…".into()
}

/// How a run of text should look.
#[derive(Clone, Copy)]
pub struct TextStyle<'a> {
    pub font: &'a FontVec,
    pub size: f32,
    pub color: Color,
    pub align: Align,
    /// Extra letter spacing in ems.
    pub tracking: f32,
    /// A soft neon halo behind the letters.
    pub glow: Option<Color>,
}

impl<'a> TextStyle<'a> {
    pub fn new(font: &'a FontVec, size: f32, color: Color) -> Self {
        TextStyle { font, size, color, align: Align::Center, tracking: 0.0, glow: None }
    }
}

/// Draws `text` with its baseline at `baseline`. Returns the drawn width.
pub fn draw(pixmap: &mut Pixmap, style: &TextStyle, text: &str, x: f32, baseline: f32) -> f32 {
    let font = style.font;
    let scaled = font.as_scaled(px_scale(font, style.size));
    let (hs, vs) = (scaled.h_scale_factor(), scaled.v_scale_factor());
    let width = text_width(font, style.size, text, style.tracking);
    let mut pen_x = match style.align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
    };

    let mut pb = PathBuilder::new();
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            pen_x += scaled.kern(p, id);
        }
        if let Some(outline) = font.outline(id) {
            let ox = pen_x;
            let tx = |p: Point| (ox + p.x * hs, baseline - p.y * vs);
            let mut last: Option<Point> = None;
            for curve in &outline.curves {
                let start = match curve {
                    OutlineCurve::Line(a, _) | OutlineCurve::Quad(a, _, _) | OutlineCurve::Cubic(a, _, _, _) => *a,
                };
                if last.is_none_or(|l| (l.x - start.x).abs() > 1e-3 || (l.y - start.y).abs() > 1e-3) {
                    if last.is_some() {
                        pb.close();
                    }
                    let (sx, sy) = tx(start);
                    pb.move_to(sx, sy);
                }
                match curve {
                    OutlineCurve::Line(_, b) => {
                        let (bx, by) = tx(*b);
                        pb.line_to(bx, by);
                        last = Some(*b);
                    }
                    OutlineCurve::Quad(_, c, b) => {
                        let ((cx, cy), (bx, by)) = (tx(*c), tx(*b));
                        pb.quad_to(cx, cy, bx, by);
                        last = Some(*b);
                    }
                    OutlineCurve::Cubic(_, c1, c2, b) => {
                        let ((c1x, c1y), (c2x, c2y), (bx, by)) = (tx(*c1), tx(*c2), tx(*b));
                        pb.cubic_to(c1x, c1y, c2x, c2y, bx, by);
                        last = Some(*b);
                    }
                }
            }
            if last.is_some() {
                pb.close();
            }
        }
        pen_x += scaled.h_advance(id) + style.tracking * style.size;
        prev = Some(id);
    }

    let Some(path) = pb.finish() else { return width };
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    if let Some(glow) = style.glow {
        for (spread, alpha) in [(0.32, 0.10), (0.18, 0.18)] {
            let mut c = glow;
            c.set_alpha(glow.alpha() * alpha);
            paint.set_color(c);
            let stroke = Stroke {
                width: style.size * spread,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    paint.set_color(style.color);
    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    width
}

/// Plain centred or left-aligned text.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    pixmap: &mut Pixmap,
    font: &FontVec,
    em_px: f32,
    text: &str,
    x: f32,
    baseline: f32,
    align: Align,
    color: Color,
) -> f32 {
    let style = TextStyle { align, ..TextStyle::new(font, em_px, color) };
    draw(pixmap, &style, text, x, baseline)
}
