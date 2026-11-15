//! Social audience counts: X, YouTube, TikTok, Instagram.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::Value;

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "x",
        name: "X",
        about: "Followers, following and post count for one account; needs an app Bearer token from the X developer portal (read-only, app-only).",
        help_url: "https://developer.x.com/en/portal/dashboard",
        fields: &[
            Field { key: "bearer_token", label: "Bearer token", placeholder: "AAAAAAAAAAAAAAAAAAAAA…", secret: true, required: true },
            Field { key: "username", label: "Username", placeholder: "jack", secret: false, required: true },
        ],
        refresh_secs: 1800,
    },
    ConnectorInfo {
        id: "youtube",
        name: "YouTube",
        about: "Subscribers, views and video count for one channel; needs a YouTube Data API key (restrict it to the YouTube Data API v3).",
        help_url: "https://console.cloud.google.com/apis/credentials",
        fields: &[
            Field { key: "api_key", label: "API key", placeholder: "AIza…", secret: true, required: true },
            Field { key: "channel", label: "Channel handle or ID", placeholder: "@name or UC…", secret: false, required: true },
        ],
        refresh_secs: 1800,
    },
    ConnectorInfo {
        id: "tiktok",
        name: "TikTok",
        about: "Followers, likes and video count for your account; needs a user access token from TikTok's Display API with only the user.info.stats scope.",
        help_url: "https://developers.tiktok.com/doc/display-api-get-started",
        fields: &[Field { key: "access_token", label: "User access token", placeholder: "act.…", secret: true, required: true }],
        refresh_secs: 1800,
    },
    ConnectorInfo {
        id: "instagram",
        name: "Instagram",
        about: "Followers and post count for a professional account; needs an access token (Instagram Login or Facebook Login) with basic read permission only.",
        help_url: "https://developers.facebook.com/docs/instagram-platform",
        fields: &[
            Field { key: "access_token", label: "Access token", placeholder: "IGAA… or EAA…", secret: true, required: true },
            Field { key: "user_id", label: "Instagram user ID (Facebook Login tokens)", placeholder: "17841400000000000", secret: false, required: false },
        ],
        refresh_secs: 1800,
    },
];

/// The standard followers feed.
fn followers(n: f64, extras: &[(&str, Option<f64>)]) -> Feed {
    let mut f = Feed { value: Some(n), unit: "followers".into(), caption: "Followers".into(), ..Default::default() };
    for (k, v) in extras {
        if let Some(v) = v {
            f.extra.insert(k.to_string(), compact(*v));
        }
    }
    f
}

fn unexpected(service: &str) -> FeedError {
    FeedError::Failed(format!("{service}: unexpected response"))
}

// ---------------------------------------------------------------- X

pub fn x(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "bearer_token", "X Bearer token")?;
    let user = need(p, "username", "X username")?.trim_start_matches('@');
    let auth = format!("Bearer {token}");
    let url = format!("https://api.x.com/2/users/by/username/{}?user.fields=public_metrics", http::encode(user));
    let v = get_json(&url, &[("Authorization", auth.as_str())], "X")?;
    parse_x(&v)
}

fn parse_x(v: &Value) -> Result<Feed, FeedError> {
    let m = &v["data"]["public_metrics"];
    let Some(n) = num(&m["followers_count"]) else {
        return match v["errors"][0]["title"].as_str() {
            Some(t) => Err(FeedError::Failed(format!("X: {t}"))),
            None => Err(unexpected("X")),
        };
    };
    let mut f = followers(n, &[("following", num(&m["following_count"])), ("posts", num(&m["tweet_count"]))]);
    if let Some(name) = v["data"]["name"].as_str() {
        f.extra.insert("name".into(), name.to_string());
    }
    Ok(f)
}

// ---------------------------------------------------------------- YouTube

pub fn youtube(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "YouTube Data API key")?;
    let ch = need(p, "channel", "channel handle (@name) or ID")?;
    let sel = if ch.starts_with("UC") && ch.len() == 24 && !ch.contains('@') { format!("id={}", http::encode(ch)) } else { format!("forHandle={}", http::encode(ch)) };
    let url = format!("https://www.googleapis.com/youtube/v3/channels?part=statistics,snippet&{sel}");
    // The key goes in a header so it never appears in a URL.
    let r = http::get(&url, &[("X-Goog-Api-Key", key)]).map_err(|e| net("YouTube", e))?;
    // Google answers a bad key with 400 (reason "API_KEY_INVALID"), not 401.
    if r.status == 400 && r.text().contains("API_KEY_INVALID") {
        return Err(FeedError::Unauthorized("YouTube refused the key (400)".into()));
    }
    parse_youtube(&json_of(&r, "YouTube")?)
}

fn parse_youtube(v: &Value) -> Result<Feed, FeedError> {
    let item = &v["items"][0];
    if item.is_null() {
        return Err(FeedError::NeedsSetup("YouTube: no channel found for that handle or ID".into()));
    }
    let s = &item["statistics"];
    let n = num(&s["subscriberCount"]).ok_or_else(|| unexpected("YouTube"))?;
    let mut f = followers(n, &[("views", num(&s["viewCount"])), ("videos", num(&s["videoCount"]))]);
    f.unit = "subscribers".into();
    f.caption = "Subscribers".into();
    if let Some(t) = item["snippet"]["title"].as_str() {
        f.extra.insert("name".into(), t.to_string());
    }
    Ok(f)
}

// ---------------------------------------------------------------- TikTok

pub fn tiktok(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "access_token", "TikTok user access token")?;
    let auth = format!("Bearer {token}");
    let r = http::get("https://open.tiktokapis.com/v2/user/info/?fields=follower_count,following_count,likes_count,video_count,display_name", &[("Authorization", auth.as_str())]).map_err(|e| net("TikTok", e))?;
    let v = json_of(&r, "TikTok")?;
    parse_tiktok(&v)
}

fn parse_tiktok(v: &Value) -> Result<Feed, FeedError> {
    match v["error"]["code"].as_str() {
        None | Some("ok") => {}
        Some("access_token_invalid") | Some("scope_not_authorized") => return Err(FeedError::Unauthorized("TikTok refused the token (reconnect it)".into())),
        Some(c) => return Err(FeedError::Failed(format!("TikTok: {c}"))),
    }
    let u = &v["data"]["user"];
    let n = num(&u["follower_count"]).ok_or_else(|| unexpected("TikTok"))?;
    let mut f = followers(n, &[("likes", num(&u["likes_count"])), ("videos", num(&u["video_count"])), ("following", num(&u["following_count"]))]);
    if let Some(name) = u["display_name"].as_str() {
        f.extra.insert("name".into(), name.to_string());
    }
    Ok(f)
}

// ---------------------------------------------------------------- Instagram

pub fn instagram(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "access_token", "Instagram access token")?;
    let auth = format!("Bearer {token}");
    let user = opt(p, "user_id");
    // Instagram Login tokens (IG…) talk to graph.instagram.com and can use "me";
    // Facebook Login tokens need the Instagram business account ID on graph.facebook.com.
    let url = if token.starts_with("IG") {
        format!("https://graph.instagram.com/v24.0/{}?fields=followers_count,media_count,username", http::encode(user.unwrap_or("me")))
    } else {
        let id = user.ok_or_else(|| FeedError::NeedsSetup("Add your Instagram user ID (needed with Facebook Login tokens)".into()))?;
        format!("https://graph.facebook.com/v24.0/{}?fields=followers_count,media_count,username", http::encode(id))
    };
    let v = get_json(&url, &[("Authorization", auth.as_str())], "Instagram")?;
    parse_instagram(&v)
}

fn parse_instagram(v: &Value) -> Result<Feed, FeedError> {
    if let Some(m) = v["error"]["message"].as_str() {
        let short: String = m.chars().take(100).collect();
        return Err(FeedError::Failed(format!("Instagram: {short}")));
    }
    let n = num(&v["followers_count"]).ok_or_else(|| unexpected("Instagram"))?;
    let mut f = followers(n, &[("posts", num(&v["media_count"]))]);
    if let Some(u) = v["username"].as_str() {
        f.extra.insert("name".into(), format!("@{u}"));
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn j(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }
    #[test]
    fn x_user() {
        let f = parse_x(&j(r#"{"data":{"id":"12","name":"jack","username":"jack","public_metrics":{"followers_count":6000000,"following_count":5000,"tweet_count":30000,"listed_count":1}}}"#)).unwrap();
        assert_eq!(f.value, Some(6e6));
        assert_eq!(f.extra["following"], "5,000");
        assert_eq!(f.extra["posts"], "30.0K");
        assert!(parse_x(&j(r#"{"errors":[{"title":"Not Found Error"}]}"#)).is_err());
    }
    #[test]
    fn youtube_channel() {
        let f = parse_youtube(&j(r#"{"items":[{"id":"UCx","snippet":{"title":"Chan"},"statistics":{"viewCount":"1234567","subscriberCount":"4200","hiddenSubscriberCount":false,"videoCount":"88"}}]}"#)).unwrap();
        assert_eq!(f.value, Some(4200.0));
        assert_eq!(f.extra["views"], "1.2M");
        assert_eq!(f.extra["videos"], "88");
        assert_eq!(f.caption, "Subscribers");
        assert!(matches!(parse_youtube(&j(r#"{"pageInfo":{"totalResults":0}}"#)), Err(FeedError::NeedsSetup(_))));
    }
    #[test]
    fn tiktok_user() {
        let f = parse_tiktok(&j(r#"{"data":{"user":{"follower_count":321,"likes_count":9000,"video_count":12}},"error":{"code":"ok","message":"","log_id":"x"}}"#)).unwrap();
        assert_eq!(f.value, Some(321.0));
        assert_eq!(f.extra["likes"], "9,000");
        assert!(matches!(parse_tiktok(&j(r#"{"error":{"code":"access_token_invalid"}}"#)), Err(FeedError::Unauthorized(_))));
    }
    #[test]
    fn instagram_user() {
        let f = parse_instagram(&j(r#"{"followers_count":1500,"media_count":40,"username":"me","id":"1"}"#)).unwrap();
        assert_eq!(f.value, Some(1500.0));
        assert_eq!(f.extra["posts"], "40");
        assert!(parse_instagram(&j(r#"{"error":{"message":"Invalid OAuth access token","code":190}}"#)).is_err());
    }
}
