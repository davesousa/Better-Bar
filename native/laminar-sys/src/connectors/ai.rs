//! AI spend: Anthropic and OpenAI admin cost reports, OpenRouter credits, Cursor team spend.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::{json, Value};
use std::collections::HashMap;

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "anthropic",
        name: "Anthropic API",
        about: "Your organization's Claude API cost this month and today's tokens, read from the Admin API; needs an Admin key (sk-ant-admin…), which only reads usage here.",
        help_url: "https://console.anthropic.com/settings/admin-keys",
        fields: &[Field { key: "admin_key", label: "Admin API key", placeholder: "sk-ant-admin01-…", secret: true, required: true }],
        refresh_secs: 600,
    },
    ConnectorInfo {
        id: "openai",
        name: "OpenAI API",
        about: "Your organization's OpenAI API cost this month and today's tokens; needs an Admin key (create it with read-only permissions).",
        help_url: "https://platform.openai.com/settings/organization/admin-keys",
        fields: &[Field { key: "admin_key", label: "Admin key", placeholder: "sk-admin-…", secret: true, required: true }],
        refresh_secs: 600,
    },
    ConnectorInfo {
        id: "openrouter",
        name: "OpenRouter",
        about: "Credits left on your OpenRouter account; OpenRouter only shares this with a management (provisioning) key.",
        help_url: "https://openrouter.ai/settings/provisioning-keys",
        fields: &[Field { key: "api_key", label: "Management key", placeholder: "sk-or-v1-…", secret: true, required: true }],
        refresh_secs: 600,
    },
    ConnectorInfo {
        id: "cursor",
        name: "Cursor (team)",
        about: "Your Cursor team's spend this billing cycle and the top spenders; needs a team Admin API key (team admins only).",
        help_url: "https://cursor.com/dashboard?tab=settings",
        fields: &[Field { key: "api_key", label: "Admin API key", placeholder: "key_…", secret: true, required: true }],
        refresh_secs: 600,
    },
];

/// Start of the current UTC month.
fn month_start(now: i64) -> i64 {
    let (y, m, _) = civil_from_days(now.div_euclid(DAY));
    days_from_civil(y, m, 1) * DAY
}

/// Month-to-date cost feed from daily (bucket start, USD) points and cost lines.
fn cost_feed(daily_pts: &[(i64, f64)], lines: Vec<(String, f64)>, now: i64) -> Feed {
    let start = month_start(now);
    let days = ((day_start(now) - start) / DAY + 1) as usize;
    let mut series = vec![0.0; days];
    for &(t, v) in daily_pts {
        if t >= start {
            if let Some(s) = series.get_mut(((t - start) / DAY) as usize) {
                *s += v;
            }
        }
    }
    let total: f64 = series.iter().sum();
    Feed {
        value: Some(round2(total)),
        currency: Some("USD".into()),
        caption: "This month".into(),
        series: series.into_iter().map(round2).collect(),
        rows: top_rows(lines, 4, total, |v| money(v, "USD")),
        ..Default::default()
    }
}

/// Adds amounts per label.
fn add(map: &mut HashMap<String, f64>, label: &str, v: f64) {
    *map.entry(label.to_string()).or_insert(0.0) += v;
}

// ---------------------------------------------------------------- Anthropic

pub fn anthropic(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "admin_key", "Anthropic Admin API key")?;
    let h = [("x-api-key", key), ("anthropic-version", "2023-06-01")];
    let now = now();
    let mut pages = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..3 {
        let mut url = format!(
            "https://api.anthropic.com/v1/organizations/cost_report?starting_at={}&bucket_width=1d&limit=31&group_by%5B%5D=description",
            http::encode(&rfc3339(month_start(now)))
        );
        if let Some(pg) = &page {
            url.push_str(&format!("&page={}", http::encode(pg)));
        }
        let v = get_json(&url, &h, "Anthropic")?;
        page = v["next_page"].as_str().filter(|_| v["has_more"].as_bool() == Some(true)).map(String::from);
        pages.push(v);
        if page.is_none() {
            break;
        }
    }
    let mut feed = parse_anthropic_cost(&pages, now);
    // Today's tokens (optional; the cost report is the headline).
    let url = format!("https://api.anthropic.com/v1/organizations/usage_report/messages?starting_at={}&bucket_width=1d&limit=1", http::encode(&rfc3339(day_start(now))));
    if let Ok(v) = get_json(&url, &h, "Anthropic") {
        feed.extra.insert("tokens_today".into(), format!("{}", anthropic_tokens(&v) as u64));
    }
    Ok(feed)
}

/// Cost report pages → feed. Amounts are decimal strings in cents.
fn parse_anthropic_cost(pages: &[Value], now: i64) -> Feed {
    let mut pts = Vec::new();
    let mut lines = HashMap::new();
    for v in pages {
        for b in v["data"].as_array().into_iter().flatten() {
            let t = b["starting_at"].as_str().and_then(parse_time).unwrap_or(0);
            for r in b["results"].as_array().into_iter().flatten() {
                let usd = num0(&r["amount"]) / 100.0;
                pts.push((t, usd));
                let label = r["model"].as_str().or(r["description"].as_str()).unwrap_or("Other");
                add(&mut lines, label, usd);
            }
        }
    }
    cost_feed(&pts, lines.into_iter().collect(), now)
}

/// Total tokens (uncached input + cache reads/writes + output) in a usage report.
fn anthropic_tokens(v: &Value) -> f64 {
    let mut n = 0.0;
    for b in v["data"].as_array().into_iter().flatten() {
        for r in b["results"].as_array().into_iter().flatten() {
            n += num0(&r["uncached_input_tokens"]) + num0(&r["cache_read_input_tokens"]) + num0(&r["output_tokens"]);
            n += num0(&r["cache_creation"]["ephemeral_1h_input_tokens"]) + num0(&r["cache_creation"]["ephemeral_5m_input_tokens"]);
        }
    }
    n
}

// ---------------------------------------------------------------- OpenAI

pub fn openai(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "admin_key", "OpenAI Admin key")?;
    let auth = format!("Bearer {key}");
    let h = [("Authorization", auth.as_str())];
    let now = now();
    let mut pages = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..3 {
        let mut url = format!("https://api.openai.com/v1/organization/costs?start_time={}&bucket_width=1d&limit=31&group_by=line_item", month_start(now));
        if let Some(pg) = &page {
            url.push_str(&format!("&page={}", http::encode(pg)));
        }
        let v = get_json(&url, &h, "OpenAI")?;
        page = v["next_page"].as_str().filter(|_| v["has_more"].as_bool() == Some(true)).map(String::from);
        pages.push(v);
        if page.is_none() {
            break;
        }
    }
    let mut feed = parse_openai_cost(&pages, now);
    let url = format!("https://api.openai.com/v1/organization/usage/completions?start_time={}&bucket_width=1d&limit=1", day_start(now));
    if let Ok(v) = get_json(&url, &h, "OpenAI") {
        feed.extra.insert("tokens_today".into(), format!("{}", openai_tokens(&v) as u64));
    }
    Ok(feed)
}

/// Costs pages → feed. `amount.value` is already in dollars.
fn parse_openai_cost(pages: &[Value], now: i64) -> Feed {
    let mut pts = Vec::new();
    let mut lines = HashMap::new();
    for v in pages {
        for b in v["data"].as_array().into_iter().flatten() {
            let t = b["start_time"].as_i64().unwrap_or(0);
            for r in b["results"].as_array().into_iter().flatten() {
                let usd = num0(&r["amount"]["value"]);
                pts.push((t, usd));
                add(&mut lines, r["line_item"].as_str().unwrap_or("Other"), usd);
            }
        }
    }
    cost_feed(&pts, lines.into_iter().collect(), now)
}

fn openai_tokens(v: &Value) -> f64 {
    let mut n = 0.0;
    for b in v["data"].as_array().into_iter().flatten() {
        for r in b["results"].as_array().into_iter().flatten() {
            n += num0(&r["input_tokens"]) + num0(&r["output_tokens"]);
        }
    }
    n
}

// ---------------------------------------------------------------- OpenRouter

pub fn openrouter(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "OpenRouter management key")?;
    let auth = format!("Bearer {key}");
    let v = get_json("https://openrouter.ai/api/v1/credits", &[("Authorization", auth.as_str())], "OpenRouter")?;
    parse_openrouter(&v)
}

fn parse_openrouter(v: &Value) -> Result<Feed, FeedError> {
    let d = &v["data"];
    let credits = num(&d["total_credits"]).ok_or_else(|| FeedError::Failed("OpenRouter: unexpected response".into()))?;
    let usage = num0(&d["total_usage"]);
    let mut f = Feed {
        value: Some(round2(credits - usage)),
        currency: Some("USD".into()),
        caption: "Credits left".into(),
        frac: (credits > 0.0).then(|| (usage / credits).clamp(0.0, 1.0) as f32),
        ..Default::default()
    };
    f.extra.insert("total_credits".into(), money(credits, "USD"));
    f.extra.insert("total_usage".into(), money(usage, "USD"));
    Ok(f)
}

// ---------------------------------------------------------------- Cursor

pub fn cursor(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "Cursor Admin API key")?;
    let auth = format!("Basic {}", http::base64(format!("{key}:").as_bytes()));
    let body = json!({ "sortBy": "amount", "sortDirection": "desc", "page": 1, "pageSize": 100 });
    let v = post_json("https://api.cursor.com/teams/spend", &[("Authorization", auth.as_str())], &body, "Cursor")?;
    Ok(parse_cursor(&v))
}

/// Team spend → feed. Cents (fractional) → dollars. `overallSpendCents` includes the
/// plan's included usage; `spendCents` is on-demand only.
fn parse_cursor(v: &Value) -> Feed {
    let mut total = 0.0;
    let mut on_demand = 0.0;
    let mut members = Vec::new();
    for m in v["teamMemberSpend"].as_array().into_iter().flatten() {
        let od = num0(&m["spendCents"]) / 100.0;
        let all = num(&m["overallSpendCents"]).map(|c| c / 100.0).unwrap_or(od);
        total += all;
        on_demand += od;
        let name = m["name"].as_str().filter(|s| !s.is_empty()).or(m["email"].as_str()).unwrap_or("Member");
        members.push((name.to_string(), all));
    }
    let mut f = Feed {
        value: Some(round2(total)),
        currency: Some("USD".into()),
        caption: "This cycle".into(),
        rows: top_rows(members, 4, total, |v| money(v, "USD")),
        ..Default::default()
    };
    f.extra.insert("on_demand".into(), money(on_demand, "USD"));
    if let Some(ms) = num(&v["subscriptionCycleStart"]) {
        f.extra.insert("cycle_start".into(), ymd((ms / 1000.0) as i64));
    }
    if let Some(n) = v["totalMembers"].as_u64() {
        f.extra.insert("members".into(), n.to_string());
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> i64 {
        parse_time("2026-11-03T15:00:00Z").unwrap()
    }
    #[test]
    fn anthropic_cost() {
        let v: Value = serde_json::from_str(
            r#"{"data":[
            {"starting_at":"2026-11-01T00:00:00Z","ending_at":"2026-11-02T00:00:00Z","results":[
              {"amount":"1200","currency":"USD","description":"Claude Opus Usage - Input Tokens","model":"claude-opus-5-5"},
              {"amount":"100","currency":"USD","description":"Web search","model":null}]},
            {"starting_at":"2026-11-02T00:00:00Z","ending_at":"2026-11-03T00:00:00Z","results":[]},
            {"starting_at":"2026-11-03T00:00:00Z","ending_at":"2026-11-04T00:00:00Z","results":[
              {"amount":"300","currency":"USD","description":"Claude Opus Usage - Output Tokens","model":"claude-opus-5-5"}]}],
            "has_more":false,"next_page":null}"#,
        )
        .unwrap();
        let f = parse_anthropic_cost(&[v], now());
        assert_eq!(f.value, Some(16.0));
        assert_eq!(f.series, vec![13.0, 0.0, 3.0]);
        assert_eq!(f.rows[0].label, "claude-opus-5-5");
        assert_eq!(f.rows[0].value, "$15.00");
        assert_eq!(f.rows[1].label, "Web search");
        let u: Value = serde_json::from_str(r#"{"data":[{"results":[{"uncached_input_tokens":1500,"cache_read_input_tokens":200,"output_tokens":500,"cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":10}}]}]}"#).unwrap();
        assert_eq!(anthropic_tokens(&u), 2210.0);
    }
    #[test]
    fn openai_cost() {
        let v: Value = serde_json::from_str(
            r#"{"object":"page","data":[{"object":"bucket","start_time":1793491200,"end_time":1793577600,"results":[
              {"object":"organization.costs.result","amount":{"value":1.5,"currency":"usd"},"line_item":"gpt-5, input"},
              {"object":"organization.costs.result","amount":{"value":0.5,"currency":"usd"},"line_item":"gpt-5, output"}]}],
              "has_more":false,"next_page":null}"#,
        )
        .unwrap();
        let f = parse_openai_cost(&[v], now());
        assert_eq!(f.value, Some(2.0));
        assert_eq!(f.series.len(), 3);
        assert_eq!(f.series[0], 2.0);
        assert_eq!(f.rows.len(), 2);
        assert_eq!(f.rows[0].frac, Some(0.75));
    }
    #[test]
    fn openrouter_credits() {
        let v: Value = serde_json::from_str(r#"{"data":{"total_credits":100.5,"total_usage":25.75}}"#).unwrap();
        let f = parse_openrouter(&v).unwrap();
        assert_eq!(f.value, Some(74.75));
        assert_eq!(f.extra["total_usage"], "$25.75");
        assert!((f.frac.unwrap() - 0.2562).abs() < 0.001);
    }
    #[test]
    fn cursor_spend() {
        let v: Value = serde_json::from_str(
            r#"{"teamMemberSpend":[
              {"userId":"u1","spendCents":2450.125487,"overallSpendCents":4450.125487,"name":"Alex","email":"a@x.com"},
              {"userId":"u2","spendCents":0,"overallSpendCents":2000,"name":"","email":"b@x.com"}],
              "subscriptionCycleStart":1708992000000,"totalMembers":2,"totalPages":1}"#,
        )
        .unwrap();
        let f = parse_cursor(&v);
        assert_eq!(f.value, Some(64.5));
        assert_eq!(f.rows[1].label, "b@x.com");
        assert_eq!(f.extra["on_demand"], "$24.50");
        assert_eq!(f.extra["cycle_start"], "2024-02-27");
    }
}
