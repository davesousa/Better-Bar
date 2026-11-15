//! Helpers shared by the web-service connectors: requests that map HTTP trouble to
//! [`FeedError`], UTC date arithmetic (no date crate), daily bucketing and money text.
//!
//! Daily series are bucketed by UTC day. Error messages never include request URLs or
//! headers, because those can carry keys or private links.

use crate::feed::*;
use crate::http;
use serde_json::Value;

pub(crate) const DAY: i64 = 86_400;

/// Seconds since 1970-01-01 UTC.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month, day) for days since 1970-01-01.
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Start (00:00 UTC) of the day containing `ts`.
pub(crate) fn day_start(ts: i64) -> i64 {
    ts.div_euclid(DAY) * DAY
}

/// "YYYY-MM-DD" (UTC).
pub(crate) fn ymd(ts: i64) -> String {
    let (y, m, d) = civil_from_days(ts.div_euclid(DAY));
    format!("{y:04}-{m:02}-{d:02}")
}

/// "YYYY-MM-DDTHH:MM:SSZ" (UTC).
pub(crate) fn rfc3339(ts: i64) -> String {
    let s = ts.rem_euclid(DAY);
    format!("{}T{:02}:{:02}:{:02}Z", ymd(ts), s / 3600, s / 60 % 60, s % 60)
}

/// Parses "YYYY-MM-DD", "YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH[:MM]|±HHMM]" (a space may
/// replace the T) into Unix seconds. No offset means UTC.
pub(crate) fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let mo: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    let mut t = days_from_civil(y, mo, d) * DAY;
    if b.len() == 10 {
        return Some(t);
    }
    if b[10] != b'T' && b[10] != b' ' {
        return None;
    }
    let rest = &s[11..];
    let hh: i64 = rest.get(0..2)?.parse().ok()?;
    let mm: i64 = rest.get(3..5)?.parse().ok()?;
    t += hh * 3600 + mm * 60;
    let mut i = 5;
    let rb = rest.as_bytes();
    if rb.get(5) == Some(&b':') {
        t += rest.get(6..8)?.parse::<i64>().ok()?;
        i = 8;
        if rb.get(8) == Some(&b'.') {
            i = 9;
            while rb.get(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
        }
    }
    let tz = rest[i..].trim();
    if tz.is_empty() || tz == "Z" || tz == "z" {
        return Some(t);
    }
    let sign = match tz.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return Some(t),
    };
    let digits: String = tz[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    let oh: i64 = digits.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
    let om: i64 = digits.get(2..4).and_then(|x| x.parse().ok()).unwrap_or(0);
    Some(t - sign * (oh * 3600 + om * 60))
}

/// A JSON number that may also arrive as a string ("12.50").
pub(crate) fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Like [`num`] but 0 when absent.
pub(crate) fn num0(v: &Value) -> f64 {
    num(v).unwrap_or(0.0)
}

/// Turns a network failure into a FeedError without echoing the URL (it may hold a secret).
pub(crate) fn net(service: &str, e: String) -> FeedError {
    if e.starts_with("bad url") {
        FeedError::NeedsSetup(format!("{service}: the address isn't a valid https:// link"))
    } else {
        FeedError::Failed(format!("{service}: {e}"))
    }
}

/// Checks the status and parses the body as JSON.
pub(crate) fn json_of(r: &http::Response, service: &str) -> Result<Value, FeedError> {
    check(r, service)?;
    r.json().ok_or_else(|| FeedError::Failed(format!("{service}: unexpected response")))
}

/// GET → JSON (non-2xx mapped by [`check`]).
pub(crate) fn get_json(url: &str, headers: &[(&str, &str)], service: &str) -> Result<Value, FeedError> {
    let r = http::get(url, headers).map_err(|e| net(service, e))?;
    json_of(&r, service)
}

/// POST JSON → JSON.
pub(crate) fn post_json(url: &str, headers: &[(&str, &str)], body: &Value, service: &str) -> Result<Value, FeedError> {
    let r = http::post_json(url, headers, body).map_err(|e| net(service, e))?;
    json_of(&r, service)
}

/// The GraphQL `errors[0].message`, if any (APIs often return them with status 200).
pub(crate) fn graphql_error(v: &Value, service: &str) -> Result<(), FeedError> {
    match v["errors"][0]["message"].as_str() {
        Some(m) => {
            let short: String = m.chars().take(120).collect();
            Err(FeedError::Failed(format!("{service}: {short}")))
        }
        None => Ok(()),
    }
}

/// Sums dated amounts into `days` UTC-day buckets ending today (oldest first), and the
/// total of the `days` before that window.
pub(crate) fn daily(points: &[(i64, f64)], days: usize, now: i64) -> (Vec<f64>, f64) {
    let start = day_start(now) - (days as i64 - 1) * DAY;
    let prev_start = start - days as i64 * DAY;
    let mut series = vec![0.0; days];
    let mut prev = 0.0;
    for &(t, v) in points {
        if t >= start {
            let i = ((t - start) / DAY) as usize;
            if i < days {
                series[i] += v;
            }
        } else if t >= prev_start {
            prev += v;
        }
    }
    (series, prev)
}

/// Percent change from `prev` to `cur`, when `prev` is positive.
pub(crate) fn pct(cur: f64, prev: f64) -> Option<f64> {
    (prev > 0.0).then(|| (cur - prev) / prev * 100.0)
}

/// The standard revenue feed: last 30 days total, 30 daily totals, change versus the 30
/// days before (only when `have_prev`, i.e. the source was asked for 60 days).
pub(crate) fn revenue_feed(points: &[(i64, f64)], now: i64, currency: &str, have_prev: bool) -> Feed {
    let (series, prev) = daily(points, 30, now);
    let total: f64 = series.iter().sum();
    Feed {
        value: Some(round2(total)),
        currency: Some(currency.to_uppercase()),
        caption: "Last 30 days".into(),
        delta_pct: if have_prev { pct(total, prev) } else { None },
        series: series.into_iter().map(round2).collect(),
        ..Default::default()
    }
}

pub(crate) fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Minor units (cents) per major unit for an ISO currency (zero-decimal ones return 1).
pub(crate) fn minor_per_major(currency: &str) -> f64 {
    match currency.to_uppercase().as_str() {
        "BIF" | "CLP" | "DJF" | "GNF" | "JPY" | "KMF" | "KRW" | "MGA" | "PYG" | "RWF" | "UGX" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 1.0,
        "BHD" | "JOD" | "KWD" | "OMR" | "TND" => 1000.0,
        _ => 100.0,
    }
}

/// "1,234.56" (thousands separators, `decimals` places).
pub(crate) fn group(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(&f);
    }
    if v < 0.0 && v.abs() >= 0.5 * 10f64.powi(-(decimals as i32)) {
        out.insert(0, '-');
    }
    out
}

/// "$1,204.50", "€12.00", "1,000 JPY"-style money text.
pub(crate) fn money(v: f64, currency: &str) -> String {
    let c = currency.to_uppercase();
    let dec = if minor_per_major(&c) == 1.0 { 0 } else { 2 };
    let sym = match c.as_str() {
        "USD" | "CAD" | "AUD" | "NZD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        _ => "",
    };
    let n = group(v.abs(), dec);
    let sign = if v < 0.0 { "-" } else { "" };
    if sym.is_empty() {
        format!("{sign}{n} {c}")
    } else {
        format!("{sign}{sym}{n}")
    }
}

/// Compact count: 950, 12.3K, 4.5M.
pub(crate) fn compact(v: f64) -> String {
    let a = v.abs();
    if a >= 1e9 {
        format!("{:.1}B", v / 1e9)
    } else if a >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else if a >= 1e4 {
        format!("{:.1}K", v / 1e3)
    } else {
        group(v, 0)
    }
}

/// Top `n` (label, amount) by amount, as rows with each one's share of `total`.
pub(crate) fn top_rows(mut items: Vec<(String, f64)>, n: usize, total: f64, fmt: impl Fn(f64) -> String) -> Vec<FeedRow> {
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    items
        .into_iter()
        .filter(|(_, v)| *v > 0.0)
        .take(n)
        .map(|(label, v)| FeedRow { label, value: fmt(v), frac: (total > 0.0).then(|| (v / total) as f32), delta: None })
        .collect()
}

/// An optional parameter, trimmed, or None if blank.
pub(crate) fn opt<'a>(p: &'a Params, key: &str) -> Option<&'a str> {
    p.get(key).map(|s| s.trim()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(parse_time("2026-11-14"), Some(days_from_civil(2026, 11, 14) * DAY));
        let t = parse_time("2026-11-14T10:30:00Z").unwrap();
        assert_eq!(rfc3339(t), "2026-11-14T10:30:00Z");
        assert_eq!(parse_time("2026-11-14T12:30:00.123+02:00"), Some(t));
        assert_eq!(parse_time("2026-11-14 05:30:00.674981-05"), Some(t));
        assert_eq!(parse_time("2026-11-14T10:30Z"), Some(t));
        assert_eq!(parse_time("nope"), None);
    }
    #[test]
    fn buckets() {
        let now = parse_time("2026-11-14T12:00:00Z").unwrap();
        let pts = [(now, 5.0), (now - DAY, 2.0), (now - 29 * DAY, 1.0), (now - 31 * DAY, 4.0), (now - 70 * DAY, 9.0)];
        let f = revenue_feed(&pts, now, "usd", true);
        assert_eq!(f.series.len(), 30);
        assert_eq!(f.value, Some(8.0));
        assert_eq!(f.series[29], 5.0);
        assert_eq!(f.series[0], 1.0);
        assert_eq!(f.delta_pct, Some(100.0));
        assert_eq!(f.currency.as_deref(), Some("USD"));
    }
    #[test]
    fn text() {
        assert_eq!(money(1204.5, "usd"), "$1,204.50");
        assert_eq!(money(1000.0, "JPY"), "¥1,000");
        assert_eq!(money(-3.0, "CHF"), "-3.00 CHF");
        assert_eq!(group(1234567.0, 0), "1,234,567");
        assert_eq!(compact(12_345.0), "12.3K");
    }
}
