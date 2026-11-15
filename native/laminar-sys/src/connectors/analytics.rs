//! Web analytics: Plausible and Umami (visitors over the last 30 days, a daily series,
//! change versus the 30 days before, and pageviews). Search Console lives in google.rs.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::{json, Value};

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "plausible",
        name: "Plausible",
        about: "Visitors and pageviews for one site over the last 30 days; needs a Stats API key (read-only).",
        help_url: "https://plausible.io/settings/api-keys",
        fields: &[
            Field { key: "api_key", label: "Stats API key", placeholder: "…", secret: true, required: true },
            Field { key: "site_id", label: "Site domain", placeholder: "example.com", secret: false, required: true },
            Field { key: "base_url", label: "Server (self-hosted only)", placeholder: "https://plausible.io", secret: false, required: false },
        ],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "umami",
        name: "Umami",
        about: "Visitors and pageviews for one website over the last 30 days; needs an Umami Cloud API key (read-only) or a self-hosted API base and token.",
        help_url: "https://cloud.umami.is/settings/api-keys",
        fields: &[
            Field { key: "api_key", label: "API key", placeholder: "api_…", secret: true, required: true },
            Field { key: "website_id", label: "Website ID", placeholder: "4fb7fa4c-5b46-438d-94b3-3a8fb9bc2e8b", secret: false, required: true },
            Field { key: "base_url", label: "API base (self-hosted only)", placeholder: "https://api.umami.is/v1", secret: false, required: false },
        ],
        refresh_secs: 900,
    },
];

fn base(p: &Params, default: &str) -> String {
    opt(p, "base_url").unwrap_or(default).trim_end_matches('/').to_string()
}

/// Visitors feed from daily (day start, visitors, pageviews) points covering 60 days.
/// `exact` = (visitors, pageviews) totals for the last 30 days when the service gives
/// true unique counts; otherwise daily sums are used.
fn visitors_feed(days: &[(i64, f64, f64)], exact: Option<(f64, f64)>, prev_exact: Option<f64>, now: i64) -> Feed {
    let vis: Vec<(i64, f64)> = days.iter().map(|d| (d.0, d.1)).collect();
    let pv: Vec<(i64, f64)> = days.iter().map(|d| (d.0, d.2)).collect();
    let (series, prev_sum) = daily(&vis, 30, now);
    let (pv_series, _) = daily(&pv, 30, now);
    let sum: f64 = series.iter().sum();
    let (value, pageviews) = exact.unwrap_or((sum, pv_series.iter().sum()));
    let delta = match prev_exact {
        Some(prev) => pct(value, prev),
        None => pct(sum, prev_sum),
    };
    let mut f = Feed { value: Some(value), unit: "visitors".into(), caption: "Last 30 days".into(), delta_pct: delta, series, ..Default::default() };
    f.extra.insert("pageviews".into(), group(pageviews, 0));
    f
}

// ---------------------------------------------------------------- Plausible

pub fn plausible(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "Plausible Stats API key")?;
    let site = need(p, "site_id", "site domain")?;
    let url = format!("{}/api/v2/query", base(p, "https://plausible.io"));
    let auth = format!("Bearer {key}");
    let h = [("Authorization", auth.as_str())];
    let now = now();
    let daily_q = json!({ "site_id": site, "metrics": ["visitors", "pageviews"], "date_range": [ymd(now - 59 * DAY), ymd(now)], "dimensions": ["time:day"] });
    let d = post_json(&url, &h, &daily_q, "Plausible")?;
    let total_q = json!({ "site_id": site, "metrics": ["visitors", "pageviews"], "date_range": [ymd(now - 29 * DAY), ymd(now)] });
    let t = post_json(&url, &h, &total_q, "Plausible").ok();
    Ok(parse_plausible(&d, t.as_ref(), now))
}

fn parse_plausible(daily_v: &Value, total: Option<&Value>, now: i64) -> Feed {
    let days: Vec<(i64, f64, f64)> = daily_v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r["dimensions"][0].as_str().and_then(parse_time)?, num0(&r["metrics"][0]), num0(&r["metrics"][1]))))
        .collect();
    let exact = total.and_then(|t| {
        let m = &t["results"][0]["metrics"];
        Some((num(&m[0])?, num0(&m[1])))
    });
    visitors_feed(&days, exact, None, now)
}

// ---------------------------------------------------------------- Umami

pub fn umami(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "Umami API key")?;
    let site = need(p, "website_id", "Umami website ID")?;
    let b = base(p, "https://api.umami.is/v1");
    let auth = format!("Bearer {key}");
    let h = [("x-umami-api-key", key), ("Authorization", auth.as_str()), ("Accept", "application/json")];
    let now = now();
    let end_ms = now * 1000;
    let start30 = (day_start(now) - 29 * DAY) * 1000;
    let start60 = (day_start(now) - 59 * DAY) * 1000;
    let site = http::encode(site);
    let pv = get_json(&format!("{b}/websites/{site}/pageviews?startAt={start60}&endAt={end_ms}&unit=day&timezone=UTC"), &h, "Umami")?;
    let stats = get_json(&format!("{b}/websites/{site}/stats?startAt={start30}&endAt={end_ms}"), &h, "Umami").ok();
    Ok(parse_umami(&pv, stats.as_ref(), now))
}

/// A stats metric in either response shape: `{"visitors":{"value":5,"prev":4}}` (Umami
/// 2.x) or `{"visitors":5,"comparison":{"visitors":4}}` (newer). Returns (value, prev).
fn umami_metric(s: &Value, k: &str) -> Option<(f64, Option<f64>)> {
    match &s[k] {
        Value::Object(o) => Some((num(o.get("value")?)?, o.get("prev").and_then(num))),
        v => Some((num(v)?, num(&s["comparison"][k]))),
    }
}

fn parse_umami(pv: &Value, stats: Option<&Value>, now: i64) -> Feed {
    let mut map: std::collections::BTreeMap<i64, (f64, f64)> = Default::default();
    for (k, slot) in [("sessions", 0), ("pageviews", 1)] {
        for pt in pv[k].as_array().into_iter().flatten() {
            let Some(t) = pt["x"].as_str().and_then(parse_time) else { continue };
            let e = map.entry(day_start(t)).or_insert((0.0, 0.0));
            if slot == 0 {
                e.0 += num0(&pt["y"]);
            } else {
                e.1 += num0(&pt["y"]);
            }
        }
    }
    let days: Vec<(i64, f64, f64)> = map.into_iter().map(|(t, (v, p))| (t, v, p)).collect();
    let vis = stats.and_then(|s| umami_metric(s, "visitors"));
    let pvs = stats.and_then(|s| umami_metric(s, "pageviews"));
    let exact = vis.map(|(v, _)| (v, pvs.map(|p| p.0).unwrap_or(0.0)));
    visitors_feed(&days, exact, vis.and_then(|v| v.1), now)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> i64 {
        parse_time("2026-11-14T12:00:00Z").unwrap()
    }
    #[test]
    fn plausible_query() {
        let d: Value = serde_json::from_str(
            r#"{"results":[{"dimensions":["2026-11-13"],"metrics":[10,30]},{"dimensions":["2026-11-14"],"metrics":[5,9]},
                           {"dimensions":["2026-10-01"],"metrics":[20,40]}],"meta":{},"query":{}}"#,
        )
        .unwrap();
        let t: Value = serde_json::from_str(r#"{"results":[{"metrics":[12,39],"dimensions":[]}]}"#).unwrap();
        let f = parse_plausible(&d, Some(&t), now());
        assert_eq!(f.value, Some(12.0));
        assert_eq!(f.series[28..], [10.0, 5.0]);
        assert_eq!(f.delta_pct, Some(-25.0));
        assert_eq!(f.extra["pageviews"], "39");
        let f = parse_plausible(&d, None, now());
        assert_eq!(f.value, Some(15.0));
    }
    #[test]
    fn umami_both_shapes() {
        let pv: Value = serde_json::from_str(
            r#"{"pageviews":[{"x":"2026-11-14 00:00:00","y":9},{"x":"2026-11-13 00:00:00","y":30}],
                "sessions":[{"x":"2026-11-14 00:00:00","y":5},{"x":"2026-11-13 00:00:00","y":10}]}"#,
        )
        .unwrap();
        let old: Value = serde_json::from_str(r#"{"pageviews":{"value":39,"prev":20},"visitors":{"value":14,"prev":7}}"#).unwrap();
        let f = parse_umami(&pv, Some(&old), now());
        assert_eq!(f.value, Some(14.0));
        assert_eq!(f.delta_pct, Some(100.0));
        assert_eq!(f.series[29], 5.0);
        assert_eq!(f.extra["pageviews"], "39");
        let new: Value = serde_json::from_str(r#"{"pageviews":39,"visitors":14,"visits":16,"comparison":{"pageviews":30,"visitors":28}}"#).unwrap();
        assert_eq!(parse_umami(&pv, Some(&new), now()).delta_pct, Some(-50.0));
    }
}
