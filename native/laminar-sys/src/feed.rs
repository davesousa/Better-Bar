//! The common data shape every widget connector returns.
//!
//! A connector (Stripe, GitHub, Yahoo Finance, the local Claude Code logs…) turns whatever
//! its source says into one `Feed`. Widgets then pick the parts they show: the headline
//! number, a chart series, a list of rows, a heat-map grid, or named extras. Keeping one
//! shape means the renderer never needs to know about any particular service.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One row in a list (top processes, usage buckets, watchlist symbols, upcoming events).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeedRow {
    pub label: String,
    /// Already formatted ("12.4%", "$1,204", "3:00 PM").
    pub value: String,
    /// 0..1 for a bar, if the row has one.
    pub frac: Option<f32>,
    /// Signed change for the row (e.g. a stock's day change in percent), if any.
    pub delta: Option<f64>,
}

/// What a connector returns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Feed {
    /// Headline number (revenue, followers, balance, price, tokens…).
    pub value: Option<f64>,
    /// Headline as text when it isn't a plain number (e.g. "Pro plan").
    pub text: Option<String>,
    /// ISO currency of `value` ("USD"), when it is money.
    pub currency: Option<String>,
    /// Unit suffix for non-money values ("%", "GB", "tokens", "followers").
    pub unit: String,
    /// What the headline covers, shown under it ("Last 30 days", "Subscribers").
    pub caption: String,
    /// Change versus the previous period, in percent (+12.5 = up 12.5%).
    pub delta_pct: Option<f64>,
    /// Chart values, oldest first (daily revenue, price ticks…). Any length.
    pub series: Vec<f64>,
    /// Labelled rows (see FeedRow).
    pub rows: Vec<FeedRow>,
    /// Heat-map intensities 0..1, oldest first, one per day (contribution graphs).
    pub grid: Vec<f32>,
    /// 0..1 progress for rings and bars (quota used, goal reached), if meaningful.
    pub frac: Option<f32>,
    /// Anything else a widget may show, by name (e.g. "streak" → "7", "limit" → "$100").
    pub extra: BTreeMap<String, String>,
}

/// Why a connector couldn't produce a feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeedError {
    /// The user hasn't connected it yet, or a required setting is missing. The text says what.
    NeedsSetup(String),
    /// The service refused the key (401/403).
    Unauthorized(String),
    /// Network trouble, rate limits, server errors, unexpected responses.
    Failed(String),
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedError::NeedsSetup(s) | FeedError::Unauthorized(s) | FeedError::Failed(s) => f.write_str(s),
        }
    }
}

/// A setting the user types in for a connector.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    /// Example shown in the empty field.
    pub placeholder: &'static str,
    /// Stored in Windows Credential Manager (not config.json) and shown as dots.
    pub secret: bool,
    /// The connector can't work without it.
    pub required: bool,
}

/// Describes a connector to the UI.
#[derive(Debug, Clone, Copy)]
pub struct ConnectorInfo {
    pub id: &'static str,
    /// Service name ("Stripe").
    pub name: &'static str,
    /// One sentence: what it reads and what access it needs.
    pub about: &'static str,
    /// Where to create the key or find the setting.
    pub help_url: &'static str,
    pub fields: &'static [Field],
    /// How often to refresh, in seconds.
    pub refresh_secs: u64,
}

/// Values passed to a connector: every field by key (secrets included, read from
/// Credential Manager by the caller). Missing fields are absent or empty.
pub type Params = std::collections::HashMap<String, String>;

/// Convenience: a required, non-empty parameter or NeedsSetup.
pub fn need<'a>(p: &'a Params, key: &str, what: &str) -> Result<&'a str, FeedError> {
    match p.get(key).map(|s| s.trim()) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(FeedError::NeedsSetup(format!("Add your {what}"))),
    }
}

/// Map an HTTP response to a FeedError for non-2xx statuses.
pub fn check(r: &crate::http::Response, service: &str) -> Result<(), FeedError> {
    match r.status {
        200..=299 => Ok(()),
        401 | 403 => Err(FeedError::Unauthorized(format!("{service} refused the key ({})", r.status))),
        404 => Err(FeedError::Failed(format!("{service}: not found (check the settings)"))),
        429 => Err(FeedError::Failed(format!("{service}: rate limited, will retry"))),
        s => Err(FeedError::Failed(format!("{service}: error {s}"))),
    }
}
