//! Time-zone maths: what each clock shows and how far it is from the master.

use crate::config::{ClockConfig, LOCAL_ZONE};
use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use jiff::Timestamp;

/// A clock whose time zone has been looked up.
#[derive(Debug, Clone)]
pub struct ResolvedClock {
    pub label: String,
    pub tz: TimeZone,
    /// Set when the zone name in the settings couldn't be found.
    pub error: Option<String>,
}

/// Looks up a time zone name from the settings file.
pub fn resolve_zone(zone: &str) -> Result<TimeZone, String> {
    if zone.eq_ignore_ascii_case(LOCAL_ZONE) {
        return Ok(TimeZone::system());
    }
    TimeZone::get(zone).map_err(|_| format!("Unknown time zone \"{zone}\""))
}

/// Turns "America/Argentina/Buenos_Aires" into "Buenos Aires".
pub fn city_from_zone(zone: &str) -> String {
    let last = zone.rsplit('/').next().unwrap_or(zone);
    last.replace('_', " ")
}

pub fn resolve(clock: &ClockConfig) -> ResolvedClock {
    match resolve_zone(&clock.zone) {
        Ok(tz) => {
            let label = clock.label.clone().unwrap_or_else(|| {
                if clock.zone.eq_ignore_ascii_case(LOCAL_ZONE) {
                    tz.iana_name().map(city_from_zone).unwrap_or_else(|| "Local".into())
                } else {
                    city_from_zone(&clock.zone)
                }
            });
            ResolvedClock { label, tz, error: None }
        }
        Err(e) => ResolvedClock {
            label: clock.label.clone().unwrap_or_else(|| city_from_zone(&clock.zone)),
            tz: TimeZone::UTC,
            error: Some(e),
        },
    }
}

/// Everything needed to draw one clock at one instant.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub label: String,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub weekday: Weekday,
    pub day: u8,
    pub month: u8,
    pub year: i16,
    /// UTC offset of this clock, in seconds.
    pub offset_seconds: i32,
    /// This clock's offset minus the master's offset, in seconds.
    pub diff_seconds: i32,
    /// This clock's calendar date minus the master's (-1 = yesterday, 1 = tomorrow).
    pub day_delta: i32,
    pub is_master: bool,
    pub error: Option<String>,
}

impl Reading {
    /// Daytime is 6am to 6pm; the dial face is light then and dark at night.
    pub fn is_daytime(&self) -> bool {
        (6..18).contains(&self.hour)
    }
}

pub fn read_all(now: Timestamp, clocks: &[ResolvedClock], master: usize) -> Vec<Reading> {
    let master = master.min(clocks.len().saturating_sub(1));
    let Some(master_clock) = clocks.get(master) else { return Vec::new() };
    let master_zdt = now.to_zoned(master_clock.tz.clone());
    let master_offset = master_zdt.offset().seconds();
    let master_date = master_zdt.date();

    clocks
        .iter()
        .enumerate()
        .map(|(i, clock)| {
            let zdt = now.to_zoned(clock.tz.clone());
            let offset = zdt.offset().seconds();
            let day_delta = zdt
                .date()
                .since(master_date)
                .map(|span| span.get_days())
                .unwrap_or(0);
            Reading {
                label: clock.label.clone(),
                hour: zdt.hour() as u8,
                minute: zdt.minute() as u8,
                second: zdt.second() as u8,
                weekday: zdt.weekday(),
                day: zdt.day() as u8,
                month: zdt.month() as u8,
                year: zdt.year(),
                offset_seconds: offset,
                diff_seconds: offset - master_offset,
                day_delta,
                is_master: i == master,
                error: clock.error.clone(),
            }
        })
        .collect()
}

/// "+11h", "−4h 30m", "+5h 45m" or "Same time". Uses a real minus sign.
pub fn format_diff(diff_seconds: i32) -> String {
    if diff_seconds == 0 {
        return "Same time".into();
    }
    let sign = if diff_seconds > 0 { "+" } else { "\u{2212}" };
    let total_minutes = diff_seconds.unsigned_abs() / 60;
    let (h, m) = (total_minutes / 60, total_minutes % 60);
    match (h, m) {
        (0, m) => format!("{sign}{m}m"),
        (h, 0) => format!("{sign}{h}h"),
        (h, m) => format!("{sign}{h}h {m}m"),
    }
}

/// The line under the date: "Master", "+11h", "−13h · Yesterday".
pub fn diff_line(reading: &Reading) -> String {
    if reading.is_master {
        return "Master".into();
    }
    let diff = format_diff(reading.diff_seconds);
    match reading.day_delta {
        0 => diff,
        1 => format!("{diff} · Tomorrow"),
        -1 => format!("{diff} · Yesterday"),
        d if d > 0 => format!("{diff} · +{d} days"),
        d => format!("{diff} · {d} days"),
    }
}

/// Returns the main time text and, in 12-hour mode, "AM"/"PM".
pub fn format_time(hour: u8, minute: u8, use_24_hour: bool) -> (String, Option<&'static str>) {
    if use_24_hour {
        (format!("{hour:02}:{minute:02}"), None)
    } else {
        let h12 = match hour % 12 {
            0 => 12,
            h => h,
        };
        (format!("{h12}:{minute:02}"), Some(if hour < 12 { "AM" } else { "PM" }))
    }
}

/// "Wed 28 Oct".
pub fn format_date(reading: &Reading) -> String {
    const MONTHS: [&str; 12] =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let wd = match reading.weekday {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    };
    let month = MONTHS[(reading.month.clamp(1, 12) - 1) as usize];
    format!("{wd} {} {month}", reading.day)
}

/// Angles in degrees clockwise from 12 o'clock: (hour, minute, second).
pub fn hand_angles(hour: u8, minute: u8, second: u8) -> (f32, f32, f32) {
    let s = second as f32;
    let m = minute as f32 + s / 60.0;
    let h = (hour % 12) as f32 + m / 60.0;
    (h * 30.0, m * 6.0, s * 6.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(label: &str, zone: &str) -> ResolvedClock {
        resolve(&ClockConfig::new(label, zone))
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn auckland_london_in_new_zealand_summer() {
        // 28 Oct 2026: NZ is on daylight time (+13), London has just left BST (+0).
        let clocks = [clock("Auckland", "Pacific/Auckland"), clock("London", "Europe/London")];
        let r = read_all(at("2026-10-28T01:47:30Z"), &clocks, 0);
        assert_eq!((r[0].hour, r[0].minute, r[0].second), (14, 47, 30));
        assert_eq!((r[1].hour, r[1].minute), (1, 47));
        assert_eq!(r[1].diff_seconds, -13 * 3600);
        assert_eq!(r[1].day_delta, 0);
        assert_eq!(diff_line(&r[0]), "Master");
        assert_eq!(diff_line(&r[1]), "\u{2212}13h");
    }

    #[test]
    fn daylight_saving_changes_the_difference() {
        // Mid-year: NZ standard time (+12), London summer time (+1) -> 11h apart.
        let clocks = [clock("Auckland", "Pacific/Auckland"), clock("London", "Europe/London")];
        let r = read_all(at("2026-07-01T00:00:00Z"), &clocks, 0);
        assert_eq!(r[1].diff_seconds, -11 * 3600);
    }

    #[test]
    fn yesterday_and_tomorrow_are_relative_to_the_master() {
        let clocks = [clock("Auckland", "Pacific/Auckland"), clock("New York", "America/New_York")];
        // 9am Thursday in Auckland is 4pm Wednesday in New York.
        let r = read_all(at("2026-10-28T20:00:00Z"), &clocks, 0);
        assert_eq!(r[0].weekday, Weekday::Thursday);
        assert_eq!(r[1].weekday, Weekday::Wednesday);
        assert_eq!(r[1].day_delta, -1);
        assert_eq!(diff_line(&r[1]), "\u{2212}17h · Yesterday");
        // Flip the master and New York sees Auckland as tomorrow.
        let r = read_all(at("2026-10-28T20:00:00Z"), &clocks, 1);
        assert_eq!(diff_line(&r[0]), "+17h · Tomorrow");
    }

    #[test]
    fn half_and_quarter_hour_zones() {
        let clocks = [
            clock("UTC", "UTC"),
            clock("Delhi", "Asia/Kolkata"),
            clock("Kathmandu", "Asia/Kathmandu"),
            clock("Chatham", "Pacific/Chatham"),
        ];
        let r = read_all(at("2026-01-15T12:00:00Z"), &clocks, 0);
        assert_eq!(diff_line(&r[1]), "+5h 30m");
        assert_eq!(diff_line(&r[2]), "+5h 45m");
        // Chatham Islands in January: daylight time, UTC+13:45, already the next day.
        assert_eq!(diff_line(&r[3]), "+13h 45m · Tomorrow");
    }

    #[test]
    fn diff_formatting() {
        assert_eq!(format_diff(0), "Same time");
        assert_eq!(format_diff(3600), "+1h");
        assert_eq!(format_diff(-1800), "\u{2212}30m");
        assert_eq!(format_diff(-(9 * 3600 + 30 * 60)), "\u{2212}9h 30m");
    }

    #[test]
    fn time_and_date_formatting() {
        assert_eq!(format_time(0, 5, false), ("12:05".into(), Some("AM")));
        assert_eq!(format_time(12, 0, false), ("12:00".into(), Some("PM")));
        assert_eq!(format_time(14, 47, false), ("2:47".into(), Some("PM")));
        assert_eq!(format_time(14, 47, true), ("14:47".into(), None));
        assert_eq!(format_time(7, 3, true), ("07:03".into(), None));
        let clocks = [clock("Auckland", "Pacific/Auckland")];
        let r = read_all(at("2026-10-28T01:47:30Z"), &clocks, 0);
        assert_eq!(format_date(&r[0]), "Wed 28 Oct");
    }

    #[test]
    fn hand_angles_move_smoothly() {
        assert_eq!(hand_angles(0, 0, 0), (0.0, 0.0, 0.0));
        assert_eq!(hand_angles(15, 30, 0), (105.0, 180.0, 0.0));
        let (h, m, s) = hand_angles(9, 45, 30);
        assert!((h - 292.75).abs() < 1e-3 && (m - 273.0).abs() < 1e-3 && s == 180.0);
    }

    #[test]
    fn labels_and_bad_zones() {
        let c = resolve(&ClockConfig { label: None, zone: "America/Argentina/Buenos_Aires".into() });
        assert_eq!(c.label, "Buenos Aires");
        assert!(c.error.is_none());
        let bad = resolve(&ClockConfig { label: None, zone: "Mars/Olympus_Mons".into() });
        assert_eq!(bad.label, "Olympus Mons");
        assert!(bad.error.is_some());
    }

    #[test]
    fn master_index_out_of_range_is_clamped() {
        let clocks = [clock("A", "UTC"), clock("B", "Asia/Tokyo")];
        let r = read_all(at("2026-10-28T00:00:00Z"), &clocks, 7);
        assert!(r[1].is_master);
        assert_eq!(r[0].diff_seconds, -9 * 3600);
    }
}
