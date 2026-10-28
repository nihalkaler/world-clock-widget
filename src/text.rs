//! Text drawing: glyph outlines from ab_glyph filled as anti-aliased tiny-skia paths.
//!
//! The widget borrows the fonts Windows already has (Segoe UI), so nothing is
//! bundled into the exe.

use ab_glyph::{Font, FontVec, OutlineCurve, Point, PxScale, ScaleFont};
use std::path::{Path, PathBuf};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

pub struct Fonts {
    pub regular: FontVec,
    pub bold: FontVec,
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

impl Fonts {
    /// Segoe UI on Windows; Inter or DejaVu Sans elsewhere (used for previews).
    pub fn load_system() -> Result<Fonts, String> {
        let mut dirs = Vec::new();
        if let Some(windir) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
        dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
        }
        dirs.push(PathBuf::from("/usr/share/fonts/opentype/inter"));
        dirs.push(PathBuf::from("/usr/share/fonts/truetype/dejavu"));

        let regular = first_loadable(
            &dirs,
            &["segoeui.ttf", "Inter-Regular.otf", "arial.ttf", "tahoma.ttf", "DejaVuSans.ttf"],
        )
        .ok_or("Couldn't find a usable font (looked for Segoe UI and Arial).")?;
        let bold = first_loadable(
            &dirs,
            &[
                "seguisb.ttf",
                "segoeuib.ttf",
                "Inter-SemiBold.otf",
                "arialbd.ttf",
                "tahomabd.ttf",
                "DejaVuSans-Bold.ttf",
            ],
        );
        let bold = match bold {
            Some(f) => f,
            None => FontVec::try_from_vec(regular.as_slice().to_vec()).map_err(|e| e.to_string())?,
        };
        Ok(Fonts { regular, bold })
    }

    pub fn from_files(regular: &Path, bold: &Path) -> Result<Fonts, String> {
        let regular = load(regular).ok_or_else(|| format!("Can't load font {}", regular.display()))?;
        let bold = load(bold).ok_or_else(|| format!("Can't load font {}", bold.display()))?;
        Ok(Fonts { regular, bold })
    }
}

/// ab_glyph's PxScale is the ascent-to-descent height; convert from an em size
/// (like CSS font-size) so different fonts come out the same visual size.
fn px_scale(font: &FontVec, em_px: f32) -> PxScale {
    let upem = font.units_per_em().unwrap_or(1000.0);
    let height = font.ascent_unscaled() - font.descent_unscaled();
    PxScale::from(em_px * height / upem)
}

pub fn text_width(font: &FontVec, em_px: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(px_scale(font, em_px));
    let mut width = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            width += scaled.kern(p, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Shortens text with "…" until it fits in `max_width`.
pub fn ellipsize(font: &FontVec, em_px: f32, text: &str, max_width: f32) -> String {
    if text_width(font, em_px, text) <= max_width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if text_width(font, em_px, &candidate) <= max_width {
            return candidate;
        }
    }
    "…".into()
}

/// Draws `text` with its baseline at `baseline`. Returns the drawn width.
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
    let scale = px_scale(font, em_px);
    let scaled = font.as_scaled(scale);
    let (hs, vs) = (scaled.h_scale_factor(), scaled.v_scale_factor());
    let width = text_width(font, em_px, text);
    let mut pen_x = match align {
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
                if last.map_or(true, |l| (l.x - start.x).abs() > 1e-3 || (l.y - start.y).abs() > 1e-3) {
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
        pen_x += scaled.h_advance(id);
        prev = Some(id);
    }

    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    width
}
