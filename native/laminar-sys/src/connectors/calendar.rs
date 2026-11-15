//! Upcoming events from a private iCalendar (.ics) link (Google Calendar's "secret
//! address in iCal format", Outlook's published ICS link, iCloud public calendars…).
//!
//! What's understood: line folding, text escapes, DTSTART as UTC (`…Z`), floating local
//! time, `TZID=` local time, and all-day `VALUE=DATE`; STATUS:CANCELLED; RRULE with
//! FREQ=DAILY/WEEKLY/MONTHLY/YEARLY, INTERVAL, COUNT, UNTIL and (weekly only) BYDAY;
//! EXDATE; RECURRENCE-ID overrides. Not understood: other BY… parts (e.g. "2nd Tuesday
//! monthly", BYMONTHDAY), RDATE — such series show only the instances above.
//!
//! Time zones: TZIDs that are Windows names ("Eastern Standard Time", used by Outlook)
//! resolve directly via the Windows time-zone database; IANA names ("America/Toronto",
//! used by Google) are mapped to Windows names with the ICU library built into Windows
//! 10 1703+ (icu.dll / icuin.dll, loaded on demand). Unknown names (custom VTIMEZONE
//! labels) are treated as this PC's local time — the VTIMEZONE rules aren't parsed.

use super::common::*;
use crate::feed::*;
use crate::http;
use std::collections::{HashMap, HashSet};

pub(super) static INFOS: &[ConnectorInfo] = &[ConnectorInfo {
    id: "calendar",
    name: "Calendar (iCal link)",
    about: "Your next events from a private iCal (.ics) link, read-only; paste the secret iCal address from Google Calendar or the published ICS link from Outlook.",
    help_url: "https://support.google.com/calendar/answer/37648",
    fields: &[Field { key: "ics_url", label: "Private iCal link", placeholder: "https://calendar.google.com/calendar/ical/…/basic.ics", secret: true, required: true }],
    refresh_secs: 300,
}];

/// Converts between "naive" wall-clock seconds (days_from_civil*86400 + time of day, no
/// zone) and Unix UTC seconds.
pub(crate) trait Clock {
    /// Wall time in zone `tzid` (None = this PC's zone) → UTC.
    fn to_utc(&self, naive: i64, tzid: Option<&str>) -> i64;
    /// UTC → wall time in this PC's zone.
    fn to_local(&self, utc: i64) -> i64;
}

/// When an event starts, in its own frame.
#[derive(Debug, Clone, PartialEq)]
enum When {
    /// All-day: naive midnight of that date.
    Date(i64),
    Utc(i64),
    /// Wall time, in TZID or floating (None = local).
    Local(i64, Option<String>),
}

impl When {
    fn naive(&self) -> i64 {
        match self {
            When::Date(n) | When::Utc(n) | When::Local(n, _) => *n,
        }
    }
    /// Same kind/zone with a different naive time.
    fn with(&self, n: i64) -> When {
        match self {
            When::Date(_) => When::Date(n),
            When::Utc(_) => When::Utc(n),
            When::Local(_, z) => When::Local(n, z.clone()),
        }
    }
    fn utc(&self, c: &dyn Clock) -> i64 {
        match self {
            When::Date(n) => c.to_utc(*n, None),
            When::Utc(n) => *n,
            When::Local(n, z) => c.to_utc(*n, z.as_deref()),
        }
    }
}

#[derive(Debug, Default)]
struct Event {
    uid: String,
    summary: String,
    start: Option<When>,
    end: Option<When>,
    rrule: Option<String>,
    exdates: Vec<When>,
    recurrence_id: Option<When>,
    cancelled: bool,
}

/// Joins folded lines (CRLF followed by a space or tab continues the previous line).
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&line[1..]);
        } else {
            out.push(line.to_string());
        }
    }
    out
}

/// "NAME;P=V;Q=\"x:y\":VALUE" → (NAME, params, VALUE).
fn split_prop(line: &str) -> Option<(String, Vec<(String, String)>, &str)> {
    let mut in_q = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_q = !in_q,
            ':' if !in_q => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let mut parts = line[..colon].split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts.filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string())).collect();
    Some((name, params, &line[colon + 1..]))
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => out.push(' '),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// "20261114", "20261114T093000", "20261114T093000Z" with optional TZID / VALUE=DATE.
fn parse_when(value: &str, params: &[(String, String)]) -> Option<When> {
    let v = value.trim();
    let date = |s: &str| -> Option<i64> { Some(days_from_civil(s.get(0..4)?.parse().ok()?, s.get(4..6)?.parse().ok()?, s.get(6..8)?.parse().ok()?) * DAY) };
    let is_date = params.iter().any(|(k, x)| k == "VALUE" && x.eq_ignore_ascii_case("DATE")) || v.len() == 8;
    if is_date {
        return Some(When::Date(date(v)?));
    }
    if v.as_bytes().get(8) != Some(&b'T') {
        return None;
    }
    let t: i64 = v.get(9..11)?.parse::<i64>().ok()? * 3600 + v.get(11..13)?.parse::<i64>().ok()? * 60 + v.get(13..15).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let n = date(v)? + t;
    if v.ends_with('Z') || v.ends_with('z') {
        return Some(When::Utc(n));
    }
    let tz = params.iter().find(|(k, _)| k == "TZID").map(|(_, z)| z.clone());
    Some(When::Local(n, tz))
}

fn parse_ics(text: &str) -> Vec<Event> {
    let mut events = Vec::new();
    let mut cur: Option<Event> = None;
    let mut depth = 0; // nested components inside VEVENT (VALARM)
    for line in unfold(text) {
        let Some((name, params, value)) = split_prop(&line) else { continue };
        match name.as_str() {
            "BEGIN" if value.eq_ignore_ascii_case("VEVENT") => cur = Some(Event::default()),
            "BEGIN" if cur.is_some() => depth += 1,
            "END" if value.eq_ignore_ascii_case("VEVENT") => {
                if let Some(e) = cur.take() {
                    if e.start.is_some() {
                        events.push(e);
                    }
                }
                depth = 0;
            }
            "END" if cur.is_some() => depth -= 1,
            _ => {
                let Some(e) = cur.as_mut() else { continue };
                if depth > 0 {
                    continue;
                }
                match name.as_str() {
                    "UID" => e.uid = value.trim().to_string(),
                    "SUMMARY" => e.summary = unescape(value),
                    "DTSTART" => e.start = parse_when(value, &params),
                    "DTEND" => e.end = parse_when(value, &params),
                    "RRULE" => e.rrule = Some(value.trim().to_string()),
                    "EXDATE" => e.exdates.extend(value.split(',').filter_map(|v| parse_when(v, &params))),
                    "RECURRENCE-ID" => e.recurrence_id = parse_when(value, &params),
                    "STATUS" => e.cancelled = value.trim().eq_ignore_ascii_case("CANCELLED"),
                    _ => {}
                }
            }
        }
    }
    events
}

/// Monday = 0 … Sunday = 6 for a day number.
fn weekday_mon0(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// Occurrence start times (naive, in the event's frame) whose UTC time is in [lo, hi].
fn expand(start: &When, rule: Option<&str>, c: &dyn Clock, lo: i64, hi: i64) -> Vec<i64> {
    let s0 = start.naive();
    let mut out = Vec::new();
    let Some(rule) = rule else {
        let u = start.utc(c);
        if u >= lo && u <= hi {
            out.push(s0);
        }
        return out;
    };
    let kv: HashMap<String, String> = rule.split(';').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_ascii_uppercase(), v.to_string())).collect();
    let freq = kv.get("FREQ").map(|s| s.to_ascii_uppercase()).unwrap_or_default();
    let interval = kv.get("INTERVAL").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1).max(1);
    let count = kv.get("COUNT").and_then(|s| s.parse::<u32>().ok());
    let until = kv.get("UNTIL").and_then(|u| parse_when(u, &[])).map(|w| match w {
        When::Date(n) => start.with(n + DAY - 1).utc(c),
        other => other.utc(c),
    });
    let byday: Vec<i64> = kv
        .get("BYDAY")
        .map(|s| {
            s.split(',')
                .filter_map(|d| ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|x| d.trim().eq_ignore_ascii_case(x)).map(|i| i as i64))
                .collect()
        })
        .unwrap_or_default();
    let unsupported = kv.keys().any(|k| k.starts_with("BY") && k != "BYDAY") || (kv.contains_key("BYDAY") && (freq != "WEEKLY" || byday.is_empty()));
    if unsupported || !matches!(freq.as_str(), "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY") {
        return expand(start, None, c, lo, hi);
    }
    let mut n = 0u32;
    // Returns false to stop the series.
    let mut emit = |ns: i64, out: &mut Vec<i64>| -> bool {
        if ns < s0 {
            return true;
        }
        n += 1;
        if count.is_some_and(|k| n > k) {
            return false;
        }
        let u = start.with(ns).utc(c);
        if until.is_some_and(|x| u > x) || u > hi {
            return false;
        }
        if u >= lo {
            out.push(ns);
        }
        true
    };
    let tod = s0.rem_euclid(DAY);
    let day0 = s0.div_euclid(DAY);
    let (y0, m0, d0) = civil_from_days(day0);
    for k in 0..20_000i64 {
        let go = match freq.as_str() {
            "DAILY" => emit(s0 + k * interval * DAY, &mut out),
            "WEEKLY" if byday.is_empty() => emit(s0 + k * interval * 7 * DAY, &mut out),
            "WEEKLY" => {
                let mut days = byday.clone();
                days.sort();
                let week = day0 - weekday_mon0(day0) + k * interval * 7;
                days.iter().all(|wd| emit((week + wd) * DAY + tod, &mut out))
            }
            _ => {
                // MONTHLY / YEARLY on the same day of the month; months without it are skipped.
                let step = if freq == "MONTHLY" { interval } else { 12 * interval };
                let mi = (m0 as i64 - 1) + k * step;
                let (y, m) = (y0 + mi.div_euclid(12), (mi.rem_euclid(12) + 1) as u32);
                let day = days_from_civil(y, m, d0);
                if civil_from_days(day) != (y, m, d0) {
                    true
                } else {
                    emit(day * DAY + tod, &mut out)
                }
            }
        };
        if !go {
            break;
        }
    }
    out
}

/// "3:00 PM".
fn clock_text(naive: i64) -> String {
    let s = naive.rem_euclid(DAY);
    let (h, m) = (s / 3600, s / 60 % 60);
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {}", if h < 12 { "AM" } else { "PM" })
}

/// "Today", "Tomorrow", "Tue" (within a week), "Nov 21".
fn day_text(day: i64, today: i64) -> String {
    const WD: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MO: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    match day - today {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        2..=6 => WD[weekday_mon0(day) as usize].into(),
        _ => {
            let (_, m, d) = civil_from_days(day);
            format!("{} {d}", MO[m as usize - 1])
        }
    }
}

/// One concrete upcoming item.
struct Item {
    title: String,
    /// Sort key and start, UTC (all-day: local midnight of the first day still relevant).
    utc: i64,
    all_day: bool,
    /// Local day it is shown on.
    day: i64,
    local: i64,
}

fn build(events: &[Event], c: &dyn Clock, now: i64) -> Feed {
    let local_now = c.to_local(now);
    let today = local_now.div_euclid(DAY);
    let today_utc = c.to_utc(today * DAY, None);
    let hi = now + 60 * DAY;
    let overridden: HashSet<(String, i64)> = events.iter().filter_map(|e| Some((e.uid.clone(), e.recurrence_id.as_ref()?.utc(c)))).collect();
    let mut items: Vec<Item> = Vec::new();
    let mut today_count = 0;
    for e in events {
        let Some(start) = &e.start else { continue };
        if e.cancelled {
            continue;
        }
        let all_day = matches!(start, When::Date(_));
        // All-day events may span several days (DTEND is exclusive).
        let span_days = match (start, &e.end) {
            (When::Date(s), Some(When::Date(en))) if en > s => (en - s) / DAY,
            _ => 1,
        };
        let lo = if all_day { today_utc - span_days * DAY } else { today_utc };
        let rule = if e.recurrence_id.is_some() { None } else { e.rrule.as_deref() };
        let ex: HashSet<i64> = e.exdates.iter().map(|x| x.utc(c)).collect();
        for ns in expand(start, rule, c, lo, hi) {
            let w = start.with(ns);
            let u = w.utc(c);
            if ex.contains(&u) || (rule.is_some() && overridden.contains(&(e.uid.clone(), u))) {
                continue;
            }
            if all_day {
                let first = ns.div_euclid(DAY);
                let last = first + span_days - 1;
                if last < today {
                    continue;
                }
                if first <= today {
                    today_count += 1;
                }
                let day = first.max(today);
                items.push(Item { title: e.summary.clone(), utc: c.to_utc(day * DAY, None) - 1, all_day, day, local: day * DAY });
            } else {
                let local = c.to_local(u);
                let day = local.div_euclid(DAY);
                if day == today {
                    today_count += 1;
                }
                if u >= now {
                    items.push(Item { title: e.summary.clone(), utc: u, all_day, day, local });
                }
            }
        }
    }
    items.sort_by_key(|i| i.utc);
    let when = |i: &Item| {
        if i.all_day {
            if i.day == today {
                "All day".to_string()
            } else {
                format!("{} all day", day_text(i.day, today))
            }
        } else {
            format!("{} {}", day_text(i.day, today), clock_text(i.local))
        }
    };
    let title = |i: &Item| if i.title.is_empty() { "(No title)".to_string() } else { i.title.clone() };
    let mut f = Feed {
        value: Some(today_count as f64),
        unit: "events".into(),
        caption: "Today".into(),
        rows: items.iter().take(5).map(|i| FeedRow { label: title(i), value: when(i), frac: None, delta: None }).collect(),
        ..Default::default()
    };
    if let Some(n) = items.iter().find(|i| !i.all_day).or(items.first()) {
        f.extra.insert("next_title".into(), title(n));
        f.extra.insert("next_time".into(), when(n));
        f.extra.insert("next_minutes".into(), ((n.utc - now).max(0) / 60).to_string());
    }
    f
}

pub fn calendar(p: &Params) -> Result<Feed, FeedError> {
    let raw = need(p, "ics_url", "private iCal link")?;
    let url = if let Some(r) = raw.strip_prefix("webcal://").or(raw.strip_prefix("webcals://")).or(raw.strip_prefix("http://")) { format!("https://{r}") } else { raw.to_string() };
    // Never echo the link: it is the secret.
    let r = http::get(&url, &[("Accept", "text/calendar")]).map_err(|e| net("Calendar", e))?;
    check(&r, "Calendar")?;
    let text = r.text();
    if !text.contains("BEGIN:VCALENDAR") {
        return Err(FeedError::Failed("Calendar: that link didn't return an iCal file".into()));
    }
    Ok(build(&parse_ics(&text), &win::WinClock::new(), now()))
}

/// The Windows time-zone database behind [`Clock`].
mod win {
    use super::super::common::{civil_from_days, days_from_civil, DAY};
    use super::Clock;
    use crate::util::{from_wide, wide};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::Time::*;

    #[derive(Clone, Copy)]
    enum Zone {
        Local,
        Utc,
        Win(DYNAMIC_TIME_ZONE_INFORMATION),
    }

    pub(crate) struct WinClock {
        local: DYNAMIC_TIME_ZONE_INFORMATION,
        cache: RefCell<HashMap<String, Zone>>,
    }

    fn st(naive: i64) -> SYSTEMTIME {
        let (y, m, d) = civil_from_days(naive.div_euclid(DAY));
        let s = naive.rem_euclid(DAY);
        SYSTEMTIME { wYear: y as u16, wMonth: m as u16, wDayOfWeek: 0, wDay: d as u16, wHour: (s / 3600) as u16, wMinute: (s / 60 % 60) as u16, wSecond: (s % 60) as u16, wMilliseconds: 0 }
    }

    fn naive(t: &SYSTEMTIME) -> i64 {
        days_from_civil(t.wYear as i64, t.wMonth as u32, t.wDay as u32) * DAY + t.wHour as i64 * 3600 + t.wMinute as i64 * 60 + t.wSecond as i64
    }

    /// The Windows zone whose registry key name is `name` ("Eastern Standard Time").
    fn find(name: &str) -> Option<DYNAMIC_TIME_ZONE_INFORMATION> {
        for i in 0..1000 {
            let mut z = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if unsafe { EnumDynamicTimeZoneInformation(i, &mut z) } != 0 {
                break;
            }
            if from_wide(&z.TimeZoneKeyName).eq_ignore_ascii_case(name) {
                return Some(z);
            }
        }
        None
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
        fn GetProcAddress(module: *mut core::ffi::c_void, name: *const u8) -> *mut core::ffi::c_void;
    }

    type UcalGetWindowsTimeZoneId = unsafe extern "C" fn(*const u16, i32, *mut u16, i32, *mut i32) -> i32;

    /// IANA → Windows zone name through Windows' built-in ICU (loaded from System32 by
    /// full path, only when an IANA name is seen).
    fn iana_to_windows(iana: &str) -> Option<String> {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        for dll in ["icu.dll", "icuin.dll"] {
            let path = wide(&format!("{sys}\\System32\\{dll}"));
            unsafe {
                let h = LoadLibraryW(path.as_ptr());
                if h.is_null() {
                    continue;
                }
                let f = GetProcAddress(h, c"ucal_getWindowsTimeZoneID".as_ptr() as *const u8);
                if f.is_null() {
                    continue;
                }
                let f: UcalGetWindowsTimeZoneId = std::mem::transmute(f);
                let id: Vec<u16> = iana.encode_utf16().collect();
                let mut out = [0u16; 128];
                let mut status = 0i32;
                let n = f(id.as_ptr(), id.len() as i32, out.as_mut_ptr(), out.len() as i32, &mut status);
                if status <= 0 && n > 0 && (n as usize) <= out.len() {
                    return Some(String::from_utf16_lossy(&out[..n as usize]));
                }
                return None;
            }
        }
        None
    }

    fn resolve(tzid: &str) -> Zone {
        let t = tzid.trim().trim_matches('"');
        if ["UTC", "Etc/UTC", "GMT", "Etc/GMT", "Z", "Zulu", "Etc/Zulu", "Universal"].iter().any(|u| t.eq_ignore_ascii_case(u)) {
            return Zone::Utc;
        }
        // "/mozilla.org/20050126_1/America/New_York" → also try "America/New_York".
        let segs: Vec<&str> = t.split('/').filter(|s| !s.is_empty()).collect();
        let tail = if segs.len() >= 2 { segs[segs.len() - 2..].join("/") } else { t.to_string() };
        for cand in [t.to_string(), tail] {
            if let Some(z) = find(&cand) {
                return Zone::Win(z);
            }
            if cand.contains('/') {
                if let Some(z) = iana_to_windows(&cand).and_then(|w| find(&w)) {
                    return Zone::Win(z);
                }
            }
        }
        Zone::Local
    }

    impl WinClock {
        pub(crate) fn new() -> Self {
            let mut local = DYNAMIC_TIME_ZONE_INFORMATION::default();
            unsafe { GetDynamicTimeZoneInformation(&mut local) };
            WinClock { local, cache: RefCell::new(HashMap::new()) }
        }
        fn zone(&self, tzid: Option<&str>) -> Zone {
            let Some(t) = tzid else { return Zone::Local };
            if let Some(z) = self.cache.borrow().get(t) {
                return *z;
            }
            let z = resolve(t);
            self.cache.borrow_mut().insert(t.to_string(), z);
            z
        }
    }

    impl Clock for WinClock {
        fn to_utc(&self, n: i64, tzid: Option<&str>) -> i64 {
            let z = match self.zone(tzid) {
                Zone::Utc => return n,
                Zone::Local => self.local,
                Zone::Win(z) => z,
            };
            let mut out = SYSTEMTIME::default();
            match unsafe { TzSpecificLocalTimeToSystemTimeEx(Some(&z), &st(n), &mut out) } {
                Ok(()) => naive(&out),
                Err(_) => n,
            }
        }
        fn to_local(&self, utc: i64) -> i64 {
            let mut out = SYSTEMTIME::default();
            match unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(&self.local), &st(utc), &mut out) } {
                Ok(()) => naive(&out),
                Err(_) => utc,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn zones() {
            // New York is UTC-5 in January (no DST), whichever name form is used.
            let c = WinClock::new();
            let jan = days_from_civil(2026, 1, 15) * DAY + 12 * 3600;
            assert_eq!(c.to_utc(jan, Some("Eastern Standard Time")), jan + 5 * 3600);
            assert_eq!(c.to_utc(jan, Some("America/New_York")), jan + 5 * 3600);
            assert_eq!(c.to_utc(jan, Some("UTC")), jan);
            let jul = days_from_civil(2026, 7, 15) * DAY + 12 * 3600;
            assert_eq!(c.to_utc(jul, Some("/mozilla.org/20050126_1/America/New_York")), jul + 4 * 3600);
            // Local round trip.
            let u = c.to_utc(jan, None);
            assert_eq!(c.to_local(u), jan);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed-offset clock; zones named "+H" get that offset, others use the local one.
    struct Fixed(i64);
    impl Clock for Fixed {
        fn to_utc(&self, n: i64, tz: Option<&str>) -> i64 {
            match tz.and_then(|t| t.parse::<i64>().ok()) {
                Some(h) => n - h * 3600,
                None => n - self.0,
            }
        }
        fn to_local(&self, utc: i64) -> i64 {
            utc + self.0
        }
    }

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261114T200000Z\r\nDTEND:20261114T210000Z\r\nSUMMARY:Dinner\\, with\r\n  friends\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE:20261114\r\nDTEND;VALUE=DATE:20261115\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:c\r\nDTSTART;TZID=\"+2\":20261114T080000\r\nSUMMARY:Early (past)\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:d\r\nDTSTART;TZID=+0:20261103T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU,TH;COUNT=6\r\nEXDATE;TZID=+0:20261117T093000\r\nSUMMARY:Standup\r\n\
BEGIN:VALARM\r\nSUMMARY:not this\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:e\r\nDTSTART:20261116T150000Z\r\nSUMMARY:Cancelled\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:d\r\nRECURRENCE-ID;TZID=+0:20261119T093000\r\nDTSTART;TZID=+0:20261119T110000\r\nSUMMARY:Standup (moved)\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    fn now() -> i64 {
        parse_time("2026-11-14T15:30:00Z").unwrap() // a Saturday
    }

    #[test]
    fn parse_and_build() {
        let evs = parse_ics(ICS);
        assert_eq!(evs.len(), 6);
        assert_eq!(evs[0].summary, "Dinner, with friends");
        assert_eq!(evs[3].summary, "Standup");
        let f = build(&evs, &Fixed(0), now());
        // Today: Holiday (all day), Dinner, Early.
        assert_eq!(f.value, Some(3.0));
        let rows: Vec<(String, String)> = f.rows.iter().map(|r| (r.label.clone(), r.value.clone())).collect();
        assert_eq!(rows[0], ("Holiday".into(), "All day".into()));
        assert_eq!(rows[1], ("Dinner, with friends".into(), "Today 8:00 PM".into()));
        // Standup series: Tue 3, Thu 5, Tue 10, Thu 12 (past), Tue 17 (excluded), Thu 19 (moved).
        assert_eq!(rows[2], ("Standup (moved)".into(), "Thu 11:00 AM".into()));
        assert_eq!(rows.len(), 3);
        assert_eq!(f.extra["next_title"], "Dinner, with friends");
        assert_eq!(f.extra["next_minutes"], "270");
    }

    #[test]
    fn local_offset() {
        // UTC-5: 20:00Z is 3:00 PM local; "today" is still the 14th.
        let f = build(&parse_ics(ICS), &Fixed(-5 * 3600), now());
        assert!(f.rows.iter().any(|r| r.value == "Today 3:00 PM"));
    }

    #[test]
    fn rules() {
        let c = Fixed(0);
        let start = When::Utc(parse_time("2026-11-01T09:00:00Z").unwrap());
        let lo = parse_time("2026-11-10T00:00:00Z").unwrap();
        let hi = lo + 10 * DAY;
        assert_eq!(expand(&start, Some("FREQ=DAILY;INTERVAL=3"), &c, lo, hi).len(), 4); // 10, 13, 16, 19
        let d = expand(&start, Some("FREQ=DAILY;UNTIL=20261112T235959Z"), &c, lo, hi);
        assert_eq!(d.len(), 3); // 10, 11, 12
        let m = expand(&When::Date(days_from_civil(2026, 1, 31) * DAY), Some("FREQ=MONTHLY"), &c, 0, days_from_civil(2026, 12, 31) * DAY);
        assert_eq!(m.len(), 7); // months with a 31st: Jan Mar May Jul Aug Oct Dec
        assert_eq!(expand(&start, Some("FREQ=MONTHLY;BYDAY=2TU"), &c, 0, hi).len(), 1); // unsupported: base only
        assert_eq!(clock_text(0), "12:00 AM");
        assert_eq!(clock_text(13 * 3600 + 5 * 60), "1:05 PM");
        assert_eq!(day_text(10, 0), "Jan 11");
    }

    #[test]
    fn folding_and_params() {
        let l = unfold("SUMMARY:a\r\n b\r\n\tc\r\nX:y");
        assert_eq!(l, vec!["SUMMARY:abc", "X:y"]);
        let (n, p, v) = split_prop("DTSTART;TZID=\"America/New_York\":20261114T090000").unwrap();
        assert_eq!((n.as_str(), v), ("DTSTART", "20261114T090000"));
        assert_eq!(p[0].1, "America/New_York");
        assert_eq!(parse_when("20261114T090000", &p), Some(When::Local(days_from_civil(2026, 11, 14) * DAY + 9 * 3600, Some("America/New_York".into()))));
    }
}
