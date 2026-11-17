//! General HTTPS requests over WinHTTP (the OS TLS stack and proxy settings; no extra
//! dependencies). Used by the widget connectors. Blocking: call from worker threads.
//!
//! Every request has 8 s resolve/connect/send/receive timeouts and a 16 MB body cap.

use crate::util::wide;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::*;

/// A response: status code and body (returned for every status, not just 200).
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Splits "https://host[:port]/path?query" into (host, port, path). Only https.
pub fn split_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("https://")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (hostport, 443),
    };
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port, path.to_string()))
}

/// Performs an HTTPS request. `headers` are sent as-is ("Name", "value").
/// Returns Err with a short reason for network failures (not for HTTP error statuses).
pub fn request(method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>) -> Result<Response, String> {
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
    let (host, port, path) = split_url(url).ok_or_else(|| format!("bad url: {url}"))?;
    let agent = wide("LaminarDock/0.2 (+https://github.com/davesousa/Laminar_Dock)");
    let whost = wide(&host);
    let wpath = wide(&path);
    let verb = wide(method);
    let mut hdr = String::new();
    for (k, v) in headers {
        hdr.push_str(k);
        hdr.push_str(": ");
        hdr.push_str(v);
        hdr.push_str("\r\n");
    }
    let whdr: Vec<u16> = hdr.encode_utf16().collect();
    unsafe {
        let session = H(WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err("network unavailable".into());
        }
        let _ = WinHttpSetTimeouts(session.0, 8000, 8000, 8000, 8000);
        let connect = H(WinHttpConnect(session.0, PCWSTR(whost.as_ptr()), port, 0));
        if connect.0.is_null() {
            return Err(format!("can't reach {host}"));
        }
        let req = H(WinHttpOpenRequest(connect.0, PCWSTR(verb.as_ptr()), PCWSTR(wpath.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE));
        if req.0.is_null() {
            return Err("request failed".into());
        }
        let (bptr, blen) = match body {
            Some(b) => (Some(b.as_ptr() as *const core::ffi::c_void), b.len() as u32),
            None => (None, 0),
        };
        WinHttpSendRequest(req.0, if whdr.is_empty() { None } else { Some(&whdr) }, bptr, blen, blen, 0).map_err(|e| format!("can't reach {host} ({})", e.code().0 & 0xFFFF))?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|_| format!("no answer from {host}"))?;
        let mut status = 0u32;
        let mut len = 4u32;
        let mut idx = 0u32;
        let _ = WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut u32 as *mut _), &mut len, &mut idx);
        let mut out = Vec::new();
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let mut read = 0u32;
            if WinHttpReadData(req.0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).is_err() || read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
            if out.len() > 16 * 1024 * 1024 {
                return Err("response too large".into());
            }
        }
        Ok(Response { status: status as u16, body: out })
    }
}

/// GET with headers.
pub fn get(url: &str, headers: &[(&str, &str)]) -> Result<Response, String> {
    request("GET", url, headers, None)
}

/// POST a JSON body (adds Content-Type).
pub fn post_json(url: &str, headers: &[(&str, &str)], body: &serde_json::Value) -> Result<Response, String> {
    let mut h: Vec<(&str, &str)> = headers.to_vec();
    h.push(("Content-Type", "application/json"));
    let b = serde_json::to_vec(body).unwrap_or_default();
    request("POST", url, &h, Some(&b))
}

/// POST an application/x-www-form-urlencoded body built from pairs.
pub fn post_form(url: &str, headers: &[(&str, &str)], form: &[(&str, &str)]) -> Result<Response, String> {
    let mut h: Vec<(&str, &str)> = headers.to_vec();
    h.push(("Content-Type", "application/x-www-form-urlencoded"));
    let b: Vec<String> = form.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect();
    request("POST", url, &h, Some(b.join("&").as_bytes()))
}

/// Percent-encodes a query-string component.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Standard base64 (for HTTP Basic auth).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url() {
        assert_eq!(split_url("https://api.x.com/2/users?a=1"), Some(("api.x.com".into(), 443, "/2/users?a=1".into())));
        assert_eq!(split_url("https://h:8443"), Some(("h".into(), 8443, "/".into())));
        assert_eq!(split_url("http://h/"), None);
    }
    #[test]
    fn b64() {
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"ab"), "YWI=");
    }
}
