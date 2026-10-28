//! Settings, stored as a small TOML file the user can also edit by hand.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Most clocks the widget will show side by side.
pub const MAX_CLOCKS: usize = 5;

/// The special zone name meaning "whatever time zone this PC is set to".
pub const LOCAL_ZONE: &str = "local";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    pub fn factor(self) -> f32 {
        match self {
            Size::Small => 0.8,
            Size::Medium => 1.0,
            Size::Large => 1.25,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// Where the widget sits relative to other windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// Pinned to the desktop: behind every other window.
    #[default]
    Desktop,
    /// An ordinary window.
    Normal,
    /// Always on top of other windows.
    Top,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockConfig {
    /// Name shown above the clock. Defaults to the city in the zone name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// An IANA time zone such as "Europe/London", or "local".
    pub zone: String,
}

impl ClockConfig {
    pub fn new(label: impl Into<String>, zone: impl Into<String>) -> Self {
        ClockConfig { label: Some(label.into()), zone: zone.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Which clock (1 = leftmost) the time differences are measured from.
    pub master: usize,
    pub use_24_hour: bool,
    pub second_hand: bool,
    pub size: Size,
    pub theme: Theme,
    pub layer: Layer,
    /// When locked, the widget can't be dragged by accident.
    pub locked: bool,
    /// Background opacity, from 0.3 to 1.0.
    pub opacity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(rename = "clock")]
    pub clocks: Vec<ClockConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            master: 1,
            use_24_hour: false,
            second_hand: true,
            size: Size::Medium,
            theme: Theme::Dark,
            layer: Layer::Desktop,
            locked: false,
            opacity: 0.85,
            x: None,
            y: None,
            clocks: vec![
                ClockConfig { label: None, zone: LOCAL_ZONE.into() },
                ClockConfig::new("London", "Europe/London"),
                ClockConfig::new("New York", "America/New_York"),
                ClockConfig::new("Tokyo", "Asia/Tokyo"),
            ],
        }
    }
}

const HEADER: &str = "\
# World Clock Widget settings.
# You can edit this file while the widget is running; changes show up within a second.
#
#   master       which clock (1 = leftmost) time differences are measured from
#   use_24_hour  true for 14:30, false for 2:30 PM
#   second_hand  show the red second hand
#   size         \"small\", \"medium\" or \"large\"
#   theme        \"dark\" or \"light\"
#   layer        \"desktop\" (behind other windows), \"normal\" or \"top\" (always on top)
#   locked       true stops the widget being dragged
#   opacity      background opacity from 0.3 to 1.0
#   x, y         screen position of the widget's top-left corner
#
# Up to 5 [[clock]] entries. `zone` is an IANA time zone name
# (https://en.wikipedia.org/wiki/List_of_tz_database_time_zones) or \"local\"
# for this PC's own time zone. `label` is optional.

";

impl Config {
    /// Parses settings text, then fixes anything out of range.
    pub fn from_toml(text: &str) -> Result<Config, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(cfg.normalized())
    }

    pub fn to_toml(&self) -> String {
        let body = toml::to_string(self).unwrap_or_default();
        format!("{HEADER}{body}")
    }

    /// Clamps every setting into a range the widget can draw.
    pub fn normalized(mut self) -> Config {
        self.clocks.retain(|c| !c.zone.trim().is_empty());
        self.clocks.truncate(MAX_CLOCKS);
        if self.clocks.is_empty() {
            self.clocks.push(ClockConfig { label: None, zone: LOCAL_ZONE.into() });
        }
        self.master = self.master.clamp(1, self.clocks.len());
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.3, 1.0) } else { 0.85 };
        for clock in &mut self.clocks {
            clock.zone = clock.zone.trim().to_string();
            if let Some(label) = &clock.label {
                if label.trim().is_empty() {
                    clock.label = None;
                }
            }
        }
        self
    }

    /// Zero-based index of the master clock.
    pub fn master_index(&self) -> usize {
        self.master.saturating_sub(1).min(self.clocks.len().saturating_sub(1))
    }

    /// Removes a clock, keeping the master pointing at the same clock where possible.
    pub fn remove_clock(&mut self, index: usize) {
        if self.clocks.len() <= 1 || index >= self.clocks.len() {
            return;
        }
        let master = self.master_index();
        self.clocks.remove(index);
        let new_master = if index < master {
            master - 1
        } else if index == master {
            0
        } else {
            master
        };
        self.master = new_master + 1;
    }

    /// Adds a clock on the right if there's room. Returns false when full.
    pub fn add_clock(&mut self, clock: ClockConfig) -> bool {
        if self.clocks.len() >= MAX_CLOCKS {
            return false;
        }
        self.clocks.push(clock);
        true
    }

    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Config::from_toml(&text)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename so a crash can't leave a half-written file.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_toml())?;
        std::fs::rename(&tmp, path)
    }
}

/// `%APPDATA%\WorldClockWidget\config.toml` on Windows.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("WorldClockWidget").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_toml() {
        let cfg = Config::default();
        let text = cfg.to_toml();
        assert!(text.starts_with("# World Clock Widget settings."));
        assert_eq!(Config::from_toml(&text).unwrap(), cfg);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let cfg = Config::from_toml("[[clock]]\nzone = \"Asia/Tokyo\"\n").unwrap();
        assert_eq!(cfg.clocks.len(), 1);
        assert_eq!(cfg.master, 1);
        assert!(cfg.second_hand);
        assert_eq!(cfg.layer, Layer::Desktop);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let text = "master = 9\nopacity = 4.0\n".to_string()
            + &"[[clock]]\nzone = \"UTC\"\n".repeat(7);
        let cfg = Config::from_toml(&text).unwrap();
        assert_eq!(cfg.clocks.len(), MAX_CLOCKS);
        assert_eq!(cfg.master, MAX_CLOCKS);
        assert_eq!(cfg.opacity, 1.0);
    }

    #[test]
    fn empty_clock_list_gets_a_local_clock() {
        let cfg = Config::from_toml("clock = []").unwrap();
        assert_eq!(cfg.clocks, vec![ClockConfig { label: None, zone: "local".into() }]);
    }

    #[test]
    fn bad_toml_is_an_error() {
        assert!(Config::from_toml("size = \"enormous\"").is_err());
        assert!(Config::from_toml("this is not toml").is_err());
    }

    #[test]
    fn removing_clocks_keeps_the_master_on_the_same_clock() {
        let mut cfg = Config { master: 3, ..Config::default() };
        let master_zone = cfg.clocks[2].zone.clone();
        cfg.remove_clock(0);
        assert_eq!(cfg.clocks[cfg.master_index()].zone, master_zone);
        cfg.remove_clock(cfg.master_index());
        assert_eq!(cfg.master, 1);
        // The last clock can't be removed.
        let mut one = Config { clocks: vec![ClockConfig::new("UTC", "UTC")], ..Config::default() };
        one.remove_clock(0);
        assert_eq!(one.clocks.len(), 1);
    }

    #[test]
    fn adding_stops_at_five() {
        let mut cfg = Config::default();
        assert!(cfg.add_clock(ClockConfig::new("Paris", "Europe/Paris")));
        assert!(!cfg.add_clock(ClockConfig::new("Rome", "Europe/Rome")));
        assert_eq!(cfg.clocks.len(), MAX_CLOCKS);
    }
}
