//! Settings, stored as a small TOML file the user can also edit by hand.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Most clocks the widget will show side by side.
pub const MAX_CLOCKS: usize = 5;

/// The special zone name meaning "whatever time zone this PC is set to".
pub const LOCAL_ZONE: &str = "local";

/// Widget size from 1 to 100. 50 is the normal size, 100 is twice as big
/// and 1 is a quarter of normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size(u8);

impl Size {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 100;
    pub const NORMAL: Size = Size(50);
    /// The ready-made sizes in the menu.
    pub const PRESETS: [u8; 4] = [25, 50, 75, 100];

    /// Any number is accepted and kept within 1 to 100.
    pub fn new(value: i64) -> Size {
        Size(value.clamp(Self::MIN as i64, Self::MAX as i64) as u8)
    }

    pub fn value(self) -> u8 {
        self.0
    }

    /// How much to scale the widget by. 1-50 runs from a quarter of normal
    /// up to normal; 50-100 runs from normal up to double.
    pub fn factor(self) -> f32 {
        let n = self.0 as f32;
        if n <= 50.0 {
            0.25 + (n - 1.0) * 0.75 / 49.0
        } else {
            n / 50.0
        }
    }
}

impl Default for Size {
    fn default() -> Self {
        Size::NORMAL
    }
}

impl Serialize for Size {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(f64),
            Name(String),
        }
        match Raw::deserialize(d)? {
            Raw::Number(n) if n.is_finite() => Ok(Size::new(n.round() as i64)),
            Raw::Number(_) => Err(serde::de::Error::custom("size must be a number from 1 to 100")),
            // Names used by version 1.x settings files.
            Raw::Name(name) => match name.trim().to_ascii_lowercase().as_str() {
                "small" => Ok(Size(37)),
                "medium" => Ok(Size::NORMAL),
                "large" => Ok(Size(63)),
                _ => Err(serde::de::Error::custom(format!(
                    "size \"{name}\" isn't recognised; use a number from 1 to 100 (50 is normal)"
                ))),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    /// Neon magenta and cyan on near-black, with glow and a HUD-style panel.
    Cyberpunk,
    /// Olive and sand field-kit colours with an amber accent, and
    /// night-vision green after dark.
    Tactical,
}

impl Theme {
    pub const ALL: [Theme; 4] = [Theme::Dark, Theme::Light, Theme::Cyberpunk, Theme::Tactical];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Cyberpunk => "Cyberpunk",
            Theme::Tactical => "Tactical",
            Theme::Dark => "Dark",
            Theme::Light => "Light",
        }
    }
}

/// The design of an analog clock face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DialStyle {
    /// Tick marks for every minute and hour.
    #[default]
    Classic,
    /// The hours written as 1 to 12.
    Numbers,
    /// The hours as Roman numerals, with a railway-track minute ring.
    Roman,
    /// Just dots and slim hands.
    Minimal,
    /// An old pocket stopwatch: crown and pushers, a 5-60 scale and a 24-hour sub-dial.
    Stopwatch,
    /// Glowing rings with the seconds drawn as an arc around the edge.
    Neon,
}

impl DialStyle {
    pub const ALL: [DialStyle; 6] = [
        DialStyle::Classic,
        DialStyle::Numbers,
        DialStyle::Roman,
        DialStyle::Minimal,
        DialStyle::Stopwatch,
        DialStyle::Neon,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DialStyle::Classic => "Classic",
            DialStyle::Numbers => "Numbers",
            DialStyle::Roman => "Roman numerals",
            DialStyle::Minimal => "Minimal",
            DialStyle::Stopwatch => "Old stopwatch",
            DialStyle::Neon => "Neon",
        }
    }
}

/// Whether the clocks have hands or show numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Display {
    #[default]
    Analog,
    Digital,
}

/// How a digital clock shows the time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DigitalStyle {
    /// Split-flap cards that flip down when the number changes.
    #[default]
    Flip,
    /// Seven-segment LED digits, like a bedside alarm clock.
    Segment,
    /// Big digits inside a ring that fills up with the seconds.
    Ring,
    /// Each digit as lit dots in binary, with the normal time underneath.
    Binary,
}

impl DigitalStyle {
    pub const ALL: [DigitalStyle; 4] = [DigitalStyle::Flip, DigitalStyle::Segment, DigitalStyle::Ring, DigitalStyle::Binary];

    pub fn name(self) -> &'static str {
        match self {
            DigitalStyle::Flip => "Flip clock",
            DigitalStyle::Segment => "LED segments",
            DigitalStyle::Ring => "Ring",
            DigitalStyle::Binary => "Binary",
        }
    }
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

/// The colours a person can pick themselves, on top of the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorField {
    Background,
    Accent,
    Text,
    DayFace,
    NightFace,
    DayHands,
    NightHands,
}

impl ColorField {
    pub const ALL: [ColorField; 7] = [
        ColorField::Background,
        ColorField::Accent,
        ColorField::Text,
        ColorField::DayFace,
        ColorField::NightFace,
        ColorField::DayHands,
        ColorField::NightHands,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ColorField::Background => "Background",
            ColorField::Accent => "Accent (second hand, master clock)",
            ColorField::Text => "Text",
            ColorField::DayFace => "Clock face by day",
            ColorField::NightFace => "Clock face at night",
            ColorField::DayHands => "Hands and marks by day",
            ColorField::NightHands => "Hands and marks at night",
        }
    }
}

/// Custom colours as "#RRGGBB". Anything left out comes from the theme.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_face: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night_face: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_hands: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night_hands: Option<String>,
}

impl Colors {
    pub fn is_empty(&self) -> bool {
        ColorField::ALL.iter().all(|f| self.get(*f).is_none())
    }

    fn slot(&mut self, field: ColorField) -> &mut Option<String> {
        match field {
            ColorField::Background => &mut self.background,
            ColorField::Accent => &mut self.accent,
            ColorField::Text => &mut self.text,
            ColorField::DayFace => &mut self.day_face,
            ColorField::NightFace => &mut self.night_face,
            ColorField::DayHands => &mut self.day_hands,
            ColorField::NightHands => &mut self.night_hands,
        }
    }

    /// The custom colour for `field`, if one is set and readable.
    pub fn get(&self, field: ColorField) -> Option<(u8, u8, u8)> {
        let value = match field {
            ColorField::Background => &self.background,
            ColorField::Accent => &self.accent,
            ColorField::Text => &self.text,
            ColorField::DayFace => &self.day_face,
            ColorField::NightFace => &self.night_face,
            ColorField::DayHands => &self.day_hands,
            ColorField::NightHands => &self.night_hands,
        };
        value.as_deref().and_then(parse_hex)
    }

    pub fn set(&mut self, field: ColorField, rgb: (u8, u8, u8)) {
        *self.slot(field) = Some(format_hex(rgb));
    }

    /// Checks every colour, so a typo in the file gets a clear message.
    fn validate(&self) -> Result<(), String> {
        let mut copy = self.clone();
        for field in ColorField::ALL {
            if let Some(text) = copy.slot(field).as_deref() {
                if parse_hex(text).is_none() {
                    return Err(format!(
                        "[colors] {}: \"{text}\" isn't a colour. Use a hex code like \"#FF2A6D\".",
                        field_key(field)
                    ));
                }
            }
        }
        Ok(())
    }
}

fn field_key(field: ColorField) -> &'static str {
    match field {
        ColorField::Background => "background",
        ColorField::Accent => "accent",
        ColorField::Text => "text",
        ColorField::DayFace => "day_face",
        ColorField::NightFace => "night_face",
        ColorField::DayHands => "day_hands",
        ColorField::NightHands => "night_hands",
    }
}

/// Reads "#RRGGBB", "RRGGBB" or "#RGB".
pub fn parse_hex(text: &str) -> Option<(u8, u8, u8)> {
    let hex = text.trim().trim_start_matches('#');
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        6 => Some((byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?)),
        3 => {
            let d = |i: usize| byte(&hex[i..i + 1]).map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

pub fn format_hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockConfig {
    /// Name shown above the clock. Defaults to the city in the zone name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// An IANA time zone such as "Europe/London", or "local".
    pub zone: String,
    /// A face design for just this clock; otherwise the widget-wide one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dial: Option<DialStyle>,
}

impl ClockConfig {
    pub fn new(label: impl Into<String>, zone: impl Into<String>) -> Self {
        ClockConfig { label: Some(label.into()), zone: zone.into(), dial: None }
    }

    pub fn local() -> Self {
        ClockConfig { label: None, zone: LOCAL_ZONE.into(), dial: None }
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
    /// Face design used by every clock that doesn't pick its own.
    pub dial: DialStyle,
    /// Analog clocks with hands, or digital ones.
    pub display: Display,
    /// Layout used when `display` is digital.
    pub digital: DigitalStyle,
    pub layer: Layer,
    /// When locked, the widget can't be dragged by accident.
    pub locked: bool,
    /// Background opacity, from 0.3 to 1.0.
    pub opacity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(skip_serializing_if = "Colors::is_empty")]
    pub colors: Colors,
    #[serde(rename = "clock")]
    pub clocks: Vec<ClockConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            master: 1,
            use_24_hour: false,
            second_hand: true,
            size: Size::NORMAL,
            theme: Theme::Dark,
            dial: DialStyle::Classic,
            display: Display::Analog,
            digital: DigitalStyle::Flip,
            layer: Layer::Desktop,
            locked: false,
            opacity: 0.85,
            x: None,
            y: None,
            colors: Colors::default(),
            clocks: vec![
                ClockConfig::local(),
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
#   second_hand  show seconds (the second hand, or seconds on a digital clock)
#   size         1 to 100: 50 is normal, 100 is twice as big, 1 is a quarter
#   theme        \"dark\", \"light\", \"cyberpunk\" or \"tactical\"
#   display      \"analog\" or \"digital\"
#   dial         analog face design: \"classic\", \"numbers\", \"roman\", \"minimal\",
#                \"stopwatch\" or \"neon\"
#   digital      digital layout: \"flip\", \"segment\", \"ring\" or \"binary\"
#   layer        \"desktop\" (behind other windows), \"normal\" or \"top\" (always on top)
#   locked       true stops the widget being dragged
#   opacity      background opacity from 0.3 to 1.0
#   x, y         screen position of the widget's top-left corner
#
# [colors] (optional) overrides the theme with your own hex colours:
#   background, accent, text, day_face, night_face, day_hands, night_hands
#   e.g.  accent = \"#FF2A6D\"
#
# Up to 5 [[clock]] entries. `zone` is an IANA time zone name
# (https://en.wikipedia.org/wiki/List_of_tz_database_time_zones) or \"local\"
# for this PC's own time zone. `label` is optional. `dial` (optional) gives
# one clock its own face design.

";

impl Config {
    /// Parses settings text, then fixes anything out of range.
    pub fn from_toml(text: &str) -> Result<Config, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        cfg.colors.validate()?;
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
            self.clocks.push(ClockConfig::local());
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

    /// The face design for clock `index`.
    pub fn dial_for(&self, index: usize) -> DialStyle {
        self.clocks.get(index).and_then(|c| c.dial).unwrap_or(self.dial)
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
    fn defaults_match_version_one() {
        // Dark theme with classic faces, as before; Cyberpunk is opt-in.
        for cfg in [Config::default(), Config::from_toml("").unwrap()] {
            assert_eq!(cfg.theme, Theme::Dark);
            assert_eq!(cfg.dial, DialStyle::Classic);
            assert_eq!(cfg.opacity, 0.85);
        }
        assert_eq!(Config::from_toml("theme = \"cyberpunk\"").unwrap().theme, Theme::Cyberpunk);
        assert_eq!(Config::from_toml("theme = \"tactical\"").unwrap().theme, Theme::Tactical);
    }

    #[test]
    fn colours_and_per_clock_dials_round_trip() {
        let mut cfg = Config::default();
        cfg.colors.set(ColorField::Accent, (255, 42, 109));
        cfg.colors.set(ColorField::NightFace, (1, 2, 3));
        cfg.clocks[1].dial = Some(DialStyle::Stopwatch);
        cfg.dial = DialStyle::Roman;
        let text = cfg.to_toml();
        assert!(text.contains("[colors]"), "{text}");
        assert!(text.contains("accent = \"#FF2A6D\""), "{text}");
        let back = Config::from_toml(&text).unwrap();
        assert_eq!(back, cfg);
        assert_eq!(back.dial_for(0), DialStyle::Roman);
        assert_eq!(back.dial_for(1), DialStyle::Stopwatch);
        assert_eq!(back.colors.get(ColorField::NightFace), Some((1, 2, 3)));
    }

    #[test]
    fn hex_colour_parsing() {
        assert_eq!(parse_hex("#FF2A6D"), Some((255, 42, 109)));
        assert_eq!(parse_hex("ff2a6d"), Some((255, 42, 109)));
        assert_eq!(parse_hex(" #0f0 "), Some((0, 255, 0)));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#GG0000"), None);
        assert_eq!(parse_hex("+1+2+3"), None);
        assert_eq!(format_hex((5, 217, 232)), "#05D9E8");
    }

    #[test]
    fn a_bad_colour_is_reported() {
        let err = Config::from_toml("[colors]\naccent = \"pinkish\"\n").unwrap_err();
        assert!(err.contains("accent") && err.contains("pinkish"), "{err}");
    }

    #[test]
    fn every_dial_style_parses_by_name() {
        for (name, style) in [
            ("classic", DialStyle::Classic),
            ("numbers", DialStyle::Numbers),
            ("roman", DialStyle::Roman),
            ("minimal", DialStyle::Minimal),
            ("stopwatch", DialStyle::Stopwatch),
            ("neon", DialStyle::Neon),
        ] {
            let cfg = Config::from_toml(&format!("dial = \"{name}\"")).unwrap();
            assert_eq!(cfg.dial, style);
        }
    }

    #[test]
    fn older_settings_files_still_load() {
        // Written by version 1.0, before themes, dials and colours were added.
        let v1 = "master = 2\ntheme = \"light\"\n[[clock]]\nzone = \"local\"\n[[clock]]\nlabel = \"Paris\"\nzone = \"Europe/Paris\"\n";
        let cfg = Config::from_toml(v1).unwrap();
        assert_eq!(cfg.theme, Theme::Light);
        assert_eq!(cfg.dial, DialStyle::default());
        assert!(cfg.colors.is_empty());
        assert_eq!(cfg.master, 2);
    }

    #[test]
    fn size_runs_from_a_quarter_to_double() {
        let f = |n| Size::new(n).factor();
        assert!((f(1) - 0.25).abs() < 1e-6);
        assert!((f(50) - 1.0).abs() < 1e-6);
        assert!((f(75) - 1.5).abs() < 1e-6);
        assert!((f(100) - 2.0).abs() < 1e-6);
        // Bigger numbers always mean a bigger widget.
        for n in 1..100 {
            assert!(f(n + 1) > f(n), "{n}");
        }
        assert_eq!(Size::new(0).value(), 1);
        assert_eq!(Size::new(250).value(), 100);
        assert_eq!(Config::default().size, Size::NORMAL);
    }

    #[test]
    fn size_is_saved_as_a_number_and_read_from_numbers_or_old_names() {
        let cfg = Config { size: Size::new(63), ..Config::default() };
        assert!(cfg.to_toml().contains("\nsize = 63\n"));
        let read = |text: &str| Config::from_toml(text).map(|c| c.size.value());
        assert_eq!(read("size = 75"), Ok(75));
        assert_eq!(read("size = 62.6"), Ok(63));
        assert_eq!(read("size = 0"), Ok(1));
        assert_eq!(read("size = 999"), Ok(100));
        // Version 1.x names keep roughly the same size.
        assert_eq!(read("size = \"small\""), Ok(37));
        assert_eq!(read("size = \"medium\""), Ok(50));
        assert_eq!(read("size = \"large\""), Ok(63));
        assert!((Size::new(37).factor() - 0.8).abs() < 0.01);
        assert!((Size::new(63).factor() - 1.25).abs() < 0.02);
        let err = Config::from_toml("size = \"huge\"").unwrap_err();
        assert!(err.contains("1 to 100"), "{err}");
    }

    #[test]
    fn analog_is_the_default_and_digital_styles_parse() {
        let cfg = Config::default();
        assert_eq!((cfg.display, cfg.digital), (Display::Analog, DigitalStyle::Flip));
        for (name, style) in [
            ("flip", DigitalStyle::Flip),
            ("segment", DigitalStyle::Segment),
            ("ring", DigitalStyle::Ring),
            ("binary", DigitalStyle::Binary),
        ] {
            let cfg = Config::from_toml(&format!("display = \"digital\"\ndigital = \"{name}\"")).unwrap();
            assert_eq!((cfg.display, cfg.digital), (Display::Digital, style));
            assert_eq!(Config::from_toml(&cfg.to_toml()).unwrap(), cfg);
        }
        assert!(Config::from_toml("display = \"holographic\"").is_err());
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
        assert_eq!(cfg.clocks, vec![ClockConfig::local()]);
    }

    #[test]
    fn bad_toml_is_an_error() {
        assert!(Config::from_toml("size = \"enormous\"").is_err());
        assert!(Config::from_toml("dial = \"sundial\"").is_err());
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
