//! Markets without keys: stocks from Yahoo Finance's chart endpoint, crypto from CoinGecko.
//! Both are unofficial/free public endpoints with rate limits; refreshes are kept to 2 min.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::Value;

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "stocks",
        name: "Stocks",
        about: "Prices and today's change for a watchlist from Yahoo Finance; no key needed.",
        help_url: "https://finance.yahoo.com/",
        fields: &[Field { key: "symbols", label: "Symbols", placeholder: "AAPL,MSFT,NVDA,TSLA", secret: false, required: false }],
        refresh_secs: 120,
    },
    ConnectorInfo {
        id: "crypto",
        name: "Crypto",
        about: "Prices and 24-hour change for a list of coins from CoinGecko; no key needed.",
        help_url: "https://www.coingecko.com/",
        fields: &[Field { key: "coins", label: "Coins (CoinGecko IDs)", placeholder: "bitcoin,ethereum,solana", secret: false, required: false }],
        refresh_secs: 120,
    },
];

/// A comma/space separated list, or the default.
fn list(p: &Params, key: &str, default: &str) -> Vec<String> {
    let raw = opt(p, key).unwrap_or(default);
    let v: Vec<String> = raw.split([',', ' ', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).take(12).collect();
    if v.is_empty() {
        default.split(',').map(String::from).collect()
    } else {
        v
    }
}

/// Price text with enough decimals for small prices.
fn price_text(v: f64, currency: &str) -> String {
    if v.abs() >= 1.0 || v == 0.0 {
        money(v, currency)
    } else {
        let s = format!("{v:.6}");
        let s = s.trim_end_matches('0');
        if currency.eq_ignore_ascii_case("USD") {
            format!("${s}")
        } else {
            format!("{s} {}", currency.to_uppercase())
        }
    }
}

/// One instrument's snapshot.
#[derive(Debug, Clone, PartialEq)]
struct Quote {
    symbol: String,
    name: Option<String>,
    price: f64,
    change_pct: Option<f64>,
    currency: String,
    series: Vec<f64>,
}

/// Watchlist feed: headline = first quote, rows = all of them.
fn watchlist(quotes: &[Quote]) -> Feed {
    let first = &quotes[0];
    let mut f = Feed {
        value: Some(first.price),
        currency: Some(first.currency.clone()),
        caption: first.name.clone().unwrap_or_else(|| first.symbol.clone()),
        delta_pct: first.change_pct,
        series: first.series.clone(),
        rows: quotes.iter().map(|q| FeedRow { label: q.symbol.clone(), value: price_text(q.price, &q.currency), frac: None, delta: q.change_pct.map(|d| (d * 100.0).round() / 100.0) }).collect(),
        ..Default::default()
    };
    f.extra.insert("symbol".into(), first.symbol.clone());
    if let Some(n) = &first.name {
        f.extra.insert("name".into(), n.clone());
    }
    f
}

// ---------------------------------------------------------------- Stocks

pub fn stocks(p: &Params) -> Result<Feed, FeedError> {
    let syms = list(p, "symbols", "AAPL,MSFT,NVDA,TSLA");
    let mut quotes = Vec::new();
    let mut last_err = None;
    for s in &syms {
        let s = s.to_uppercase();
        let path = format!("/v8/finance/chart/{}?range=1d&interval=5m", http::encode(&s));
        let mut got = None;
        for host in ["query1", "query2"] {
            match get_json(&format!("https://{host}.finance.yahoo.com{path}"), &[("Accept", "application/json")], "Yahoo Finance") {
                Ok(v) => {
                    got = parse_yahoo(&v);
                    if got.is_some() {
                        break;
                    }
                }
                Err(e) => last_err = Some(e),
            }
        }
        match got {
            Some(q) => quotes.push(q),
            None if quotes.is_empty() && last_err.is_none() => last_err = Some(FeedError::Failed(format!("Yahoo Finance: no data for {s}"))),
            None => {}
        }
    }
    if quotes.is_empty() {
        return Err(last_err.unwrap_or_else(|| FeedError::Failed("Yahoo Finance: no data".into())));
    }
    Ok(watchlist(&quotes))
}

/// chart.result[0] → quote. Day change is against the previous close.
fn parse_yahoo(v: &Value) -> Option<Quote> {
    let r = &v["chart"]["result"][0];
    let m = &r["meta"];
    let closes: Vec<f64> = r["indicators"]["quote"][0]["close"].as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
    let price = m["regularMarketPrice"].as_f64().or(closes.last().copied())?;
    let prev = m["chartPreviousClose"].as_f64().or(m["previousClose"].as_f64());
    let name = m["shortName"].as_str().or(m["longName"].as_str()).map(String::from);
    Some(Quote {
        symbol: m["symbol"].as_str().unwrap_or("?").to_string(),
        name,
        price,
        change_pct: prev.and_then(|p| pct(price, p)),
        currency: m["currency"].as_str().unwrap_or("USD").to_uppercase(),
        series: closes,
    })
}

// ---------------------------------------------------------------- Crypto

pub fn crypto(p: &Params) -> Result<Feed, FeedError> {
    let coins: Vec<String> = list(p, "coins", "bitcoin,ethereum,solana").into_iter().map(|c| c.to_lowercase()).collect();
    let ids = coins.join(",");
    let h = [("Accept", "application/json")];
    let v = get_json(&format!("https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&ids={}&price_change_percentage=24h", http::encode(&ids)), &h, "CoinGecko")?;
    let mut quotes = parse_coingecko_markets(&v, &coins);
    if quotes.is_empty() {
        return Err(FeedError::NeedsSetup("CoinGecko: none of those coin IDs were found (use IDs like bitcoin, ethereum)".into()));
    }
    let first_id = coins.iter().find(|c| v.as_array().is_some_and(|a| a.iter().any(|x| x["id"].as_str() == Some(c.as_str())))).cloned().unwrap_or_default();
    if let Ok(ch) = get_json(&format!("https://api.coingecko.com/api/v3/coins/{}/market_chart?vs_currency=usd&days=1", http::encode(&first_id)), &h, "CoinGecko") {
        quotes[0].series = parse_coingecko_chart(&ch);
    }
    Ok(watchlist(&quotes))
}

/// /coins/markets → quotes in the order the user listed the coins.
fn parse_coingecko_markets(v: &Value, order: &[String]) -> Vec<Quote> {
    let arr = v.as_array().cloned().unwrap_or_default();
    order
        .iter()
        .filter_map(|id| {
            let c = arr.iter().find(|c| c["id"].as_str() == Some(id.as_str()))?;
            Some(Quote {
                symbol: c["symbol"].as_str().unwrap_or(id).to_uppercase(),
                name: c["name"].as_str().map(String::from),
                price: num(&c["current_price"])?,
                change_pct: num(&c["price_change_percentage_24h"]),
                currency: "USD".into(),
                series: Vec::new(),
            })
        })
        .collect()
}

/// market_chart `prices: [[ms, price], …]` → prices.
fn parse_coingecko_chart(v: &Value) -> Vec<f64> {
    v["prices"].as_array().into_iter().flatten().filter_map(|p| p[1].as_f64()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yahoo_chart() {
        let v: Value = serde_json::from_str(
            r#"{"chart":{"result":[{"meta":{"currency":"USD","symbol":"AAPL","regularMarketPrice":202.0,"chartPreviousClose":200.0,"shortName":"Apple Inc."},
              "timestamp":[1,2,3],"indicators":{"quote":[{"close":[200.5,null,202.0]}]}}],"error":null}}"#,
        )
        .unwrap();
        let q = parse_yahoo(&v).unwrap();
        assert_eq!(q.price, 202.0);
        assert_eq!(q.change_pct, Some(1.0));
        assert_eq!(q.series, vec![200.5, 202.0]);
        let f = watchlist(&[q.clone(), Quote { symbol: "MSFT".into(), price: 410.25, change_pct: Some(-0.5), ..q }]);
        assert_eq!(f.value, Some(202.0));
        assert_eq!(f.extra["symbol"], "AAPL");
        assert_eq!(f.extra["name"], "Apple Inc.");
        assert_eq!(f.rows[1].value, "$410.25");
        assert_eq!(f.rows[1].delta, Some(-0.5));
        assert!(parse_yahoo(&serde_json::from_str(r#"{"chart":{"result":null,"error":{"code":"Not Found"}}}"#).unwrap()).is_none());
    }
    #[test]
    fn coingecko() {
        let v: Value = serde_json::from_str(
            r#"[{"id":"ethereum","symbol":"eth","name":"Ethereum","current_price":3000.5,"price_change_percentage_24h":-1.25},
                {"id":"bitcoin","symbol":"btc","name":"Bitcoin","current_price":90000,"price_change_percentage_24h":2.5}]"#,
        )
        .unwrap();
        let q = parse_coingecko_markets(&v, &["bitcoin".into(), "nope".into(), "ethereum".into()]);
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].symbol, "BTC");
        assert_eq!(q[1].change_pct, Some(-1.25));
        let ch: Value = serde_json::from_str(r#"{"prices":[[1,89000.0],[2,90000.0]],"market_caps":[],"total_volumes":[]}"#).unwrap();
        assert_eq!(parse_coingecko_chart(&ch), vec![89000.0, 90000.0]);
        assert_eq!(price_text(0.000123, "usd"), "$0.000123");
        assert_eq!(price_text(90000.0, "USD"), "$90,000.00");
    }
}
