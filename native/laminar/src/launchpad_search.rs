//! Launchpad search: one ranked list across apps, open windows, commands, Windows settings
//! (Windows' own index of ~1,500 settings and Control Panel tasks), files and the web, plus
//! instant answers (calculator, unit conversion, world time, weather) and web shortcuts
//! ("g cats", "yt lo-fi"). A child module of `launchpad`, so it shares its private state.
//!
//! Ranking: every candidate gets a score (see `laminar_sys::search`), plus what the user
//! picked before for this query (`laminar_sys::history`). The best one is the Top Hit
//! (an instant answer always is); the rest are grouped into sections, and sections are
//! ordered by their best result, so the most relevant kind of thing comes first.

use super::*;
use laminar_sys::search::{fuzzy_score, score_named, whole_word};

/// System commands that need a second Enter (or click) before they run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SysCmd {
    Sleep,
    SignOut,
    Restart,
    ShutDown,
    EmptyBin,
}

/// A window the user can switch to.
pub struct WinRow {
    pub hwnd: isize,
    pub title: String,
    /// Lower-case app identity (as the dock keys it).
    pub app: String,
}

#[derive(Clone)]
pub(super) enum RIcon {
    App(String),
    Glyph(&'static str, (f32, f32, f32, f32)),
    Emoji(String),
}

#[derive(Clone)]
pub(super) struct Row {
    pub section: &'static str,
    pub title: String,
    pub sub: String,
    pub icon: RIcon,
    /// Shown at the right: a toggle's state ("On"/"Off").
    pub accessory: Option<String>,
    /// What Enter does, shown on the selected row ("Open", "Copy", "Switch").
    pub verb: &'static str,
    pub act: Act,
    /// Stable key for learning what the user picks.
    pub key: Option<String>,
    pub score: i32,
    /// An instant answer (always the Top Hit, drawn larger).
    pub answer: bool,
}

// Segoe Fluent Icons.
const G_CALC: &str = "\u{E8EF}";
const G_CLOCK: &str = "\u{E823}";
const G_SWITCH: &str = "\u{E8AB}";
const G_WIFI: &str = "\u{E701}";
const G_BT: &str = "\u{E702}";
const G_THEME: &str = "\u{E793}";
const G_MUTE: &str = "\u{E74F}";
const G_VOLUME: &str = "\u{E767}";
const G_PLAY: &str = "\u{E768}";
const G_PAUSE: &str = "\u{E769}";
const G_NEXT: &str = "\u{E893}";
const G_PREV: &str = "\u{E892}";
const G_SLEEP: &str = "\u{E708}";
const G_RESTART: &str = "\u{E777}";
const G_SIGNOUT: &str = "\u{E748}";
const G_TRASH: &str = "\u{E74D}";
const G_CAMERA: &str = "\u{E722}";
const G_TASKVIEW: &str = "\u{E7C4}";
const G_HISTORY: &str = "\u{E81C}";
const G_LINK: &str = "\u{E71B}";
const G_CONTROL: &str = "\u{E7F8}";
const G_SHIELD: &str = "\u{EA18}";
const G_DOWNLOAD: &str = "\u{E896}";

/// Common words and the Settings page people mean by them (Windows' own synonyms point
/// elsewhere first: "uninstall" would open "Uninstall updates").
const INTENTS: &[(&str, &str)] = &[
    ("uninstall", "ms-settings:installed-apps"),
    ("remove app", "ms-settings:installed-apps"),
    ("programs", "ms-settings:installed-apps"),
    ("background", "ms-settings:personalization-background"),
    ("brightness", "ms-settings:display"),
    ("monitor", "ms-settings:display"),
    ("sound", "ms-settings:sound"),
    ("audio", "ms-settings:sound"),
    ("speakers", "ms-settings:sound"),
    ("microphone", "ms-settings:sound"),
    ("battery", "ms-settings:powersleep"),
    ("power", "ms-settings:powersleep"),
    ("internet", "ms-settings:network-status"),
    ("updates", "ms-settings:windowsupdate"),
    ("update", "ms-settings:windowsupdate"),
    ("keyboard", "ms-settings:keyboard"),
    ("language", "ms-settings:regionlanguage"),
    ("notifications", "ms-settings:notifications"),
    ("storage", "ms-settings:storagesense"),
    ("disk space", "ms-settings:storagesense"),
    ("default browser", "ms-settings:defaultapps"),
    ("default apps", "ms-settings:defaultapps"),
    ("password", "ms-settings:signinoptions"),
    ("fingerprint", "ms-settings:signinoptions"),
    ("taskbar", "ms-settings:taskbar"),
    ("fonts", "ms-settings:fonts"),
    ("time zone", "ms-settings:dateandtime"),
    ("camera", "ms-settings:camera"),
    ("webcam", "ms-settings:camera"),
];

/// Rows per section after the Top Hit.
fn section_limit(s: &str) -> usize {
    match s {
        "Apps" => 6,
        "Settings" | "Files" => 5,
        "Commands" | "Windows" => 3,
        _ => 4,
    }
}

/// Web shortcuts: prefix → (engine name, URL before the query).
const ENGINES: &[(&[&str], &str, &str)] = &[
    (&["g", "google"], "Google", "https://www.google.com/search?q="),
    (&["yt", "youtube"], "YouTube", "https://www.youtube.com/results?search_query="),
    (&["gh", "github"], "GitHub", "https://github.com/search?q="),
    (&["w", "wiki", "wikipedia"], "Wikipedia", "https://en.wikipedia.org/w/index.php?search="),
    (&["maps", "map"], "Google Maps", "https://www.google.com/maps/search/"),
    (&["ddg", "duck"], "DuckDuckGo", "https://duckduckgo.com/?q="),
    (&["b", "bing"], "Bing", "https://www.bing.com/search?q="),
    (&["amazon", "amz"], "Amazon", "https://www.amazon.ca/s?k="),
];

pub fn url_encode(q: &str) -> String {
    q.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// "g cats" → (engine, query, url).
pub fn web_shortcut(q: &str) -> Option<(&'static str, String, String)> {
    let (pre, rest) = q.trim_start().split_once(' ')?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let pre = pre.to_lowercase();
    let (_, name, base) = ENGINES.iter().find(|(p, _, _)| p.contains(&pre.as_str()))?;
    Some((name, rest.to_string(), format!("{base}{}", url_encode(rest))))
}

/// "github.com", "https://x.y/z", "localhost:3000" → the URL to open.
pub fn as_url(q: &str) -> Option<String> {
    let q = q.trim();
    if q.contains(char::is_whitespace) || q.len() < 4 {
        return None;
    }
    let lower = q.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(q.to_string());
    }
    if lower.starts_with("localhost:") || lower.starts_with("127.0.0.1") {
        return Some(format!("http://{q}"));
    }
    let host = lower.split(['/', '?', '#']).next()?;
    let host = host.split(':').next()?;
    let parts: Vec<&str> = host.split('.').collect();
    let tld = parts.last()?;
    let ok = parts.len() >= 2
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && (2..=12).contains(&tld.len())
        && tld.chars().all(|c| c.is_ascii_alphabetic())
        // File names look like domains ("notes.txt"); only common web endings count.
        && ["com", "org", "net", "io", "dev", "ca", "co", "uk", "app", "ai", "edu", "gov", "me", "tv", "us", "de", "fr", "info", "xyz", "so", "gg", "sh", "ly"].contains(tld);
    ok.then(|| format!("https://{q}"))
}

/// "volume 40", "vol 40%", "set volume to 40" → 40.
pub fn volume_query(q: &str) -> Option<u8> {
    let q = q.trim().to_lowercase();
    let rest = ["set volume to ", "set volume ", "volume to ", "volume ", "vol "].iter().find_map(|p| q.strip_prefix(p))?;
    let n: u32 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
    (n <= 100).then_some(n as u8)
}

fn weather_emoji(code: i32, day: bool) -> &'static str {
    match code {
        0 => {
            if day {
                "☀️"
            } else {
                "🌙"
            }
        }
        1 | 2 => {
            if day {
                "🌤️"
            } else {
                "☁️"
            }
        }
        3 => "☁️",
        45 | 48 => "🌫️",
        51..=67 => "🌧️",
        71..=77 | 85 | 86 => "🌨️",
        80..=82 => "🌦️",
        95..=99 => "⛈️",
        _ => "🌡️",
    }
}

fn clock_text(h: u16, m: u16) -> String {
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {}", if h < 12 { "AM" } else { "PM" })
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `c:\program files\x\notepad++.exe` → "Notepad++"; an app id → its last part.
fn exe_label(app: &str) -> String {
    let base = app.rsplit(['\\', '!']).next().unwrap_or(app).trim_end_matches(".exe");
    let mut c = base.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => app.to_string(),
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn row(section: &'static str, title: String, sub: String, icon: RIcon, verb: &'static str, act: Act, key: Option<String>, score: i32) -> Row {
    Row { section, title, sub, icon, accessory: None, verb, act, key, score, answer: false }
}

/// Instant answers for the query, best first (at most one is shown).
fn answers(q: &str, d: &Data) -> Vec<Row> {
    let mut out = Vec::new();
    let ink = theme::text_2();
    if let Some((engine, text, url)) = web_shortcut(q) {
        let mut r = row("Web", format!("Search {engine} for \u{201c}{text}\u{201d}"), "Opens your browser".into(), RIcon::Glyph(G_GLOBE, ink), "Search", Act::Uri(url), None, 0);
        r.answer = true;
        out.push(r);
    }
    if let Some(url) = as_url(q) {
        let shown = url.trim_start_matches("https://").trim_start_matches("http://").to_string();
        let mut r = row("Web", format!("Open {shown}"), "Opens your browser".into(), RIcon::Glyph(G_LINK, ink), "Open", Act::Uri(url), None, 0);
        r.answer = true;
        out.push(r);
    }
    if let Some(v) = volume_query(q) {
        let mut r = row("Commands", format!("Set Volume to {v}%"), "Sound".into(), RIcon::Glyph(G_VOLUME, ink), "Set", Act::SetVolume(v), None, 0);
        r.answer = true;
        out.push(r);
    }
    if let Some(a) = laminar_sys::calc::answer(q) {
        let conv = laminar_sys::calc::convert(q).is_some();
        let mut r = row(if conv { "Conversion" } else { "Calculator" }, a.value.clone(), a.detail.clone(), RIcon::Glyph(if conv { G_SWITCH } else { G_CALC }, ink), "Copy", Act::Copy(a.copy.clone()), None, 0);
        r.answer = true;
        out.push(r);
    }
    if let Some(tq) = laminar_sys::worldtime::parse_query(q) {
        let local = laminar_sys::worldtime::local_now();
        let found = match tq {
            laminar_sys::worldtime::TimeQuery::Local => Some(("Here".to_string(), local)),
            laminar_sys::worldtime::TimeQuery::Place(p) => {
                let mine = crate::widgets::world_cities(d.cfg);
                laminar_sys::worldtime::find(&p, laminar_sys::worldtime::zones(), &mine).and_then(|(label, key)| laminar_sys::worldtime::now_in(&key).map(|t| (label, t)))
            }
        };
        if let Some((label, t)) = found {
            let time = clock_text(t.wHour, t.wMinute);
            let day = format!("{}, {} {}", DAYS[(t.wDayOfWeek as usize) % 7], MONTHS[(t.wMonth as usize).clamp(1, 12) - 1], t.wDay);
            let diff = laminar_sys::worldtime::minutes_ahead(&t, &local);
            let rel = if label == "Here" {
                String::new()
            } else if diff == 0 {
                " · Same time as here".into()
            } else {
                let (h, m) = (diff.abs() / 60, diff.abs() % 60);
                let span = if m == 0 { format!("{h} hour{}", if h == 1 { "" } else { "s" }) } else { format!("{h}h {m}m") };
                format!(" · {span} {}", if diff > 0 { "ahead" } else { "behind" })
            };
            let place = if label == "Here" { "Local time".to_string() } else { label };
            let mut r = row("World Clock", time.clone(), format!("{place} · {day}{rel}"), RIcon::Glyph(G_CLOCK, ink), "Copy", Act::Copy(time), None, 0);
            r.answer = true;
            out.push(r);
        }
    }
    let ql = q.trim().to_lowercase();
    if ["weather", "forecast", "weather today", "weather now", "temperature", "temp outside"].contains(&ql.as_str()) {
        if let Some(w) = d.weather {
            let city = d.cfg.widget_settings.get("weather").and_then(|m| m.get("city")).cloned().unwrap_or_else(|| "Toronto".into());
            let title = format!("{:.0}° {}", w.temp_c, w.description);
            let sub = format!("{city} · H {:.0}°  L {:.0}° · Feels like {:.0}°", w.high_c, w.low_c, w.feels_c);
            let mut r = row("Weather", title, sub, RIcon::Emoji(weather_emoji(w.code, w.is_day).into()), "Forecast", Act::Uri(format!("https://www.bing.com/search?q={}", url_encode(&format!("weather {city}")))), None, 0);
            r.answer = true;
            out.push(r);
        }
    }
    out
}

/// System commands (with live state for toggles).
fn commands(q: &str, d: &Data, armed_cmd: Option<SysCmd>) -> Vec<Row> {
    let ink = theme::text_2();
    let ctl = d.control;
    let mut v: Vec<(String, String, &'static str, Act, &'static [&'static str], Option<bool>, &'static str)> = Vec::new();
    if let Some(on) = ctl.wifi {
        v.push((if on { "Turn Wi-Fi Off" } else { "Turn Wi-Fi On" }.into(), format!("Wi-Fi is {}", if on { "on" } else { "off" }), G_WIFI, Act::Wifi, &["wifi", "wi-fi", "wireless", "internet", "network"], Some(on), "wifi"));
    }
    if let Some(on) = ctl.bluetooth {
        v.push((if on { "Turn Bluetooth Off" } else { "Turn Bluetooth On" }.into(), format!("Bluetooth is {}", if on { "on" } else { "off" }), G_BT, Act::Bluetooth, &["bluetooth", "bt", "headphones", "airpods"], Some(on), "bluetooth"));
    }
    let dark = ctl.dark;
    v.push((if dark { "Switch to Light Mode" } else { "Switch to Dark Mode" }.into(), format!("Windows is in {} mode", if dark { "dark" } else { "light" }), G_THEME, Act::Dark, &["dark mode", "light mode", "dark", "light", "theme", "appearance", "night mode"], Some(dark), "dark"));
    if let Some(m) = ctl.muted {
        let vol = ctl.volume.map(|v| format!(" · Volume {:.0}%", v * 100.0)).unwrap_or_default();
        v.push((if m { "Unmute" } else { "Mute" }.into(), format!("Sound is {}{vol}", if m { "muted" } else { "on" }), if m { G_VOLUME } else { G_MUTE }, Act::Mute, &["mute", "unmute", "sound", "audio", "silence", "volume", "quiet"], Some(m), "mute"));
    }
    if let Some(media) = &ctl.media {
        let what = if media.title.is_empty() { "Media".to_string() } else { media.title.clone() };
        let playing = media.playing;
        v.push((if playing { "Pause" } else { "Play" }.into(), what.clone(), if playing { G_PAUSE } else { G_PLAY }, Act::MediaPlay, &["play", "pause", "music", "media", "resume", "song"], None, "playpause"));
        v.push(("Next Track".into(), what.clone(), G_NEXT, Act::MediaNext, &["next", "skip", "next song", "track"], None, "next"));
        v.push(("Previous Track".into(), what, G_PREV, Act::MediaPrev, &["previous", "back", "previous song", "track"], None, "prev"));
    }
    v.push(("Lock Screen".into(), "Lock this PC".into(), G_LOCK, Act::Lock, &["lock", "lock pc", "lock computer", "away"], None, "lock"));
    v.push(("Take a Screenshot".into(), "Snipping Tool".into(), G_CAMERA, Act::Snip, &["screenshot", "snip", "screen capture", "capture", "print screen", "snipping tool"], None, "snip"));
    v.push(("Empty Recycle Bin".into(), "Windows asks before deleting".into(), G_TRASH, Act::Sys(SysCmd::EmptyBin), &["empty trash", "trash", "recycle bin", "empty bin", "bin"], None, "emptybin"));
    // These ask for a second Enter (or click) before they run, and search doesn't learn
    // them, so a habit can't turn one key into a restart.
    for (cmd, name, sub, g, kw, key) in [
        (SysCmd::Sleep, "Sleep", "Put this PC to sleep", G_SLEEP, &["sleep", "suspend", "standby"][..], "sleep"),
        (SysCmd::SignOut, "Sign Out", "Close your apps and sign out", G_SIGNOUT, &["sign out", "log out", "logout", "log off", "signout"][..], "signout"),
        (SysCmd::Restart, "Restart", "Close your apps and restart", G_RESTART, &["restart", "reboot"][..], "restart"),
        (SysCmd::ShutDown, "Shut Down", "Close your apps and turn off", G_POWER, &["shut down", "shutdown", "power off", "turn off"][..], "shutdown"),
    ] {
        let armed = armed_cmd == Some(cmd);
        let title = if armed { format!("Press Enter again to {}", name.to_lowercase()) } else { name.to_string() };
        v.push((title, sub.into(), g, if armed { Act::Sys(cmd) } else { Act::Arm(cmd) }, kw, None, key));
    }
    v.push(("Check for Laminar Updates".into(), format!("Laminar Dock {}", d.app_version), G_DOWNLOAD, Act::CheckUpdates, &["update laminar", "laminar update", "check for updates"], None, "laminarupdate"));
    if !d.history.is_empty() {
        v.push(("Clear Search History".into(), "Forget what you picked in search".into(), G_HISTORY, Act::ClearHistory, &["clear history", "forget searches", "search history"], None, "clearhistory"));
    }
    let mut out = Vec::new();
    for (title, sub, g, act, kw, state, key) in v {
        let kws: Vec<String> = kw.iter().map(|s| s.to_string()).collect();
        // Armed rows keep matching by their plain name.
        let name_for_match = match &act {
            Act::Sys(c) | Act::Arm(c) if *c != SysCmd::EmptyBin => match c {
                SysCmd::Sleep => "Sleep".to_string(),
                SysCmd::SignOut => "Sign Out".to_string(),
                SysCmd::Restart => "Restart".to_string(),
                _ => "Shut Down".to_string(),
            },
            _ => title.clone(),
        };
        let learn = !matches!(&act, Act::Sys(c) | Act::Arm(c) if *c != SysCmd::EmptyBin);
        if let Some(mut s) = score_named(q, &name_for_match, &kws) {
            // Typing exactly what a command is about ("bluetooth", "lock") makes it the Top
            // Hit: acting is usually what's wanted; the settings follow right below.
            if kws.contains(&q.trim().to_lowercase()) {
                s += 1700;
            }
            let col = if state == Some(true) { theme::accent_f() } else { ink };
            // A command waiting for its second press says so on the Enter hint.
            let verb = if matches!(&act, Act::Sys(c) if *c != SysCmd::EmptyBin) { "Confirm" } else { "Run" };
            let mut r = row("Commands", title, sub, RIcon::Glyph(g, col), verb, act, learn.then(|| format!("cmd:{key}")), s + 100);
            r.accessory = state.map(|on| if on { "On".into() } else { "Off".into() });
            out.push(r);
        }
    }
    out
}

/// Windows' settings and Control Panel tasks (the index), plus Laminar's own settings.
fn settings_rows(q: &str) -> Vec<Row> {
    let ql = q.trim().to_lowercase();
    let first = ql.split_whitespace().next().unwrap_or("").to_string();
    let mut out: Vec<Row> = Vec::new();
    match laminar_sys::settings_index::get().filter(|v| !v.is_empty()).zip(laminar_sys::settings_index::hay()) {
        Some((index, hay)) => {
            for (e, h) in index.iter().zip(hay) {
                // Cheap pre-check: every match needs the first word somewhere in it.
                if !h.contains(&first) {
                    continue;
                }
                let Some(mut s) = score_named(q, &e.name, &e.keywords) else { continue };
                if e.page {
                    s += 150;
                }
                if e.area == "Control Panel" {
                    s -= 100;
                }
                if e.page && INTENTS.iter().any(|(w, t)| *w == ql && *t == e.target) {
                    s += 1600;
                }
                let (g, col) = match e.area.as_str() {
                    "Control Panel" => (G_CONTROL, theme::text_2()),
                    "Windows Security" => (G_SHIELD, theme::text_2()),
                    _ => (G_SETTINGS, theme::text_2()),
                };
                let icon = match &e.glyph {
                    Some(gl) => RIcon::Emoji(gl.clone()),
                    None => RIcon::Glyph(g, col),
                };
                let act = Act::Run(e.target.clone(), e.args.clone());
                out.push(row("Settings", e.name.clone(), e.area.clone(), icon, "Open", act, Some(format!("set:{}|{}", e.target, e.name.to_lowercase())), s));
            }
        }
        None => {
            for (name, uri) in WIN_SETTINGS {
                if let Some(s) = score_named(q, name, &[]) {
                    out.push(row("Settings", name.to_string(), "Windows Settings".into(), RIcon::Glyph(G_SETTINGS, theme::text_2()), "Open", Act::Run(uri.to_string(), String::new()), Some(format!("set:{uri}|{}", name.to_lowercase())), s));
                }
            }
        }
    }
    for (name, sec) in LAMINAR_SETTINGS {
        let short = &name["Laminar: ".len()..];
        let kw = vec!["laminar".to_string(), "dock".to_string()];
        if let Some(s) = score_named(q, short, &kw).or_else(|| score_named(q, name, &[])) {
            out.push(row("Settings", name.to_string(), "Laminar Settings".into(), RIcon::Glyph(G_SETTINGS, (0.55, 0.6, 1.0, 1.0)), "Open", Act::Section(*sec), Some(format!("lam:{sec}|{short}")), s));
        }
    }
    out
}

/// Every result for `q`, in display order (Top Hit first).
pub(super) fn build_results(q: &str, d: &Data, armed: Option<SysCmd>) -> Vec<Row> {
    let q = q.trim().to_string();
    if q.is_empty() {
        return Vec::new();
    }
    let ql = q.to_lowercase();
    let now = unix_now();
    let answers = answers(&q, d);
    let mut rows: Vec<Row> = Vec::new();

    // Apps: fuzzy rank, whole-word hits ("code" → Visual Studio Code before "Codec …"),
    // pinned apps on ties (genuine matches only).
    for (a, s) in laminar_sys::search::rank(&q, d.apps, 40) {
        let mut sc = s + 200;
        if a.name.to_lowercase().split(|ch: char| !ch.is_alphanumeric()).any(|w| w == ql) || whole_word(&ql, &a.name) {
            sc += 1500;
        }
        if d.pinned.contains(&a.id) && s > 500 {
            sc += 700;
        }
        rows.push(row("Apps", a.name.clone(), "Application".into(), RIcon::App(a.id.clone()), "Open", Act::App(a.id.clone()), Some(format!("app:{}", a.id)), sc));
    }
    rows.extend(commands(&q, d, armed));
    // Open windows, by title (and by app name, below the app itself).
    for w in d.windows {
        if w.title.trim().is_empty() {
            continue;
        }
        let app_name = d
            .names
            .get(&w.app)
            .cloned()
            .or_else(|| d.apps.iter().find(|a| a.id.to_lowercase() == w.app).map(|a| a.name.clone()))
            .unwrap_or_else(|| exe_label(&w.app));
        let by_title = fuzzy_score(&q, &w.title).filter(|&s| s > 500);
        let by_app = score_named(&q, &app_name, &[]).map(|s| s - 900);
        let Some(s) = by_title.max(by_app) else { continue };
        let icon = if d.brushes.contains_key(&w.app) { RIcon::App(w.app.clone()) } else { RIcon::Glyph(G_TASKVIEW, theme::text_2()) };
        rows.push(row("Windows", w.title.clone(), format!("Window · {app_name}"), icon, "Switch", Act::Window(w.hwnd), Some(format!("win:{}|{}", w.app, w.title.to_lowercase())), s - 100));
    }
    rows.extend(settings_rows(&q));
    // A sum is not a file name: Windows reads "12*7" as a wildcard and returns noise.
    let sum = answers.first().map(|a| a.section == "Calculator").unwrap_or(false);
    for f in d.files.iter().filter(|_| !sum) {
        let s = fuzzy_score(&q, &f.name).unwrap_or(1) - 300;
        let folder = std::path::Path::new(&f.path).parent().map(|p| p.display().to_string()).unwrap_or_default();
        rows.push(row("Files", f.name.clone(), folder, RIcon::Glyph(if f.is_folder { G_FOLDER } else { G_DOC }, theme::text_2()), "Open", Act::File(f.path.clone()), Some(format!("file:{}", f.path.to_lowercase())), s));
    }
    // What the user picked before for this query.
    for r in rows.iter_mut() {
        if let Some(k) = &r.key {
            r.score += d.history.boost(&q, k, now);
        }
    }
    rows.sort_by(|a, b| b.score.cmp(&a.score));
    // Settings pages repeat (one page holds many settings): at most two rows per link.
    let mut per_target: HashMap<String, usize> = HashMap::new();
    rows.retain(|r| match &r.act {
        Act::Run(t, a) => {
            let n = per_target.entry(format!("{t} {a}")).or_insert(0);
            *n += 1;
            *n <= 2
        }
        _ => true,
    });

    let mut out: Vec<Row> = Vec::new();
    let web_top = answers.first().map(|a| a.section == "Web").unwrap_or(false);
    // An app or window whose name starts with the whole query beats an answer: "google
    // chrome" opens Chrome (not a Google search), "clock" the Clock app, "paint.net" the app.
    let named = rows.first().map(|r| matches!(r.section, "Apps" | "Windows") && fuzzy_score(&q, &r.title).unwrap_or(0) > 2000).unwrap_or(false);
    let mut answer = answers.into_iter().next();
    if named {
        out.push(rows.remove(0));
        if let Some(mut a) = answer.take() {
            a.answer = false;
            out.push(a);
        }
    } else if let Some(a) = answer.take() {
        out.push(a);
    } else if !rows.is_empty() {
        out.push(rows.remove(0));
    }
    // Sections, ordered by their best result.
    let mut order: Vec<&'static str> = Vec::new();
    for r in &rows {
        if !order.contains(&r.section) {
            order.push(r.section);
        }
    }
    for sec in order {
        let lim = section_limit(sec);
        out.extend(rows.iter().filter(|r| r.section == sec).take(lim).cloned());
    }
    if !web_top {
        out.push(row("Web", format!("Search the web for \u{201c}{q}\u{201d}"), "Opens your browser · try \u{201c}g\u{201d} or \u{201c}yt\u{201d} before a search".into(), RIcon::Glyph(G_GLOBE, theme::text_2()), "Search", Act::Web(q.clone()), None, 0));
    }
    out
}

impl Launchpad {
    /// Recompute search results for the current query.
    pub fn update_results(&mut self, d: &Data) {
        let out = build_results(&self.query, d, self.armed);
        self.results_len = out.len();
        self.sel = self.sel.min(out.len().saturating_sub(1));
        // Keep the selection on a command waiting for its second Enter, wherever it moved.
        if let Some(cmd) = self.armed {
            if let Some(i) = out.iter().position(|r| r.act == Act::Sys(cmd)) {
                self.sel = i;
            }
        }
        self.results = out;
    }

    /// The learning key of the result row that does `act` (None outside search results).
    pub fn result_key(&self, act: &Act) -> Option<String> {
        if self.query.trim().is_empty() {
            return None;
        }
        self.results.iter().find(|r| &r.act == act).and_then(|r| r.key.clone())
    }

    /// App ids whose icons the results show.
    pub fn result_app_ids(&self) -> Vec<String> {
        self.results.iter().filter_map(|r| if let RIcon::App(id) = &r.icon { Some(id.clone()) } else { None }).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_results(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, inner: &ContainerVisual, ox: f32, oy: f32, pw: f32, mut y: f32) -> Result<f32> {
        let s = self.scale;
        let pad = 22.0 * s;
        let row_w = pw - 2.0 * pad;
        let mut last_section = "";
        let results = self.results.clone();
        for (i, r) in results.iter().enumerate() {
            let top = i == 0;
            let section = if top && !r.answer { "Top Hit" } else { r.section };
            if section != last_section || (i == 1 && !results[0].answer) {
                if i > 0 {
                    y += 4.0 * s;
                }
                Self::txt(c, gfx, inner, section, TextStyle { px: 12.0 * s, weight: 600, color: theme::text_3(), ..Default::default() }, pad + 10.0 * s, y + 6.0 * s)?;
                y += 28.0 * s;
                last_section = section;
            }
            let rh = if top && r.answer { 76.0 * s } else if top { 64.0 * s } else { 46.0 * s };
            let isz = if top { 44.0 * s } else { 30.0 * s };
            let selected = i == self.sel;
            if selected {
                // A fresh highlight each render (a visual can only have one parent).
                let (ss, _, _) = ui::rounded(c, row_w, rh - 2.0 * s, 10.0 * s, rgba(10, 132, 255, 0.85))?;
                ss.SetOffset(v3(pad, y, 0.0))?;
                inner.Children()?.InsertAtBottom(&ss)?;
            }
            let icx = pad + 10.0 * s + isz / 2.0;
            let gpx = if top { 24.0 } else { 17.0 } * s;
            match &r.icon {
                RIcon::App(id) => Self::icon(c, inner, d.brushes.get(id), pad + 10.0 * s, y + (rh - isz) / 2.0, isz)?,
                RIcon::Glyph(g, col) => {
                    let col = if selected { (1.0, 1.0, 1.0, 0.95) } else { *col };
                    Self::glyph(c, gfx, inner, g, gpx, col, icx, y + rh / 2.0)?
                }
                RIcon::Emoji(e) => {
                    // Settings glyphs from Windows' index are icon-font characters; real
                    // emoji (weather) are drawn larger with the colour font.
                    let is_icon = e.chars().all(|ch| ('\u{E000}'..='\u{F8FF}').contains(&ch));
                    if is_icon {
                        let col = if selected { (1.0, 1.0, 1.0, 0.95) } else { theme::text_2() };
                        Self::glyph(c, gfx, inner, e, gpx, col, icx, y + rh / 2.0)?
                    } else {
                        let px = if top { 30.0 } else { 20.0 } * s;
                        let (v, w, h) = ui::text(c, gfx, e, &TextStyle { px, ..Default::default() })?;
                        v.SetOffset(v3((icx - w / 2.0).round(), (y + rh / 2.0 - h / 2.0).round(), 0.0))?;
                        inner.Children()?.InsertAtTop(&v)?;
                    }
                }
            }
            // Right side: toggle state, and on the selected row what Enter does.
            let mut rx = pad + row_w - 12.0 * s;
            if selected {
                let hint = format!("\u{21B5} {}", r.verb);
                let st = TextStyle { px: 11.5 * s, weight: 500, color: (1.0, 1.0, 1.0, 0.85), ..Default::default() };
                let (w, h) = Self::txt(c, gfx, inner, &hint, st.clone(), 0.0, -1000.0)?;
                Self::txt(c, gfx, inner, &hint, st, rx - w, y + (rh - h) / 2.0)?;
                rx -= w + 12.0 * s;
            }
            if let Some(acc) = &r.accessory {
                let on = acc == "On";
                let st = TextStyle { px: 11.0 * s, weight: 600, color: if on || selected { (1.0, 1.0, 1.0, 0.95) } else { theme::text_2() }, ..Default::default() };
                let (w, h) = Self::txt(c, gfx, inner, acc, st.clone(), 0.0, -1000.0)?;
                let (pw2, ph) = (w + 16.0 * s, h + 6.0 * s);
                let bg = if on { theme::accent() } else if selected { rgba(255, 255, 255, 0.22) } else { theme::ink(0.12) };
                Self::rect(c, inner, rx - pw2, y + (rh - ph) / 2.0, pw2, ph, ph / 2.0, bg)?;
                Self::txt(c, gfx, inner, acc, st, rx - pw2 + 8.0 * s, y + (rh - h) / 2.0)?;
                rx -= pw2 + 10.0 * s;
            }
            let tx = pad + 22.0 * s + isz;
            let max_w = (rx - tx - 8.0 * s).max(40.0 * s);
            let (tc, sc) = if selected { ((1.0, 1.0, 1.0, 1.0), (1.0, 1.0, 1.0, 0.78)) } else { (theme::text(), theme::text_2()) };
            let (tpx, tw) = if top && r.answer { (24.0, 600) } else if top { (17.0, 600) } else { (13.5, 500) };
            let gap = if top && r.answer { 30.0 } else if top { 22.0 } else { 17.0 };
            Self::txt(c, gfx, inner, &r.title, TextStyle { px: tpx * s, weight: tw, color: tc, max_w: Some(max_w), ..Default::default() }, tx, y + rh / 2.0 - gap * s)?;
            Self::txt(c, gfx, inner, &r.sub, TextStyle { px: 11.5 * s, color: sc, max_w: Some(max_w), ..Default::default() }, tx, y + rh / 2.0 + if top && r.answer { 5.0 } else { 1.0 } * s)?;
            self.hit(PanelId::Center, (ox + pad, oy + y, row_w, rh - 2.0 * s), r.act.clone(), 10.0 * s, true, true);
            y += rh;
        }
        Ok(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_shortcuts() {
        let (e, q, url) = web_shortcut("g rust traits").unwrap();
        assert_eq!((e, q.as_str()), ("Google", "rust traits"));
        assert_eq!(url, "https://www.google.com/search?q=rust%20traits");
        assert_eq!(web_shortcut("yt lofi").unwrap().0, "YouTube");
        assert!(web_shortcut("g").is_none());
        assert!(web_shortcut("g ").is_none());
        assert!(web_shortcut("good morning").is_none(), "only exact prefixes");
    }

    #[test]
    fn urls() {
        assert_eq!(as_url("github.com").as_deref(), Some("https://github.com"));
        assert_eq!(as_url("https://x.dev/a?b").as_deref(), Some("https://x.dev/a?b"));
        assert_eq!(as_url("localhost:3000").as_deref(), Some("http://localhost:3000"));
        for no in ["notes.txt", "budget.xlsx", "vs code", "a.b", "3.14", "readme.md"] {
            assert_eq!(as_url(no), None, "{no}");
        }
    }

    #[test]
    fn volume() {
        assert_eq!(volume_query("volume 40"), Some(40));
        assert_eq!(volume_query("vol 5%"), Some(5));
        assert_eq!(volume_query("set volume to 100"), Some(100));
        assert_eq!(volume_query("volume 140"), None);
        assert_eq!(volume_query("volume"), None);
    }

    #[test]
    fn clock() {
        assert_eq!(clock_text(0, 5), "12:05 AM");
        assert_eq!(clock_text(13, 30), "1:30 PM");
    }

    /// Real searches against this PC's apps and Windows' settings index. Prints what each
    /// query shows; set LAMINAR_SEARCH_REPORT=1 to see it (`cargo test -- --nocapture`).
    pub(super) fn real_data() -> (Vec<laminar_sys::AppEntry>, crate::config::Config, crate::services::ControlState, crate::widgets::Hub) {
        laminar_sys::com_init_sta();
        laminar_sys::settings_index::warm();
        let t = std::time::Instant::now();
        while laminar_sys::settings_index::get().is_none() && t.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let apps = laminar_sys::apps::installed_apps();
        let control = crate::services::ControlState { wifi: Some(true), bluetooth: Some(false), muted: Some(false), volume: Some(0.4), ..Default::default() };
        (apps, crate::config::Config::default(), control, crate::widgets::Hub::new(windows::Win32::Foundation::HWND::default()))
    }

    pub(super) fn top(q: &str, apps: &[laminar_sys::AppEntry], cfg: &crate::config::Config, control: &crate::services::ControlState, hub: &crate::widgets::Hub, history: &laminar_sys::history::History) -> Vec<(String, String, String)> {
        let brushes = HashMap::new();
        let names = HashMap::new();
        let d = Data {
            apps,
            brushes: &brushes,
            pinned: &[],
            names: &names,
            control,
            stats: None,
            weather: None,
            files: &[],
            cfg,
            user: "Test",
            wifi: None,
            tray: &[],
            tray_list: &[],
            hub,
            whatsapp: None,
            update: None,
            app_version: "0.0.0",
            app_update: "",
            update_ui: crate::launchpad::UpdateUi::Off,
            windows: &[],
            history,
        };
        build_results(q, &d, None).into_iter().map(|r| (r.section.to_string(), r.title, r.sub)).collect()
    }

    #[test]
    fn real_searches() {
        let (apps, cfg, control, hub) = real_data();
        let hist = laminar_sys::history::History::default();
        if laminar_sys::settings_index::get().map(|v| v.is_empty()).unwrap_or(true) {
            return; // no Windows index here
        }
        let report = std::env::var("LAMINAR_SEARCH_REPORT").is_ok();
        // (query, expected Top Hit title fragment, case-insensitive)
        let cases: &[(&str, &str)] = &[
            ("night light", "night light"),
            ("vpn", "vpn"),
            ("resolution", "resolution"),
            ("startup apps", "startup apps"),
            ("environment variables", "environment variables"),
            ("bluetooth", "turn bluetooth"),
            ("wifi", "turn wi-fi"),
            ("dark mode", "switch to dark mode"),
            ("wallpaper", "wallpaper"),
            ("mouse speed", "mouse"),
            ("printer", "printer"),
            ("proxy", "proxy"),
            ("windows update", "update"),
            ("uninstall", "installed apps"),
            ("device manager", "device manager"),
            ("sound", "sound"),
            ("display", "display"),
            ("12*7+3", "87"),
            ("5 km in miles", "3.10686 mi"),
            ("time in tokyo", ":"),
            ("g rust traits", "google"),
            ("github.com", "github.com"),
            ("volume 30", "30%"),
            ("restart", "restart"),
            ("google chrome", "chrome"),
            ("3x4", "12"),
            ("sound", "sound settings"),
            ("brightness", "brightness"),
            ("calculator", "calculator"),
            ("wifi settings", "wi-fi"),
            ("lock", "lock screen"),
            ("screenshot", "take a screenshot"),
            ("empty trash", "recycle bin"),
        ];
        let mut failed = Vec::new();
        let mut slowest = std::time::Duration::ZERO;
        for (q, want) in cases {
            let t = std::time::Instant::now();
            let rows = top(q, &apps, &cfg, &control, &hub, &hist);
            slowest = slowest.max(t.elapsed());
            if report {
                eprintln!("
{q:?} ({:?})", t.elapsed());
                for (i, (sec, title, sub)) in rows.iter().take(6).enumerate() {
                    eprintln!("  {}{sec:<11} {title}  —  {sub}", if i == 0 { "▶ " } else { "  " });
                }
            }
            let ok = rows.first().map(|r| r.1.to_lowercase().contains(want)).unwrap_or(false);
            if !ok {
                failed.push(format!("{q:?}: top = {:?}", rows.first().map(|r| &r.1)));
            }
        }
        eprintln!("slowest query: {slowest:?}");
        assert!(failed.is_empty(), "{failed:#?}");
        assert!(slowest.as_millis() < 150, "a query took {slowest:?}");
    }

    #[test]
    fn learning_lifts_a_pick() {
        let (apps, cfg, control, hub) = real_data();
        if laminar_sys::settings_index::get().map(|v| v.is_empty()).unwrap_or(true) {
            return;
        }
        let mut hist = laminar_sys::history::History::default();
        let before = top("v", &apps, &cfg, &control, &hub, &hist);
        assert!(!before.first().map(|r| r.1.to_lowercase().contains("vpn")).unwrap_or(false), "VPN isn't the top hit for \"v\" before learning");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        hist.record("vpn", "set:ms-settings:network-vpn|vpn settings", now);
        let after = top("v", &apps, &cfg, &control, &hub, &hist);
        assert_eq!(after.first().map(|r| r.1.as_str()), Some("VPN settings"), "{after:?}");
    }
}
