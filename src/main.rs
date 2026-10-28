// No console window when started normally on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

use std::path::PathBuf;
use world_clock_widget::config::{self, Config};
use world_clock_widget::text::Fonts;

const USAGE: &str = "\
World Clock Widget

Usage:
  world-clock                      start the widget (Windows)
  world-clock --preview OUT.png    save a picture of the widget instead
      [--config FILE]              settings file to use (default: your saved settings)
      [--at TIME]                  e.g. 2026-10-28T01:47:30Z (default: now)
      [--local ZONE]               time zone to use for \"local\" clocks
      [--scale N]                  1.0 = 100% display scaling
      [--theme NAME] [--dial NAME] override the theme or clock face design
      [--fonts REGULAR BOLD]       font files to draw with
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "--preview") {
        if let Err(e) = preview(&args) {
            eprintln!("error: {e}\n\n{USAGE}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(windows)]
    win::run();

    #[cfg(not(windows))]
    {
        eprintln!("The widget itself runs on Windows; use --preview here.\n\n{USAGE}");
        std::process::exit(2);
    }
}

fn value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

fn preview(args: &[String]) -> Result<(), String> {
    let out = value(args, "--preview").ok_or("--preview needs an output file")?;
    let cfg_path = value(args, "--config").map(PathBuf::from).unwrap_or_else(config::default_path);
    let mut cfg = if cfg_path.exists() { Config::load(&cfg_path)? } else { Config::default() };

    // Settings overrides, e.g. --theme light --dial roman, using the same
    // names as the settings file.
    for key in ["theme", "dial"] {
        if let Some(v) = value(args, &format!("--{key}")) {
            let mut text = cfg.to_toml();
            text = text.lines().filter(|l| !l.starts_with(&format!("{key} ="))).collect::<Vec<_>>().join("\n");
            cfg = Config::from_toml(&format!("{key} = \"{v}\"\n{text}"))?;
        }
    }
    if let Some(zone) = value(args, "--local") {
        for clock in &mut cfg.clocks {
            if clock.zone.eq_ignore_ascii_case(config::LOCAL_ZONE) {
                clock.zone = zone.to_string();
            }
        }
    }
    let now = match value(args, "--at") {
        None => jiff::Timestamp::now(),
        Some(t) => t
            .parse::<jiff::Timestamp>()
            .or_else(|_| t.parse::<jiff::Zoned>().map(|z| z.timestamp()))
            .map_err(|e| format!("can't read --at {t}: {e}"))?,
    };
    let scale: f32 = value(args, "--scale").map_or(Ok(1.0), str::parse).map_err(|e| format!("--scale: {e}"))?;
    let fonts = match args.iter().position(|a| a == "--fonts") {
        Some(i) => {
            let (r, b) = (args.get(i + 1), args.get(i + 2));
            let (Some(r), Some(b)) = (r, b) else { return Err("--fonts needs two files".into()) };
            Fonts::from_files(r.as_ref(), b.as_ref())?
        }
        None => Fonts::load_system()?,
    };

    let clocks = world_clock_widget::resolve_clocks(&cfg);
    let pixmap = world_clock_widget::render_now(now, &cfg, &clocks, scale * cfg.size.factor(), &fonts);
    pixmap.save_png(out).map_err(|e| e.to_string())
}
