//! A curated list of cities offered in the right-click menu.
//!
//! Any IANA time zone (e.g. `America/Argentina/Ushuaia`) can still be used by
//! editing the settings file; this list just keeps the menu short.

/// One menu entry: the region submenu it lives in, the label shown on the
/// clock, and the IANA time zone name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct City {
    pub region: &'static str,
    pub label: &'static str,
    pub zone: &'static str,
}

const fn c(region: &'static str, label: &'static str, zone: &'static str) -> City {
    City { region, label, zone }
}

/// Region submenus, in the order they appear.
pub const REGIONS: &[&str] = &[
    "Pacific & Oceania",
    "Asia",
    "Middle East",
    "Europe",
    "Africa",
    "North America",
    "Central & South America",
];

pub const CITIES: &[City] = &[
    // Pacific & Oceania
    c("Pacific & Oceania", "Auckland", "Pacific/Auckland"),
    c("Pacific & Oceania", "Chatham Islands", "Pacific/Chatham"),
    c("Pacific & Oceania", "Sydney", "Australia/Sydney"),
    c("Pacific & Oceania", "Melbourne", "Australia/Melbourne"),
    c("Pacific & Oceania", "Brisbane", "Australia/Brisbane"),
    c("Pacific & Oceania", "Adelaide", "Australia/Adelaide"),
    c("Pacific & Oceania", "Darwin", "Australia/Darwin"),
    c("Pacific & Oceania", "Perth", "Australia/Perth"),
    c("Pacific & Oceania", "Hobart", "Australia/Hobart"),
    c("Pacific & Oceania", "Fiji", "Pacific/Fiji"),
    c("Pacific & Oceania", "Tonga", "Pacific/Tongatapu"),
    c("Pacific & Oceania", "Samoa", "Pacific/Apia"),
    c("Pacific & Oceania", "Rarotonga", "Pacific/Rarotonga"),
    c("Pacific & Oceania", "Port Moresby", "Pacific/Port_Moresby"),
    c("Pacific & Oceania", "Guam", "Pacific/Guam"),
    c("Pacific & Oceania", "Honolulu", "Pacific/Honolulu"),
    // Asia
    c("Asia", "Tokyo", "Asia/Tokyo"),
    c("Asia", "Seoul", "Asia/Seoul"),
    c("Asia", "Beijing", "Asia/Shanghai"),
    c("Asia", "Shanghai", "Asia/Shanghai"),
    c("Asia", "Hong Kong", "Asia/Hong_Kong"),
    c("Asia", "Taipei", "Asia/Taipei"),
    c("Asia", "Manila", "Asia/Manila"),
    c("Asia", "Singapore", "Asia/Singapore"),
    c("Asia", "Kuala Lumpur", "Asia/Kuala_Lumpur"),
    c("Asia", "Jakarta", "Asia/Jakarta"),
    c("Asia", "Bangkok", "Asia/Bangkok"),
    c("Asia", "Ho Chi Minh City", "Asia/Ho_Chi_Minh"),
    c("Asia", "Dhaka", "Asia/Dhaka"),
    c("Asia", "Kathmandu", "Asia/Kathmandu"),
    c("Asia", "New Delhi", "Asia/Kolkata"),
    c("Asia", "Mumbai", "Asia/Kolkata"),
    c("Asia", "Bengaluru", "Asia/Kolkata"),
    c("Asia", "Colombo", "Asia/Colombo"),
    c("Asia", "Karachi", "Asia/Karachi"),
    c("Asia", "Tashkent", "Asia/Tashkent"),
    c("Asia", "Almaty", "Asia/Almaty"),
    // Middle East
    c("Middle East", "Dubai", "Asia/Dubai"),
    c("Middle East", "Abu Dhabi", "Asia/Dubai"),
    c("Middle East", "Doha", "Asia/Qatar"),
    c("Middle East", "Riyadh", "Asia/Riyadh"),
    c("Middle East", "Tehran", "Asia/Tehran"),
    c("Middle East", "Kabul", "Asia/Kabul"),
    c("Middle East", "Jerusalem", "Asia/Jerusalem"),
    c("Middle East", "Beirut", "Asia/Beirut"),
    c("Middle East", "Amman", "Asia/Amman"),
    // Europe
    c("Europe", "London", "Europe/London"),
    c("Europe", "Dublin", "Europe/Dublin"),
    c("Europe", "Lisbon", "Europe/Lisbon"),
    c("Europe", "Madrid", "Europe/Madrid"),
    c("Europe", "Paris", "Europe/Paris"),
    c("Europe", "Amsterdam", "Europe/Amsterdam"),
    c("Europe", "Brussels", "Europe/Brussels"),
    c("Europe", "Berlin", "Europe/Berlin"),
    c("Europe", "Zurich", "Europe/Zurich"),
    c("Europe", "Rome", "Europe/Rome"),
    c("Europe", "Vienna", "Europe/Vienna"),
    c("Europe", "Copenhagen", "Europe/Copenhagen"),
    c("Europe", "Oslo", "Europe/Oslo"),
    c("Europe", "Stockholm", "Europe/Stockholm"),
    c("Europe", "Warsaw", "Europe/Warsaw"),
    c("Europe", "Prague", "Europe/Prague"),
    c("Europe", "Athens", "Europe/Athens"),
    c("Europe", "Helsinki", "Europe/Helsinki"),
    c("Europe", "Kyiv", "Europe/Kyiv"),
    c("Europe", "Istanbul", "Europe/Istanbul"),
    c("Europe", "Moscow", "Europe/Moscow"),
    // Africa
    c("Africa", "Cairo", "Africa/Cairo"),
    c("Africa", "Casablanca", "Africa/Casablanca"),
    c("Africa", "Lagos", "Africa/Lagos"),
    c("Africa", "Accra", "Africa/Accra"),
    c("Africa", "Nairobi", "Africa/Nairobi"),
    c("Africa", "Addis Ababa", "Africa/Addis_Ababa"),
    c("Africa", "Johannesburg", "Africa/Johannesburg"),
    c("Africa", "Cape Town", "Africa/Johannesburg"),
    // North America
    c("North America", "Anchorage", "America/Anchorage"),
    c("North America", "Vancouver", "America/Vancouver"),
    c("North America", "Los Angeles", "America/Los_Angeles"),
    c("North America", "San Francisco", "America/Los_Angeles"),
    c("North America", "Seattle", "America/Los_Angeles"),
    c("North America", "Phoenix", "America/Phoenix"),
    c("North America", "Denver", "America/Denver"),
    c("North America", "Chicago", "America/Chicago"),
    c("North America", "Dallas", "America/Chicago"),
    c("North America", "Mexico City", "America/Mexico_City"),
    c("North America", "Toronto", "America/Toronto"),
    c("North America", "New York", "America/New_York"),
    c("North America", "Washington DC", "America/New_York"),
    c("North America", "Miami", "America/New_York"),
    c("North America", "Halifax", "America/Halifax"),
    c("North America", "St. John's", "America/St_Johns"),
    // Central & South America
    c("Central & South America", "Panama", "America/Panama"),
    c("Central & South America", "Bogotá", "America/Bogota"),
    c("Central & South America", "Lima", "America/Lima"),
    c("Central & South America", "Caracas", "America/Caracas"),
    c("Central & South America", "Santiago", "America/Santiago"),
    c("Central & South America", "Buenos Aires", "America/Argentina/Buenos_Aires"),
    c("Central & South America", "São Paulo", "America/Sao_Paulo"),
    c("Central & South America", "Rio de Janeiro", "America/Sao_Paulo"),
];

/// Coordinated Universal Time, offered as its own menu item.
pub const UTC: City = c("", "UTC", "UTC");

/// Cities belonging to one region, with their index into [`CITIES`].
pub fn in_region(region: &str) -> impl Iterator<Item = (usize, &'static City)> + '_ {
    CITIES
        .iter()
        .enumerate()
        .filter(move |(_, city)| city.region == region)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_city_has_a_known_region_and_a_valid_zone() {
        for city in CITIES {
            assert!(REGIONS.contains(&city.region), "{} has unknown region", city.label);
            assert!(
                jiff::tz::TimeZone::get(city.zone).is_ok(),
                "{} uses an unknown zone {}",
                city.label,
                city.zone
            );
        }
        assert!(jiff::tz::TimeZone::get(UTC.zone).is_ok());
    }

    #[test]
    fn menu_fits_in_the_command_id_range() {
        // win.rs reserves 900 ids per "set" / "add" block.
        assert!(CITIES.len() < 900);
    }
}
