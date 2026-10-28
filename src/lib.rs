//! World Clock Widget: the platform-independent parts (time maths, settings,
//! drawing). The Windows window itself lives in `src/win.rs`.

pub mod cities;
pub mod clock;
pub mod config;
pub mod digital;
pub mod render;
pub mod text;

use clock::ResolvedClock;
use config::Config;

/// Looks up every clock's time zone.
pub fn resolve_clocks(cfg: &Config) -> Vec<ResolvedClock> {
    cfg.clocks.iter().map(clock::resolve).collect()
}

/// Renders the widget for `now`, ready to show or save.
pub fn render_now(
    now: jiff::Timestamp,
    cfg: &Config,
    clocks: &[ResolvedClock],
    scale: f32,
    fonts: &text::Fonts,
) -> tiny_skia::Pixmap {
    let readings = clock::read_all(now, clocks, cfg.master_index());
    render::render(&readings, cfg, scale, fonts)
}
