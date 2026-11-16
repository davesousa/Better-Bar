//! "time in tokyo": the time in a city, using Windows' own time zones.
//!
//! Windows lists each zone under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones`
//! with a display name that names cities ("(UTC+09:00) Osaka, Sapporo, Tokyo"). Those names
//! cover most of the world; North American zones are named by region ("Eastern Time (US &
//! Canada)"), so [`ALIASES`] adds common cities there and elsewhere.

use std::sync::OnceLock;
use windows::core::HSTRING;
use windows::Win32::Foundation::SYSTEMTIME;

/// Extra city → Windows time-zone key.
const ALIASES: &[(&str, &str)] = &[
    ("new york", "Eastern Standard Time"),
    ("nyc", "Eastern Standard Time"),
    ("boston", "Eastern Standard Time"),
    ("washington", "Eastern Standard Time"),
    ("miami", "Eastern Standard Time"),
    ("atlanta", "Eastern Standard Time"),
    ("toronto", "Eastern Standard Time"),
    ("ottawa", "Eastern Standard Time"),
    ("montreal", "Eastern Standard Time"),
    ("detroit", "Eastern Standard Time"),
    ("philadelphia", "Eastern Standard Time"),
    ("chicago", "Central Standard Time"),
    ("dallas", "Central Standard Time"),
    ("houston", "Central Standard Time"),
    ("austin", "Central Standard Time"),
    ("winnipeg", "Central Standard Time"),
    ("new orleans", "Central Standard Time"),
    ("minneapolis", "Central Standard Time"),
    ("denver", "Mountain Standard Time"),
    ("calgary", "Mountain Standard Time"),
    ("edmonton", "Mountain Standard Time"),
    ("salt lake city", "Mountain Standard Time"),
    ("phoenix", "US Mountain Standard Time"),
    ("los angeles", "Pacific Standard Time"),
    ("la", "Pacific Standard Time"),
    ("san francisco", "Pacific Standard Time"),
    ("sf", "Pacific Standard Time"),
    ("seattle", "Pacific Standard Time"),
    ("vancouver", "Pacific Standard Time"),
    ("portland", "Pacific Standard Time"),
    ("las vegas", "Pacific Standard Time"),
    ("san diego", "Pacific Standard Time"),
    ("anchorage", "Alaskan Standard Time"),
    ("honolulu", "Hawaiian Standard Time"),
    ("hawaii", "Hawaiian Standard Time"),
    ("halifax", "Atlantic Standard Time"),
    ("st. john's", "Newfoundland Standard Time"),
    ("st johns", "Newfoundland Standard Time"),
    ("regina", "Canada Central Standard Time"),
    ("saskatoon", "Canada Central Standard Time"),
    ("utc", "UTC"),
    ("gmt", "GMT Standard Time"),
    ("uk", "GMT Standard Time"),
    ("manchester", "GMT Standard Time"),
    ("edinburgh", "GMT Standard Time"),
    ("dublin", "GMT Standard Time"),
    ("lisbon", "GMT Standard Time"),
    ("barcelona", "Romance Standard Time"),
    ("munich", "W. Europe Standard Time"),
    ("frankfurt", "W. Europe Standard Time"),
    ("zurich", "W. Europe Standard Time"),
    ("milan", "W. Europe Standard Time"),
    ("istanbul", "Turkey Standard Time"),
    ("dubai", "Arabian Standard Time"),
    ("mumbai", "India Standard Time"),
    ("delhi", "India Standard Time"),
    ("new delhi", "India Standard Time"),
    ("bangalore", "India Standard Time"),
    ("india", "India Standard Time"),
    ("shanghai", "China Standard Time"),
    ("china", "China Standard Time"),
    ("hong kong", "China Standard Time"),
    ("shenzhen", "China Standard Time"),
    ("japan", "Tokyo Standard Time"),
    ("seoul", "Korea Standard Time"),
    ("sydney", "AUS Eastern Standard Time"),
    ("melbourne", "AUS Eastern Standard Time"),
    ("auckland", "New Zealand Standard Time"),
    ("sao paulo", "E. South America Standard Time"),
    ("são paulo", "E. South America Standard Time"),
    ("rio de janeiro", "E. South America Standard Time"),
    ("mexico city", "Central Standard Time (Mexico)"),
];

/// The places named in a zone's display name: "(UTC+09:00) Osaka, Sapporo, Tokyo" →
/// ["Osaka", "Sapporo", "Tokyo"]; "(UTC-05:00) Eastern Time (US & Canada)" → ["Eastern Time"].
pub fn places(display: &str) -> Vec<String> {
    let s = display.trim();
    let s = match s.find(')') {
        Some(i) if s.starts_with('(') => &s[i + 1..],
        _ => s,
    };
    // Drop parenthetical notes such as "(US & Canada)" or "(old)".
    let mut clean = String::new();
    let mut depth = 0;
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            _ if depth == 0 => clean.push(ch),
            _ => {}
        }
    }
    clean.split([',', '&']).map(|p| p.trim().to_string()).filter(|p| p.len() > 1 && !p.starts_with("UTC") && !p.starts_with("Coordinated")).collect()
}

/// (Windows time-zone key, display name) for every zone on this PC (read once).
pub fn zones() -> &'static [(String, String)] {
    static Z: OnceLock<Vec<(String, String)>> = OnceLock::new();
    Z.get_or_init(read_zones)
}

fn read_zones() -> Vec<(String, String)> {
    use windows::Win32::System::Registry::*;
    let mut out = Vec::new();
    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones"), Some(0), KEY_READ, &mut root).is_err() {
            return out;
        }
        let mut i = 0u32;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(root, i, Some(windows::core::PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                break;
            }
            i += 1;
            let key = String::from_utf16_lossy(&name[..len as usize]);
            let mut buf = [0u16; 512];
            let mut size = (buf.len() * 2) as u32;
            let ok = RegGetValueW(root, &HSTRING::from(key.as_str()), &HSTRING::from("Display"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).is_ok();
            if ok {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                out.push((key, String::from_utf16_lossy(&buf[..n])));
            }
        }
        let _ = RegCloseKey(root);
    }
    out
}

/// What a time query asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeQuery {
    /// The time here.
    Local,
    /// The time in a place (as typed, lower-case).
    Place(String),
}

/// "time", "time in tokyo", "tokyo time", "what time is it in paris", "current time in sf".
pub fn parse_query(q: &str) -> Option<TimeQuery> {
    let q = q.trim().to_lowercase().replace('?', "");
    let q = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if ["time", "time now", "what time is it", "current time", "local time", "clock"].contains(&q.as_str()) {
        return Some(TimeQuery::Local);
    }
    for p in ["what time is it in ", "what's the time in ", "current time in ", "local time in ", "time in ", "time at ", "time "] {
        if let Some(rest) = q.strip_prefix(p) {
            let rest = rest.trim();
            if rest.len() >= 2 {
                return Some(TimeQuery::Place(rest.to_string()));
            }
        }
    }
    for s in [" time now", " local time", " time"] {
        if let Some(rest) = q.strip_suffix(s) {
            let rest = rest.trim();
            if rest.len() >= 2 {
                return Some(TimeQuery::Place(rest.to_string()));
            }
        }
    }
    None
}

/// The zone for a place: (label to show, Windows key). Exact city names first, then
/// aliases, then a name that starts with what was typed (3+ letters).
pub fn find(place: &str, zones: &[(String, String)], extra: &[(String, String)]) -> Option<(String, String)> {
    let p = place.trim().to_lowercase();
    if p.len() < 2 {
        return None;
    }
    // Places the user set up for the world clocks win (their own labels).
    if let Some((l, k)) = extra.iter().find(|(l, _)| l.to_lowercase() == p) {
        return Some((l.clone(), k.clone()));
    }
    for (key, disp) in zones {
        if let Some(name) = places(disp).into_iter().find(|n| n.to_lowercase() == p) {
            return Some((name, key.clone()));
        }
    }
    if let Some((city, key)) = ALIASES.iter().find(|(c, _)| *c == p) {
        if zones.is_empty() || zones.iter().any(|(k, _)| k == key) {
            return Some((title_case(city), key.to_string()));
        }
    }
    if p.len() >= 3 {
        for (key, disp) in zones {
            if let Some(name) = places(disp).into_iter().find(|n| n.to_lowercase().starts_with(&p)) {
                return Some((name, key.clone()));
            }
        }
        if let Some((city, key)) = ALIASES.iter().find(|(c, _)| c.starts_with(&p)) {
            return Some((title_case(city), key.to_string()));
        }
    }
    None
}

fn title_case(s: &str) -> String {
    match s {
        "nyc" | "la" | "sf" | "uk" | "utc" | "gmt" => return s.to_uppercase(),
        _ => {}
    }
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The current time in a Windows time zone.
pub fn now_in(key: &str) -> Option<SYSTEMTIME> {
    use windows::Win32::System::Time::*;
    unsafe {
        let mut i = 0u32;
        loop {
            let mut z = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if EnumDynamicTimeZoneInformation(i, &mut z) != 0 {
                return None;
            }
            let n = z.TimeZoneKeyName.iter().position(|&c| c == 0).unwrap_or(z.TimeZoneKeyName.len());
            if String::from_utf16_lossy(&z.TimeZoneKeyName[..n]).eq_ignore_ascii_case(key) {
                let utc = windows::Win32::System::SystemInformation::GetSystemTime();
                let mut local = SYSTEMTIME::default();
                SystemTimeToTzSpecificLocalTimeEx(Some(&z), &utc, &mut local).ok()?;
                return Some(local);
            }
            i += 1;
        }
    }
}

/// The local time now.
pub fn local_now() -> SYSTEMTIME {
    unsafe { windows::Win32::System::SystemInformation::GetLocalTime() }
}

/// Minutes from local time to `t` (positive: ahead), ignoring seconds; within ±1 day.
pub fn minutes_ahead(t: &SYSTEMTIME, local: &SYSTEMTIME) -> i32 {
    let m = |s: &SYSTEMTIME| s.wHour as i32 * 60 + s.wMinute as i32;
    let mut d = m(t) - m(local);
    // Day difference from the date (handles crossing midnight).
    let day = |s: &SYSTEMTIME| s.wYear as i32 * 400 + s.wMonth as i32 * 32 + s.wDay as i32;
    let dd = day(t) - day(local);
    if dd > 0 {
        d += 24 * 60;
    } else if dd < 0 {
        d -= 24 * 60;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_from_display_names() {
        assert_eq!(places("(UTC+09:00) Osaka, Sapporo, Tokyo"), vec!["Osaka", "Sapporo", "Tokyo"]);
        assert_eq!(places("(UTC-05:00) Eastern Time (US & Canada)"), vec!["Eastern Time"]);
        assert_eq!(places("(UTC+01:00) Brussels, Copenhagen, Madrid, Paris"), vec!["Brussels", "Copenhagen", "Madrid", "Paris"]);
        assert!(places("(UTC) Coordinated Universal Time").is_empty());
    }

    #[test]
    fn time_queries() {
        assert_eq!(parse_query("time"), Some(TimeQuery::Local));
        assert_eq!(parse_query("What time is it?"), Some(TimeQuery::Local));
        assert_eq!(parse_query("time in Tokyo"), Some(TimeQuery::Place("tokyo".into())));
        assert_eq!(parse_query("tokyo time"), Some(TimeQuery::Place("tokyo".into())));
        assert_eq!(parse_query("what time is it in new york"), Some(TimeQuery::Place("new york".into())));
        assert_eq!(parse_query("timer"), None);
        assert_eq!(parse_query("time machine"), Some(TimeQuery::Place("machine".into())), "parsed; find() then rejects it");
    }

    #[test]
    fn finding_places() {
        let zones = vec![
            ("Tokyo Standard Time".to_string(), "(UTC+09:00) Osaka, Sapporo, Tokyo".to_string()),
            ("Eastern Standard Time".to_string(), "(UTC-05:00) Eastern Time (US & Canada)".to_string()),
            ("Romance Standard Time".to_string(), "(UTC+01:00) Brussels, Copenhagen, Madrid, Paris".to_string()),
        ];
        assert_eq!(find("tokyo", &zones, &[]), Some(("Tokyo".into(), "Tokyo Standard Time".into())));
        assert_eq!(find("toronto", &zones, &[]), Some(("Toronto".into(), "Eastern Standard Time".into())));
        assert_eq!(find("par", &zones, &[]), Some(("Paris".into(), "Romance Standard Time".into())));
        assert_eq!(find("machine", &zones, &[]), None);
        let mine = vec![("Home".to_string(), "Eastern Standard Time".to_string())];
        assert_eq!(find("home", &zones, &mine), Some(("Home".into(), "Eastern Standard Time".into())));
    }

    #[test]
    fn real_zones_cover_big_cities() {
        let z = zones();
        if z.is_empty() {
            return;
        }
        for c in ["tokyo", "london", "paris", "toronto", "sydney", "berlin"] {
            assert!(find(c, z, &[]).is_some(), "{c}");
        }
        let t = now_in("Tokyo Standard Time").expect("tokyo time");
        assert!(t.wHour < 24);
    }

    #[test]
    fn offsets_cross_midnight() {
        let mk = |d, h, m| SYSTEMTIME { wYear: 2026, wMonth: 11, wDay: d, wHour: h, wMinute: m, ..Default::default() };
        assert_eq!(minutes_ahead(&mk(17, 9, 0), &mk(16, 19, 0)), 14 * 60);
        assert_eq!(minutes_ahead(&mk(16, 1, 0), &mk(16, 6, 0)), -5 * 60);
    }
}
