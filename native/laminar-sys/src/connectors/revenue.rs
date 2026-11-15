//! Revenue: Stripe, Lemon Squeezy, Paddle, Shopify, Gumroad, Polar (all "last 30 days"
//! with 30 daily totals and the change versus the 30 days before), and RevenueCat (MRR).
//!
//! Each connector asks for 60 days so the previous-period change comes from the same
//! data. Paged sources stop at a page cap; if the cap is hit the change is left out
//! (the older window may be incomplete) but the newest 30 days are normally complete.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::{json, Value};

const F_KEY: &str = "api_key";

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "stripe",
        name: "Stripe",
        about: "Net revenue (charges minus refunds and fees) over the last 30 days from your balance history; use a restricted key with read access to Balance only.",
        help_url: "https://dashboard.stripe.com/apikeys/create",
        fields: &[Field { key: F_KEY, label: "Restricted key", placeholder: "rk_live_…", secret: true, required: true }],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "lemonsqueezy",
        name: "Lemon Squeezy",
        about: "Order revenue (in USD, minus refunds) over the last 30 days; needs an API key, used here only to read orders.",
        help_url: "https://app.lemonsqueezy.com/settings/api",
        fields: &[
            Field { key: F_KEY, label: "API key", placeholder: "eyJ0eXAiOiJKV1Qi…", secret: true, required: true },
            Field { key: "store_id", label: "Store ID (optional)", placeholder: "12345", secret: false, required: false },
        ],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "paddle",
        name: "Paddle",
        about: "Completed transaction revenue over the last 30 days from Paddle Billing; create an API key with read-only permission for transactions.",
        help_url: "https://vendors.paddle.com/authentication-v2",
        fields: &[Field { key: F_KEY, label: "API key", placeholder: "pdl_live_apikey_…", secret: true, required: true }],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "shopify",
        name: "Shopify",
        about: "Order revenue over the last 30 days from your store; needs an Admin API access token from a custom app with only the read_orders scope.",
        help_url: "https://help.shopify.com/en/manual/apps/app-types/custom-apps",
        fields: &[
            Field { key: "shop", label: "Store domain", placeholder: "yourstore.myshopify.com", secret: false, required: true },
            Field { key: "access_token", label: "Admin API access token", placeholder: "shpat_…", secret: true, required: true },
        ],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "gumroad",
        name: "Gumroad",
        about: "Sales revenue over the last 30 days; needs an access token from an application you create in Gumroad (read-only use).",
        help_url: "https://app.gumroad.com/settings/advanced#application-form",
        fields: &[Field { key: "access_token", label: "Access token", placeholder: "…", secret: true, required: true }],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "polar",
        name: "Polar",
        about: "Revenue over the last 30 days from Polar's metrics; needs an organization access token with only the metrics:read scope.",
        help_url: "https://polar.sh/dashboard",
        fields: &[Field { key: "access_token", label: "Organization access token", placeholder: "polar_oat_…", secret: true, required: true }],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "revenuecat",
        name: "RevenueCat",
        about: "MRR, active subscriptions and trials from your project's overview; needs a v2 secret API key with read-only Charts & Metrics permission.",
        help_url: "https://app.revenuecat.com/",
        fields: &[
            Field { key: F_KEY, label: "Secret API key (v2)", placeholder: "sk_…", secret: true, required: true },
            Field { key: "project_id", label: "Project ID", placeholder: "proj1ab2c3d4", secret: false, required: true },
        ],
        refresh_secs: 900,
    },
];

/// Start of the 60-day window (two 30-day periods ending today).
fn window_start(now: i64) -> i64 {
    day_start(now) - 59 * DAY
}

// ---------------------------------------------------------------- Stripe

pub fn stripe(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, F_KEY, "Stripe restricted key")?;
    let auth = format!("Bearer {key}");
    let now = now();
    let mut pages = Vec::new();
    let mut after: Option<String> = None;
    let mut complete = false;
    for _ in 0..30 {
        let mut url = format!("https://api.stripe.com/v1/balance_transactions?created%5Bgte%5D={}&limit=100", window_start(now));
        if let Some(a) = &after {
            url.push_str(&format!("&starting_after={}", http::encode(a)));
        }
        let v = get_json(&url, &[("Authorization", auth.as_str())], "Stripe")?;
        let more = v["has_more"].as_bool() == Some(true);
        after = v["data"].as_array().and_then(|d| d.last()).and_then(|t| t["id"].as_str()).map(String::from);
        pages.push(v);
        if !more || after.is_none() {
            complete = true;
            break;
        }
    }
    Ok(parse_stripe(&pages, now, complete))
}

/// Balance transaction pages → feed. Sums `net` (minor units) of charges/payments and
/// their refunds, in the account's balance currency.
fn parse_stripe(pages: &[Value], now: i64, complete: bool) -> Feed {
    let mut pts = Vec::new();
    let mut currency: Option<String> = None;
    for v in pages {
        for t in v["data"].as_array().into_iter().flatten() {
            let ty = t["type"].as_str().unwrap_or("");
            if !matches!(ty, "charge" | "payment" | "refund" | "payment_refund" | "payment_failure_refund") {
                continue;
            }
            let c = t["currency"].as_str().unwrap_or("usd").to_uppercase();
            if currency.get_or_insert_with(|| c.clone()) != &c {
                continue;
            }
            pts.push((t["created"].as_i64().unwrap_or(0), num0(&t["net"]) / minor_per_major(&c)));
        }
    }
    revenue_feed(&pts, now, currency.as_deref().unwrap_or("USD"), complete)
}

// ---------------------------------------------------------------- Lemon Squeezy

pub fn lemonsqueezy(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, F_KEY, "Lemon Squeezy API key")?;
    let auth = format!("Bearer {key}");
    let h = [("Authorization", auth.as_str()), ("Accept", "application/vnd.api+json")];
    let store = opt(p, "store_id").map(|s| format!("&filter%5Bstore_id%5D={}", http::encode(s))).unwrap_or_default();
    let now = now();
    let start = window_start(now);
    let mut pages = Vec::new();
    let mut complete = false;
    for n in 1..=10 {
        let url = format!("https://api.lemonsqueezy.com/v1/orders?page%5Bsize%5D=100&page%5Bnumber%5D={n}{store}");
        let v = get_json(&url, &h, "Lemon Squeezy")?;
        let last = v["meta"]["page"]["lastPage"].as_u64().unwrap_or(1);
        // Orders come newest first: stop once a page reaches past the window.
        let oldest = v["data"].as_array().and_then(|d| d.iter().filter_map(|o| o["attributes"]["created_at"].as_str().and_then(parse_time)).min());
        pages.push(v);
        if n as u64 >= last || oldest.is_none_or(|t| t < start) {
            complete = true;
            break;
        }
    }
    Ok(parse_lemonsqueezy(&pages, now, complete))
}

/// Orders (JSON:API) → feed. Uses the USD amounts (cents) minus refunds.
fn parse_lemonsqueezy(pages: &[Value], now: i64, complete: bool) -> Feed {
    let mut pts = Vec::new();
    for v in pages {
        for o in v["data"].as_array().into_iter().flatten() {
            let a = &o["attributes"];
            if !matches!(a["status"].as_str(), Some("paid" | "refunded" | "partial_refund")) {
                continue;
            }
            let Some(t) = a["created_at"].as_str().and_then(parse_time) else { continue };
            let net = num0(&a["total_usd"]) - num0(&a["refunded_amount_usd"]);
            pts.push((t, net / 100.0));
        }
    }
    revenue_feed(&pts, now, "USD", complete)
}

// ---------------------------------------------------------------- Paddle

pub fn paddle(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, F_KEY, "Paddle API key")?;
    let auth = format!("Bearer {key}");
    let now = now();
    let mut url = format!(
        "https://api.paddle.com/transactions?status=completed&created_at%5BGTE%5D={}&per_page=30&order_by=created_at%5BDESC%5D",
        http::encode(&rfc3339(window_start(now)))
    );
    let mut pages = Vec::new();
    let mut complete = false;
    for _ in 0..25 {
        let v = get_json(&url, &[("Authorization", auth.as_str())], "Paddle")?;
        let pg = &v["meta"]["pagination"];
        let next = pg["next"].as_str().filter(|_| pg["has_more"].as_bool() == Some(true)).filter(|n| n.starts_with("https://api.paddle.com/")).map(String::from);
        pages.push(v);
        match next {
            Some(n) => url = n,
            None => {
                complete = true;
                break;
            }
        }
    }
    Ok(parse_paddle(&pages, now, complete))
}

/// Transactions → feed. Prefers payout totals (your payout currency) over the
/// customer-currency totals; amounts are strings in minor units.
fn parse_paddle(pages: &[Value], now: i64, complete: bool) -> Feed {
    let mut pts = Vec::new();
    let mut currency: Option<String> = None;
    for v in pages {
        for t in v["data"].as_array().into_iter().flatten() {
            let d = &t["details"];
            let (amount, c) = if d["payout_totals"].is_object() {
                (num0(&d["payout_totals"]["total"]), d["payout_totals"]["currency_code"].as_str())
            } else {
                (num0(&d["totals"]["total"]), d["totals"]["currency_code"].as_str().or(t["currency_code"].as_str()))
            };
            let c = c.unwrap_or("USD").to_uppercase();
            if currency.get_or_insert_with(|| c.clone()) != &c {
                continue;
            }
            let when = t["billed_at"].as_str().or(t["created_at"].as_str()).and_then(parse_time).unwrap_or(0);
            pts.push((when, amount / minor_per_major(&c)));
        }
    }
    revenue_feed(&pts, now, currency.as_deref().unwrap_or("USD"), complete)
}

// ---------------------------------------------------------------- Shopify

const SHOPIFY_QUERY: &str = "query($cursor:String,$q:String){orders(first:250,after:$cursor,query:$q,sortKey:CREATED_AT,reverse:true){nodes{createdAt cancelledAt currentTotalPriceSet{shopMoney{amount currencyCode}}} pageInfo{hasNextPage endCursor}}}";

/// "https://yourstore.myshopify.com/" → "yourstore.myshopify.com".
fn shop_host(s: &str) -> String {
    let s = s.trim().trim_start_matches("https://").trim_start_matches("http://");
    s.split('/').next().unwrap_or("").to_lowercase()
}

pub fn shopify(p: &Params) -> Result<Feed, FeedError> {
    let shop = shop_host(need(p, "shop", "store domain (yourstore.myshopify.com)")?);
    let token = need(p, "access_token", "Shopify Admin API access token")?;
    if shop.is_empty() || !shop.contains('.') {
        return Err(FeedError::NeedsSetup("Add your store domain (yourstore.myshopify.com)".into()));
    }
    let url = format!("https://{shop}/admin/api/2026-07/graphql.json");
    let now = now();
    let q = format!("created_at:>={}", ymd(window_start(now)));
    let mut pages = Vec::new();
    let mut cursor = Value::Null;
    let mut complete = false;
    for _ in 0..8 {
        let body = json!({ "query": SHOPIFY_QUERY, "variables": { "cursor": cursor, "q": q } });
        let v = post_json(&url, &[("X-Shopify-Access-Token", token)], &body, "Shopify")?;
        graphql_error(&v, "Shopify")?;
        let pi = &v["data"]["orders"]["pageInfo"];
        let next = pi["endCursor"].as_str().filter(|_| pi["hasNextPage"].as_bool() == Some(true)).map(String::from);
        pages.push(v);
        match next {
            Some(c) => cursor = Value::String(c),
            None => {
                complete = true;
                break;
            }
        }
    }
    Ok(parse_shopify(&pages, now, complete))
}

/// GraphQL order pages → feed (current totals, i.e. after refunds/edits; cancelled skipped).
fn parse_shopify(pages: &[Value], now: i64, complete: bool) -> Feed {
    let mut pts = Vec::new();
    let mut currency: Option<String> = None;
    for v in pages {
        for o in v["data"]["orders"]["nodes"].as_array().into_iter().flatten() {
            if !o["cancelledAt"].is_null() {
                continue;
            }
            let m = &o["currentTotalPriceSet"]["shopMoney"];
            let c = m["currencyCode"].as_str().unwrap_or("USD").to_uppercase();
            currency.get_or_insert(c);
            let Some(t) = o["createdAt"].as_str().and_then(parse_time) else { continue };
            pts.push((t, num0(&m["amount"])));
        }
    }
    revenue_feed(&pts, now, currency.as_deref().unwrap_or("USD"), complete)
}

// ---------------------------------------------------------------- Gumroad

pub fn gumroad(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "access_token", "Gumroad access token")?;
    let auth = format!("Bearer {token}");
    let now = now();
    let mut pages = Vec::new();
    let mut key: Option<String> = None;
    let mut complete = false;
    for _ in 0..10 {
        let mut url = format!("https://api.gumroad.com/v2/sales?after={}", ymd(window_start(now)));
        if let Some(k) = &key {
            url.push_str(&format!("&page_key={}", http::encode(k)));
        }
        let v = get_json(&url, &[("Authorization", auth.as_str())], "Gumroad")?;
        if v["success"].as_bool() == Some(false) {
            return Err(FeedError::Unauthorized("Gumroad refused the token".into()));
        }
        key = v["next_page_key"].as_str().filter(|s| !s.is_empty()).map(String::from);
        pages.push(v);
        if key.is_none() {
            complete = true;
            break;
        }
    }
    Ok(parse_gumroad(&pages, now, complete))
}

/// Sales → feed. `price` is in cents; fully refunded sales are skipped.
fn parse_gumroad(pages: &[Value], now: i64, complete: bool) -> Feed {
    let mut pts = Vec::new();
    for v in pages {
        for s in v["sales"].as_array().into_iter().flatten() {
            if s["refunded"].as_bool() == Some(true) || s["chargedback"].as_bool() == Some(true) {
                continue;
            }
            let Some(t) = s["created_at"].as_str().and_then(parse_time) else { continue };
            pts.push((t, num0(&s["price"]) / 100.0));
        }
    }
    revenue_feed(&pts, now, "USD", complete)
}

// ---------------------------------------------------------------- Polar

pub fn polar(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "access_token", "Polar organization access token")?;
    let auth = format!("Bearer {token}");
    let now = now();
    let url = format!("https://api.polar.sh/v1/metrics/?start_date={}&end_date={}&interval=day&timezone=UTC", ymd(window_start(now)), ymd(now));
    let v = get_json(&url, &[("Authorization", auth.as_str())], "Polar")?;
    Ok(parse_polar(&v, now))
}

/// Metrics periods → feed. Currency values are in cents.
fn parse_polar(v: &Value, now: i64) -> Feed {
    let pts: Vec<(i64, f64)> = v["periods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["timestamp"].as_str().and_then(parse_time)?, num0(&p["revenue"]) / 100.0)))
        .collect();
    revenue_feed(&pts, now, "USD", true)
}

// ---------------------------------------------------------------- RevenueCat

pub fn revenuecat(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, F_KEY, "RevenueCat secret API key")?;
    let project = need(p, "project_id", "RevenueCat project ID")?;
    let auth = format!("Bearer {key}");
    let url = format!("https://api.revenuecat.com/v2/projects/{}/metrics/overview", http::encode(project));
    let v = get_json(&url, &[("Authorization", auth.as_str())], "RevenueCat")?;
    parse_revenuecat(&v)
}

fn parse_revenuecat(v: &Value) -> Result<Feed, FeedError> {
    let metrics = v["metrics"].as_array().ok_or_else(|| FeedError::Failed("RevenueCat: unexpected response".into()))?;
    let currency = v["currency"].as_str().unwrap_or("USD").to_uppercase();
    let get = |id: &str| metrics.iter().find(|m| m["id"].as_str() == Some(id)).and_then(|m| num(&m["value"]));
    let mut f = Feed { value: get("mrr").map(round2), currency: Some(currency.clone()), caption: "MRR".into(), ..Default::default() };
    for (id, money_value) in [("active_subscriptions", false), ("active_trials", false), ("revenue", true), ("new_customers", false), ("active_users", false)] {
        if let Some(x) = get(id) {
            f.extra.insert(id.into(), if money_value { money(x, &currency) } else { group(x, 0) });
        }
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> i64 {
        parse_time("2026-11-14T12:00:00Z").unwrap()
    }
    fn t(s: &str) -> i64 {
        parse_time(s).unwrap()
    }
    #[test]
    fn stripe_net() {
        let v: Value = serde_json::from_str(&format!(
            r#"{{"object":"list","has_more":false,"data":[
              {{"id":"txn_1","type":"charge","amount":10000,"fee":320,"net":9680,"currency":"usd","created":{}}},
              {{"id":"txn_2","type":"refund","amount":-2000,"fee":0,"net":-2000,"currency":"usd","created":{}}},
              {{"id":"txn_3","type":"payout","amount":-50000,"net":-50000,"currency":"usd","created":{}}},
              {{"id":"txn_4","type":"payment","amount":5000,"net":4820,"currency":"usd","created":{}}}]}}"#,
            t("2026-11-14T08:00:00Z"),
            t("2026-11-13T08:00:00Z"),
            t("2026-11-13T09:00:00Z"),
            t("2026-10-01T09:00:00Z")
        ))
        .unwrap();
        let f = parse_stripe(&[v], now(), true);
        assert_eq!(f.value, Some(76.8));
        assert_eq!(f.series[29], 96.8);
        assert_eq!(f.series[28], -20.0);
        assert_eq!(f.delta_pct.map(|d| d.round()), Some(59.0));
    }
    #[test]
    fn lemonsqueezy_orders() {
        let v: Value = serde_json::from_str(
            r#"{"meta":{"page":{"currentPage":1,"lastPage":1}},"data":[
              {"type":"orders","id":"1","attributes":{"status":"paid","total":999,"total_usd":1099,"refunded_amount_usd":0,"currency":"EUR","created_at":"2026-11-14T10:00:00.000000Z"}},
              {"type":"orders","id":"2","attributes":{"status":"partial_refund","total_usd":2000,"refunded_amount_usd":500,"created_at":"2026-11-10T10:00:00.000000Z"}},
              {"type":"orders","id":"3","attributes":{"status":"failed","total_usd":5000,"created_at":"2026-11-10T10:00:00.000000Z"}}]}"#,
        )
        .unwrap();
        let f = parse_lemonsqueezy(&[v], now(), true);
        assert_eq!(f.value, Some(25.99));
        assert_eq!(f.currency.as_deref(), Some("USD"));
    }
    #[test]
    fn paddle_transactions() {
        let v: Value = serde_json::from_str(
            r#"{"data":[
              {"id":"txn_1","status":"completed","currency_code":"EUR","billed_at":"2026-11-14T09:00:00.000Z","created_at":"2026-11-14T08:59:00.000Z",
               "details":{"totals":{"total":"1100","currency_code":"EUR"},"payout_totals":{"total":"1200","currency_code":"USD"}}},
              {"id":"txn_2","status":"completed","currency_code":"USD","billed_at":"2026-11-12T09:00:00Z",
               "details":{"totals":{"total":"16500","currency_code":"USD"}}}],
              "meta":{"pagination":{"per_page":30,"next":"https://api.paddle.com/transactions?after=txn_2","has_more":false}}}"#,
        )
        .unwrap();
        let f = parse_paddle(&[v], now(), true);
        assert_eq!(f.value, Some(177.0));
        assert_eq!(f.series[29], 12.0);
    }
    #[test]
    fn shopify_orders() {
        let v: Value = serde_json::from_str(
            r#"{"data":{"orders":{"nodes":[
              {"createdAt":"2026-11-14T03:00:00Z","cancelledAt":null,"currentTotalPriceSet":{"shopMoney":{"amount":"45.50","currencyCode":"CAD"}}},
              {"createdAt":"2026-11-13T03:00:00Z","cancelledAt":"2026-11-13T05:00:00Z","currentTotalPriceSet":{"shopMoney":{"amount":"99.00","currencyCode":"CAD"}}},
              {"createdAt":"2026-09-20T03:00:00Z","cancelledAt":null,"currentTotalPriceSet":{"shopMoney":{"amount":"20.00","currencyCode":"CAD"}}}],
              "pageInfo":{"hasNextPage":false,"endCursor":null}}}}"#,
        )
        .unwrap();
        let f = parse_shopify(&[v], now(), true);
        assert_eq!(f.value, Some(45.5));
        assert_eq!(f.currency.as_deref(), Some("CAD"));
        assert_eq!(shop_host("https://My.myshopify.com/admin"), "my.myshopify.com");
    }
    #[test]
    fn gumroad_sales() {
        let v: Value = serde_json::from_str(
            r#"{"success":true,"next_page_key":null,"sales":[
              {"id":"a","created_at":"2026-11-14T08:00:00Z","price":1500,"refunded":false},
              {"id":"b","created_at":"2026-11-13T08:00:00Z","price":900,"refunded":true}]}"#,
        )
        .unwrap();
        assert_eq!(parse_gumroad(&[v], now(), true).value, Some(15.0));
    }
    #[test]
    fn polar_metrics() {
        let v: Value = serde_json::from_str(
            r#"{"periods":[{"timestamp":"2026-11-13T00:00:00Z","revenue":2500,"orders":1},{"timestamp":"2026-11-14T00:00:00Z","revenue":1000},
              {"timestamp":"2026-10-01T00:00:00Z","revenue":7000}],"totals":{"revenue":10500},"metrics":{}}"#,
        )
        .unwrap();
        let f = parse_polar(&v, now());
        assert_eq!(f.value, Some(35.0));
        assert_eq!(f.delta_pct, Some(-50.0));
        assert_eq!(f.series[28], 25.0);
    }
    #[test]
    fn revenuecat_overview() {
        let v: Value = serde_json::from_str(
            r#"{"object":"overview_metrics","currency":"USD","metrics":[
              {"object":"overview_metric","id":"mrr","name":"MRR","unit":"$","period":"P28D","value":4321.5},
              {"object":"overview_metric","id":"active_subscriptions","value":321},
              {"object":"overview_metric","id":"active_trials","value":34},
              {"object":"overview_metric","id":"revenue","unit":"$","period":"P28D","value":5000}]}"#,
        )
        .unwrap();
        let f = parse_revenuecat(&v).unwrap();
        assert_eq!(f.value, Some(4321.5));
        assert_eq!(f.caption, "MRR");
        assert_eq!(f.extra["active_subscriptions"], "321");
        assert_eq!(f.extra["revenue"], "$5,000.00");
        assert!(f.series.is_empty());
    }
}
