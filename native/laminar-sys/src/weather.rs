//! Weather from Open-Meteo (free, no API key) over HTTPS.
//!
//! HTTP is done with Win32 WinHTTP (`WinHttpOpen/Connect/OpenRequest/SendRequest/
//! ReceiveResponse/ReadData`) so the crate needs no HTTP/TLS dependency; WinHTTP uses
//! the OS TLS stack (Schannel) and the system/auto proxy. Each request is bounded by
//! 5 s resolve/connect/send/receive timeouts. Typical latency: 150-600 ms. No COM needed.
//! Default location used by callers: Toronto (43.65, -79.38).

use crate::util::wide;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::*;

/// Current conditions + today's range + the next 6 hours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    pub temp_c: f32,
    /// WMO weather code (see [`describe`]).
    pub code: i32,
    /// Short English description of `code`, e.g. "Partly cloudy".
    pub description: String,
    pub high_c: f32,
    pub low_c: f32,
    /// Next 6 full hours after now (local time of the location): (hour 0-23, temp °C, WMO code).
    pub hourly: Vec<(u8, f32, i32)>,
}

/// Default location (Toronto).
pub const DEFAULT_LAT_LON: (f32, f32) = (43.65, -79.38);

/// Performs a blocking HTTPS GET to `host` + `path` (path includes the query string).
/// Returns the body when the status is 200.
pub fn https_get(host: &str, path: &str) -> Option<Vec<u8>> {
    struct H(*mut core::ffi::c_void);
    impl Drop for H {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }
    let agent = wide("LaminarDock/0.1");
    let whost = wide(host);
    let wpath = wide(path);
    let verb = wide("GET");
    unsafe {
        let session = H(WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000).ok()?;
        let connect = H(WinHttpConnect(session.0, PCWSTR(whost.as_ptr()), 443, 0));
        if connect.0.is_null() {
            return None;
        }
        let req = H(WinHttpOpenRequest(connect.0, PCWSTR(verb.as_ptr()), PCWSTR(wpath.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE));
        if req.0.is_null() {
            return None;
        }
        WinHttpSendRequest(req.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).ok()?;
        let mut status = 0u32;
        let mut len = 4u32;
        let mut idx = 0u32;
        WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut u32 as *mut _), &mut len, &mut idx).ok()?;
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(req.0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
            if body.len() > 8 * 1024 * 1024 {
                return None;
            }
        }
        Some(body)
    }
}

fn get_json(host: &str, path: &str) -> Option<Value> {
    serde_json::from_slice(&https_get(host, path)?).ok()
}

/// Fetches current weather, today's high/low and the next 6 hours for a location.
pub fn fetch(lat: f32, lon: f32) -> Option<Weather> {
    let path = format!(
        "/v1/forecast?latitude={lat:.4}&longitude={lon:.4}&current=temperature_2m,weather_code&hourly=temperature_2m,weather_code&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=2"
    );
    parse_forecast(&get_json("api.open-meteo.com", &path)?)
}

fn parse_forecast(v: &Value) -> Option<Weather> {
    let cur = &v["current"];
    let temp_c = cur["temperature_2m"].as_f64()? as f32;
    let code = cur["weather_code"].as_i64().unwrap_or(-1) as i32;
    let now = cur["time"].as_str().unwrap_or("");
    let d = &v["daily"];
    let high_c = d["temperature_2m_max"][0].as_f64().unwrap_or(temp_c as f64) as f32;
    let low_c = d["temperature_2m_min"][0].as_f64().unwrap_or(temp_c as f64) as f32;
    let h = &v["hourly"];
    let mut hourly = Vec::with_capacity(6);
    if let (Some(times), Some(temps), Some(codes)) = (h["time"].as_array(), h["temperature_2m"].as_array(), h["weather_code"].as_array()) {
        // ISO-8601 local times ("2026-11-08T14:00") compare correctly as strings.
        for (i, t) in times.iter().enumerate() {
            let Some(t) = t.as_str() else { continue };
            if t <= now {
                continue;
            }
            let hour = t.get(11..13).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
            let temp = temps.get(i).and_then(|x| x.as_f64()).unwrap_or(f64::NAN) as f32;
            let c = codes.get(i).and_then(|x| x.as_i64()).unwrap_or(-1) as i32;
            hourly.push((hour, temp, c));
            if hourly.len() == 6 {
                break;
            }
        }
    }
    Some(Weather { temp_c, code, description: describe(code).to_string(), high_c, low_c, hourly })
}

/// Resolves a place name to (lat, lon, "Name, Region, Country") via Open-Meteo geocoding.
pub fn geocode(city: &str) -> Option<(f32, f32, String)> {
    let path = format!("/v1/search?name={}&count=1&language=en&format=json", url_encode(city.trim()));
    let v = get_json("geocoding-api.open-meteo.com", &path)?;
    let r = &v["results"][0];
    let lat = r["latitude"].as_f64()? as f32;
    let lon = r["longitude"].as_f64()? as f32;
    let parts: Vec<&str> = ["name", "admin1", "country"].iter().filter_map(|k| r[*k].as_str()).filter(|s| !s.is_empty()).collect();
    let mut label: Vec<&str> = Vec::new();
    for p in parts {
        if !label.contains(&p) {
            label.push(p);
        }
    }
    Some((lat, lon, label.join(", ")))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Short English description of a WMO weather interpretation code.
pub fn describe(code: i32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 | 81 => "Showers",
        82 => "Heavy showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm, hail",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse() {
        let v: Value = serde_json::from_str(
            r#"{"current":{"time":"2026-11-08T14:15","temperature_2m":5.2,"weather_code":2},
            "hourly":{"time":["2026-11-08T13:00","2026-11-08T14:00","2026-11-08T15:00","2026-11-08T16:00","2026-11-08T17:00","2026-11-08T18:00","2026-11-08T19:00","2026-11-08T20:00","2026-11-08T21:00"],
            "temperature_2m":[1,2,3,4,5,6,7,8,9],"weather_code":[0,0,1,2,3,45,61,71,95]},
            "daily":{"temperature_2m_max":[8.1,9.0],"temperature_2m_min":[-1.0,0.5]}}"#,
        )
        .unwrap();
        let w = parse_forecast(&v).unwrap();
        assert_eq!(w.description, "Partly cloudy");
        assert_eq!(w.hourly.len(), 6);
        assert_eq!(w.hourly[0], (15, 3.0, 1));
        assert_eq!(w.hourly[5].0, 20);
        assert_eq!((w.high_c, w.low_c), (8.1, -1.0));
    }
    #[test]
    fn encode() {
        assert_eq!(url_encode("São Paulo"), "S%C3%A3o%20Paulo");
    }
}
