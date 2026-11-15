//! Google services signed in with the user's own OAuth client (client id + secret +
//! refresh token): Search Console and Gmail.
//!
//! The refresh token is exchanged for a short-lived access token, which is cached in
//! memory only (keyed by the client id plus a hash of the refresh token, so Gmail and
//! Search Console can use different tokens from one client). Nothing is written to disk.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

/// The OAuth fields every Google connector asks for.
pub(super) const GOOGLE_FIELDS: [Field; 3] = [
    Field { key: "client_id", label: "OAuth client ID", placeholder: "1234-abc.apps.googleusercontent.com", secret: false, required: true },
    Field { key: "client_secret", label: "OAuth client secret", placeholder: "GOCSPX-…", secret: true, required: true },
    Field { key: "refresh_token", label: "Refresh token", placeholder: "1//0g…", secret: true, required: true },
];

static SC_FIELDS: [Field; 4] = [GOOGLE_FIELDS[0], GOOGLE_FIELDS[1], GOOGLE_FIELDS[2], Field { key: "site_url", label: "Property", placeholder: "sc-domain:example.com or https://example.com/", secret: false, required: true }];

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "search_console",
        name: "Google Search Console",
        about: "Search clicks, impressions, CTR and position for one property over 28 days; needs your own Google OAuth client and a refresh token with the read-only webmasters.readonly scope.",
        help_url: "https://console.cloud.google.com/apis/credentials",
        fields: &SC_FIELDS,
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "gmail",
        name: "Gmail",
        about: "Unread count and newest unread subjects in your inbox; needs your own Google OAuth client and a refresh token with the read-only gmail.readonly scope.",
        help_url: "https://console.cloud.google.com/apis/credentials",
        fields: &GOOGLE_FIELDS,
        refresh_secs: 120,
    },
];

/// (cache key) → (access token, expiry unix seconds).
static TOKENS: Mutex<Option<HashMap<String, (String, i64)>>> = Mutex::new(None);

fn cache_key(client_id: &str, refresh: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    refresh.hash(&mut h);
    format!("{client_id}#{:x}", h.finish())
}

/// A valid access token for the OAuth fields in `p` (cached until a minute before expiry).
pub(super) fn access_token(p: &Params, service: &str) -> Result<String, FeedError> {
    let id = need(p, "client_id", "Google OAuth client ID")?;
    let secret = need(p, "client_secret", "Google OAuth client secret")?;
    let refresh = need(p, "refresh_token", "Google refresh token")?;
    let key = cache_key(id, refresh);
    let now = now();
    if let Ok(g) = TOKENS.lock() {
        if let Some((tok, exp)) = g.as_ref().and_then(|m| m.get(&key)) {
            if *exp > now + 60 {
                return Ok(tok.clone());
            }
        }
    }
    let form = [("grant_type", "refresh_token"), ("client_id", id), ("client_secret", secret), ("refresh_token", refresh)];
    let r = http::post_form("https://oauth2.googleapis.com/token", &[], &form).map_err(|e| net(service, e))?;
    if r.status == 400 || r.status == 401 {
        // invalid_grant (revoked/expired refresh token) or invalid_client.
        return Err(FeedError::Unauthorized(format!("Google refused the sign-in for {service} (reconnect it)")));
    }
    let v = json_of(&r, service)?;
    let (tok, exp) = parse_token(&v, now).ok_or_else(|| FeedError::Failed(format!("{service}: Google sign-in failed")))?;
    if let Ok(mut g) = TOKENS.lock() {
        g.get_or_insert_with(HashMap::new).insert(key, (tok.clone(), exp));
    }
    Ok(tok)
}

fn parse_token(v: &Value, now: i64) -> Option<(String, i64)> {
    let tok = v["access_token"].as_str()?.to_string();
    Some((tok, now + v["expires_in"].as_i64().unwrap_or(3600)))
}

/// Forgets cached tokens for this client (after a 401 from the API itself).
fn forget(p: &Params) {
    if let (Some(id), Some(r)) = (p.get("client_id"), p.get("refresh_token")) {
        if let Ok(mut g) = TOKENS.lock() {
            if let Some(m) = g.as_mut() {
                m.remove(&cache_key(id.trim(), r.trim()));
            }
        }
    }
}

// ---------------------------------------------------------------- Search Console

pub fn search_console(p: &Params) -> Result<Feed, FeedError> {
    let site = need(p, "site_url", "Search Console property (site URL)")?;
    let tok = access_token(p, "Search Console")?;
    let auth = format!("Bearer {tok}");
    let now = now();
    let url = format!("https://www.googleapis.com/webmasters/v3/sites/{}/searchAnalytics/query", http::encode(site));
    let body = json!({ "startDate": ymd(now - 27 * DAY), "endDate": ymd(now), "dimensions": ["date"], "rowLimit": 100, "dataState": "all" });
    let r = post_json(&url, &[("Authorization", auth.as_str())], &body, "Search Console");
    if matches!(r, Err(FeedError::Unauthorized(_))) {
        forget(p);
    }
    Ok(parse_search_console(&r?))
}

/// Rows by date → clicks (headline + daily series), impressions, CTR and average position
/// (impression-weighted). Search data lags 2–3 days, so the newest days may be missing.
fn parse_search_console(v: &Value) -> Feed {
    let mut rows: Vec<(&str, &Value)> = v["rows"].as_array().into_iter().flatten().filter_map(|r| Some((r["keys"][0].as_str()?, r))).collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    let (mut clicks, mut impr, mut pos_w) = (0.0, 0.0, 0.0);
    let mut series = Vec::new();
    for (_, r) in &rows {
        let c = num0(&r["clicks"]);
        let i = num0(&r["impressions"]);
        clicks += c;
        impr += i;
        pos_w += num0(&r["position"]) * i;
        series.push(c);
    }
    let mut f = Feed { value: Some(clicks), unit: "clicks".into(), caption: "Clicks, 28 days".into(), series, ..Default::default() };
    f.extra.insert("impressions".into(), group(impr, 0));
    if impr > 0.0 {
        f.extra.insert("ctr".into(), format!("{:.1}%", clicks / impr * 100.0));
        f.extra.insert("position".into(), format!("{:.1}", pos_w / impr));
    }
    f
}

// ---------------------------------------------------------------- Gmail

pub fn gmail(p: &Params) -> Result<Feed, FeedError> {
    let tok = access_token(p, "Gmail")?;
    let auth = format!("Bearer {tok}");
    let h = [("Authorization", auth.as_str())];
    let base = "https://gmail.googleapis.com/gmail/v1/users/me";
    let r = get_json(&format!("{base}/labels/INBOX"), &h, "Gmail");
    if matches!(r, Err(FeedError::Unauthorized(_))) {
        forget(p);
    }
    let mut f = parse_gmail_label(&r?);
    if f.value.unwrap_or(0.0) > 0.0 {
        if let Ok(list) = get_json(&format!("{base}/messages?labelIds=INBOX&labelIds=UNREAD&maxResults=4"), &h, "Gmail") {
            for m in list["messages"].as_array().into_iter().flatten().take(4) {
                let Some(id) = m["id"].as_str() else { continue };
                let url = format!("{base}/messages/{}?format=metadata&metadataHeaders=Subject&metadataHeaders=From", http::encode(id));
                if let Ok(msg) = get_json(&url, &h, "Gmail") {
                    f.rows.push(parse_gmail_message(&msg));
                }
            }
        }
    }
    Ok(f)
}

fn parse_gmail_label(v: &Value) -> Feed {
    let mut f = Feed { value: num(&v["messagesUnread"]).or(Some(0.0)), caption: "Unread".into(), ..Default::default() };
    if let Some(t) = num(&v["messagesTotal"]) {
        f.extra.insert("total".into(), group(t, 0));
    }
    if let Some(t) = num(&v["threadsUnread"]) {
        f.extra.insert("threads_unread".into(), group(t, 0));
    }
    f
}

/// A message (format=metadata) → row: label = subject, value = sender's display name.
fn parse_gmail_message(v: &Value) -> FeedRow {
    let header = |name: &str| {
        v["payload"]["headers"].as_array().into_iter().flatten().find(|h| h["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name))).and_then(|h| h["value"].as_str()).unwrap_or("")
    };
    let subject = header("Subject");
    let from = header("From");
    // "Jane Doe <jane@x.com>" → "Jane Doe"; bare address stays.
    let sender = from.split('<').next().unwrap_or("").trim().trim_matches('"');
    let sender = if sender.is_empty() { from.trim_matches(|c| c == '<' || c == '>') } else { sender };
    FeedRow { label: if subject.is_empty() { "(no subject)".into() } else { subject.to_string() }, value: sender.to_string(), frac: None, delta: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token() {
        let v: Value = serde_json::from_str(r#"{"access_token":"ya29.x","expires_in":3599,"scope":"s","token_type":"Bearer"}"#).unwrap();
        assert_eq!(parse_token(&v, 100), Some(("ya29.x".into(), 3699)));
        assert_ne!(cache_key("c", "a"), cache_key("c", "b"));
    }
    #[test]
    fn search_console_rows() {
        let v: Value = serde_json::from_str(
            r#"{"rows":[{"keys":["2026-11-02"],"clicks":10,"impressions":400,"ctr":0.025,"position":8.0},
                        {"keys":["2026-11-01"],"clicks":30,"impressions":600,"ctr":0.05,"position":3.0}],"responseAggregationType":"byProperty"}"#,
        )
        .unwrap();
        let f = parse_search_console(&v);
        assert_eq!(f.value, Some(40.0));
        assert_eq!(f.series, vec![30.0, 10.0]);
        assert_eq!(f.extra["impressions"], "1,000");
        assert_eq!(f.extra["ctr"], "4.0%");
        assert_eq!(f.extra["position"], "5.0");
    }
    #[test]
    fn gmail_parse() {
        let v: Value = serde_json::from_str(r#"{"id":"INBOX","name":"INBOX","messagesTotal":1200,"messagesUnread":7,"threadsUnread":5}"#).unwrap();
        let f = parse_gmail_label(&v);
        assert_eq!(f.value, Some(7.0));
        assert_eq!(f.extra["total"], "1,200");
        let m: Value = serde_json::from_str(r#"{"id":"1","payload":{"headers":[{"name":"From","value":"\"Jane Doe\" <jane@x.com>"},{"name":"Subject","value":"Lunch?"}]}}"#).unwrap();
        let r = parse_gmail_message(&m);
        assert_eq!((r.label.as_str(), r.value.as_str()), ("Lunch?", "Jane Doe"));
        let m: Value = serde_json::from_str(r#"{"payload":{"headers":[{"name":"from","value":"<a@b.com>"}]}}"#).unwrap();
        assert_eq!(parse_gmail_message(&m).value, "a@b.com");
    }
}
