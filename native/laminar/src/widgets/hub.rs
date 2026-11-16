//! Widget state that lives as long as the dock: connector feeds (fetched on worker
//! threads), timers, screen time, keep-awake, microphone and brightness.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use laminar_sys::feed::{Feed, FeedError, Params};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Imaging::IWICBitmapSource;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use super::catalog::TimerKind;

/// Posted to the dock when a widget job finishes.
pub const WM_APP_WIDGETS: u32 = WM_APP + 11;

/// One connector's latest result.
#[derive(Default)]
pub struct Slot {
    pub feed: Option<Feed>,
    pub err: Option<FeedError>,
    pub at: Option<Instant>,
    pub busy: bool,
    /// Settings fingerprint the result was fetched with (a change refetches at once).
    pub sig: u64,
}

/// A countdown that can be paused (Pomodoro and Timer).
#[derive(Clone)]
pub struct Countdown {
    pub total: u32,
    /// Seconds left while paused.
    pub paused_left: f32,
    pub end: Option<Instant>,
    /// Pomodoro: 0 focus, 1 break.
    pub phase: u8,
    /// Just finished (shown until the next click).
    pub done: bool,
}

impl Countdown {
    pub fn new(total: u32) -> Self {
        Self { total, paused_left: total as f32, end: None, phase: 0, done: false }
    }
    pub fn running(&self) -> bool {
        self.end.is_some()
    }
    pub fn left(&self) -> f32 {
        match self.end {
            Some(e) => e.saturating_duration_since(Instant::now()).as_secs_f32(),
            None => self.paused_left,
        }
    }
    pub fn toggle(&mut self) {
        self.done = false;
        match self.end {
            Some(_) => {
                self.paused_left = self.left();
                self.end = None;
            }
            None => {
                if self.paused_left <= 0.5 {
                    self.paused_left = self.total as f32;
                }
                self.end = Some(Instant::now() + Duration::from_secs_f32(self.paused_left));
            }
        }
    }
    pub fn reset(&mut self, total: u32) {
        *self = Countdown { phase: self.phase, ..Countdown::new(total) };
    }
    /// True once, when it reaches zero.
    pub fn tick(&mut self) -> bool {
        if let Some(e) = self.end {
            if Instant::now() >= e {
                self.end = None;
                self.paused_left = 0.0;
                self.done = true;
                return true;
            }
        }
        false
    }
}

#[derive(Default, Clone)]
pub struct Stopwatch {
    pub acc: Duration,
    pub start: Option<Instant>,
}

impl Stopwatch {
    pub fn elapsed(&self) -> Duration {
        self.acc + self.start.map(|s| s.elapsed()).unwrap_or_default()
    }
    pub fn toggle(&mut self) {
        match self.start.take() {
            Some(s) => self.acc += s.elapsed(),
            None => self.start = Some(Instant::now()),
        }
    }
}

/// Time at the PC today: seconds per app and per hour. Counted only while you're active
/// (input in the last two minutes) and saved to `%APPDATA%\Laminar\screentime.json`.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct ScreenTime {
    pub day: String,
    pub apps: BTreeMap<String, u64>,
    pub hours: Vec<u32>,
}

impl ScreenTime {
    fn path() -> std::path::PathBuf {
        crate::config::dir().join("screentime.json")
    }
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        // config.rs watches the folder for config.json only; this file doesn't trigger reloads.
        if let Ok(t) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::path(), t);
        }
    }
    pub fn total(&self) -> u64 {
        self.apps.values().sum()
    }
    pub fn add(&mut self, today: &str, hour: usize, app: &str, secs: u64) {
        if self.day != today {
            *self = ScreenTime { day: today.to_string(), apps: BTreeMap::new(), hours: vec![0; 24] };
        }
        if self.hours.len() != 24 {
            self.hours = vec![0; 24];
        }
        *self.apps.entry(app.to_string()).or_default() += secs;
        self.hours[hour.min(23)] += secs as u32;
    }
}

enum Job {
    Fetch(String, Params),
    Mic,
    SetMic(bool),
    Brightness,
    SetBrightness(f32),
    Output,
    ApplyWallpaper(String),
    Geocode(String),
}

enum Reply {
    Feed(String, Result<Feed, FeedError>),
    Mic(Option<bool>),
    Brightness(Option<f32>),
    Output(Option<String>),
    Geocode(String, Option<(f32, f32, String)>),
}

pub struct Hub {
    pub slots: HashMap<String, Slot>,
    pub timers: HashMap<TimerKind, Countdown>,
    pub stopwatch: Stopwatch,
    pub keep_awake: bool,
    pub mic_muted: Option<bool>,
    pub brightness: Option<f32>,
    pub output_name: Option<String>,
    pub screen: ScreenTime,
    screen_dirty: u32,
    /// Recent CPU % and network (down, up) bytes/s, oldest first.
    pub cpu_hist: VecDeque<f32>,
    pub net_hist: VecDeque<(f64, f64)>,
    pub notes: String,
    notes_at: Option<Instant>,
    /// Decoded pictures: (key, bitmap). Key = file path or a hash of the bytes.
    pub wallpaper_img: Option<(String, IWICBitmapSource)>,
    pub art_img: Option<(u64, IWICBitmapSource)>,
    pub desktop_shown: bool,
    /// A weather location lookup that finished: (what was typed, (lat, lon, place)).
    pub geocoded: Option<(String, Option<(f32, f32, String)>)>,
    hwnd: isize,
    replies: Arc<Mutex<Vec<Reply>>>,
}

impl Hub {
    pub fn new(notify: HWND) -> Self {
        let mut timers = HashMap::new();
        timers.insert(TimerKind::Pomodoro, Countdown::new(25 * 60));
        timers.insert(TimerKind::Countdown, Countdown::new(5 * 60));
        Self {
            slots: HashMap::new(),
            timers,
            stopwatch: Stopwatch::default(),
            keep_awake: false,
            mic_muted: None,
            brightness: None,
            output_name: None,
            screen: ScreenTime::load(),
            screen_dirty: 0,
            cpu_hist: VecDeque::new(),
            net_hist: VecDeque::new(),
            notes: String::new(),
            notes_at: None,
            wallpaper_img: None,
            art_img: None,
            desktop_shown: false,
            geocoded: None,
            hwnd: notify.0 as isize,
            replies: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn run(&self, job: Job) {
        let out = self.replies.clone();
        let hwnd = self.hwnd;
        std::thread::Builder::new()
            .name("laminar-widget-job".into())
            .spawn(move || {
                laminar_sys::com_init_sta();
                let r = match job {
                    Job::Fetch(id, params) => Some(Reply::Feed(id.clone(), fetch_with_secrets(&id, params))),
                    Job::Mic => Some(Reply::Mic(laminar_sys::audio::mic_muted())),
                    Job::SetMic(m) => {
                        laminar_sys::audio::set_mic_muted(m);
                        Some(Reply::Mic(laminar_sys::audio::mic_muted()))
                    }
                    Job::Brightness => Some(Reply::Brightness(laminar_sys::display::brightness())),
                    Job::SetBrightness(v) => {
                        laminar_sys::display::set_brightness(v);
                        Some(Reply::Brightness(laminar_sys::display::brightness()))
                    }
                    Job::Output => Some(Reply::Output(laminar_sys::audio::default_output_name())),
                    Job::Geocode(city) => {
                        let r = laminar_sys::weather::geocode(&city);
                        Some(Reply::Geocode(city, r))
                    }
                    Job::ApplyWallpaper(p) => {
                        laminar_sys::connectors::apply_wallpaper(&p);
                        None
                    }
                };
                if let Some(r) = r {
                    out.lock().unwrap().push(r);
                    unsafe {
                        let _ = PostMessageW(Some(HWND(hwnd as _)), WM_APP_WIDGETS, WPARAM(0), LPARAM(0));
                    }
                }
            })
            .ok();
    }

    /// Apply finished jobs. Returns true if anything changed (redraw).
    pub fn take_replies(&mut self) -> bool {
        let list = std::mem::take(&mut *self.replies.lock().unwrap());
        let changed = !list.is_empty();
        for r in list {
            match r {
                Reply::Feed(id, res) => {
                    let s = self.slots.entry(id.clone()).or_default();
                    s.busy = false;
                    s.at = Some(Instant::now());
                    match res {
                        Ok(f) => {
                            if id == "wallpaper" {
                                // New picture: decode it again on the next draw.
                                if self.wallpaper_img.as_ref().map(|w| Some(&w.0) != f.extra.get("image_path")).unwrap_or(false) {
                                    self.wallpaper_img = None;
                                }
                            }
                            s.feed = Some(f);
                            s.err = None;
                        }
                        Err(e) => {
                            crate::dock::trace_widget(&format!("connector {id}: {e}"));
                            s.err = Some(e);
                        }
                    }
                }
                Reply::Mic(m) => self.mic_muted = m,
                Reply::Brightness(b) => self.brightness = b,
                Reply::Output(n) => self.output_name = n,
                Reply::Geocode(city, r) => self.geocoded = Some((city, r)),
            }
        }
        changed
    }

    /// Fetch these connectors if they're due (or their settings changed).
    pub fn want(&mut self, cfg: &crate::config::Config, conns: &[&str]) {
        for &id in conns {
            let Some(info) = laminar_sys::connectors::info(id) else { continue };
            let params = cfg.widget_settings.get(id).cloned().unwrap_or_default();
            let sig = fingerprint(id, &params, info);
            let s = self.slots.entry(id.to_string()).or_default();
            if s.busy {
                continue;
            }
            let due = match (s.at, &s.err) {
                (None, _) => true,
                _ if s.sig != sig => true,
                (Some(_), Some(FeedError::NeedsSetup(_) | FeedError::Unauthorized(_))) => false,
                (Some(t), Some(_)) => t.elapsed().as_secs() >= info.refresh_secs.min(120),
                (Some(t), None) => t.elapsed().as_secs() >= info.refresh_secs,
            };
            if due {
                s.busy = true;
                s.sig = sig;
                let p: Params = params.into_iter().collect();
                self.run(Job::Fetch(id.to_string(), p));
            }
        }
    }

    /// Forget a connector's result (after its settings change) so it refetches.
    pub fn reset(&mut self, id: &str) {
        self.slots.remove(id);
    }

    pub fn ask_mic(&self) {
        self.run(Job::Mic);
    }
    pub fn set_mic(&mut self, m: bool) {
        self.mic_muted = Some(m);
        self.run(Job::SetMic(m));
    }
    pub fn ask_brightness(&self) {
        self.run(Job::Brightness);
    }
    pub fn set_brightness(&mut self, v: f32) {
        self.brightness = Some(v);
        self.run(Job::SetBrightness(v));
    }
    pub fn ask_output(&self) {
        self.run(Job::Output);
    }
    /// Look up a place name for the weather (answer in `geocoded`).
    pub fn geocode(&self, city: String) {
        self.run(Job::Geocode(city));
    }

    pub fn apply_wallpaper(&self, path: String) {
        self.run(Job::ApplyWallpaper(path));
    }

    pub fn feed(&self, id: &str) -> Option<&Feed> {
        self.slots.get(id).and_then(|s| s.feed.as_ref())
    }
    pub fn err(&self, id: &str) -> Option<&FeedError> {
        self.slots.get(id).and_then(|s| s.err.as_ref())
    }

    /// Keep Windows (and the screen) awake while on.
    pub fn set_keep_awake(&mut self, on: bool) {
        use windows::Win32::System::Power::*;
        self.keep_awake = on;
        unsafe {
            if on {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
            } else {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }

    /// Once a second: timers, screen time, notes. Returns finished timers.
    pub fn tick(&mut self, fg_app: Option<&str>, now: &super::Now) -> Vec<TimerKind> {
        let mut done = Vec::new();
        for (k, t) in self.timers.iter_mut() {
            if t.tick() {
                done.push(*k);
            }
        }
        // Pomodoro: after focus comes a 5-minute break (and back), ready to start.
        if let Some(p) = self.timers.get_mut(&TimerKind::Pomodoro) {
            if done.contains(&TimerKind::Pomodoro) {
                p.phase = 1 - p.phase;
                let total = if p.phase == 1 { 5 * 60 } else { 25 * 60 };
                p.total = total;
                p.paused_left = total as f32;
            }
        }
        if let Some(app) = fg_app {
            if idle_secs() < 120 {
                self.screen.add(&now.ymd(), now.hour as usize, app, 1);
                self.screen_dirty += 1;
                if self.screen_dirty >= 60 {
                    self.screen_dirty = 0;
                    self.screen.save();
                }
            }
        }
        if self.notes_at.map(|t| t.elapsed().as_secs() > 5).unwrap_or(true) {
            self.notes_at = Some(Instant::now());
            self.notes = std::fs::read_to_string(notes_path()).unwrap_or_default();
        }
        done
    }

    pub fn push_stats(&mut self, st: &laminar_sys::stats::Stats) {
        self.cpu_hist.push_back(st.cpu_percent);
        self.net_hist.push_back((st.net_down_bps, st.net_up_bps));
        while self.cpu_hist.len() > 40 {
            self.cpu_hist.pop_front();
        }
        while self.net_hist.len() > 40 {
            self.net_hist.pop_front();
        }
    }

    pub fn shutdown(&mut self) {
        self.screen.save();
        if self.keep_awake {
            self.set_keep_awake(false);
        }
    }
}

/// Notes widget file: `%APPDATA%\Laminar\notes.txt`.
pub fn notes_path() -> std::path::PathBuf {
    crate::config::dir().join("notes.txt")
}

fn idle_secs() -> u32 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut li).as_bool() {
            GetTickCount().wrapping_sub(li.dwTime) / 1000
        } else {
            0
        }
    }
}

/// Settings + which secrets are stored, hashed (secrets' values never leave Credential Manager here).
fn fingerprint(id: &str, params: &BTreeMap<String, String>, info: &laminar_sys::feed::ConnectorInfo) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    params.hash(&mut h);
    for f in info.fields.iter().filter(|f| f.secret) {
        (f.key, laminar_sys::secrets::has(id, f.key)).hash(&mut h);
    }
    h.finish()
}

/// Worker side: add the connector's secrets from Credential Manager, then fetch.
fn fetch_with_secrets(id: &str, mut p: Params) -> Result<Feed, FeedError> {
    if let Some(info) = laminar_sys::connectors::info(id) {
        for f in info.fields.iter().filter(|f| f.secret) {
            if let Some(v) = laminar_sys::secrets::get(id, f.key) {
                p.insert(f.key.to_string(), v);
            }
        }
    }
    laminar_sys::connectors::fetch(id, &p)
}

// ------------------------------------------------------------------ sample data

fn rng(seed: &str) -> impl FnMut() -> f64 {
    let mut x: u64 = seed.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211));
    move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % 10_000) as f64 / 10_000.0
    }
}

fn walk(seed: &str, n: usize, base: f64, vol: f64, trend: f64) -> Vec<f64> {
    let mut r = rng(seed);
    let mut v = base;
    (0..n)
        .map(|i| {
            v = (v + (r() - 0.5) * vol + trend).max(base * 0.2);
            v + (i as f64 * 0.0)
        })
        .collect()
}

/// Made-up data so the gallery can show what a connector widget looks like before it's connected.
pub fn sample(conn: &str) -> Feed {
    use laminar_sys::feed::FeedRow;
    let row = |l: &str, v: &str, f: f32| FeedRow { label: l.into(), value: v.into(), frac: Some(f), delta: None };
    let money = |v: f64, s: &str| Feed {
        value: Some(v),
        currency: Some("USD".into()),
        caption: "Last 30 days".into(),
        delta_pct: Some(12.4),
        series: walk(s, 30, v / 30.0, v / 40.0, v / 900.0),
        ..Default::default()
    };
    let mut f = match conn {
        "stripe" => money(5336.0, conn),
        "lemonsqueezy" => money(3630.0, conn),
        "paddle" => money(2970.0, conn),
        "shopify" => money(17491.0, conn),
        "gumroad" => money(1407.0, conn),
        "polar" => money(1613.0, conn),
        "revenuecat" => Feed { value: Some(6807.0), currency: Some("USD".into()), caption: "MRR".into(), extra: [("active_subscriptions".to_string(), "812".to_string())].into(), ..Default::default() },
        "anthropic" => Feed { value: Some(83.46), currency: Some("USD".into()), caption: "This month".into(), series: walk(conn, 14, 6.0, 4.0, 0.1), ..Default::default() },
        "openai" => Feed { value: Some(41.20), currency: Some("USD".into()), caption: "This month".into(), series: walk(conn, 14, 3.0, 2.5, 0.05), ..Default::default() },
        "openrouter" => Feed { value: Some(18.4), currency: Some("USD".into()), caption: "Credits left".into(), frac: Some(0.63), extra: [("total_credits".to_string(), "50".to_string())].into(), ..Default::default() },
        "cursor" => Feed { value: Some(126.0), currency: Some("USD".into()), caption: "This cycle".into(), ..Default::default() },
        "claude_code" | "codex" => Feed {
            value: Some(if conn == "codex" { 412_000.0 } else { 1_920_000.0 }),
            unit: "tokens".into(),
            caption: "Today".into(),
            frac: Some(0.19),
            series: walk(conn, 14, 900_000.0, 700_000.0, 20_000.0),
            extra: [("tokens_5h".to_string(), "380000".to_string()), ("tokens_week".to_string(), "9400000".to_string())].into(),
            ..Default::default()
        },
        "plausible" | "umami" => Feed { value: Some(4012.0), unit: "visitors".into(), caption: "Last 30 days".into(), delta_pct: Some(8.1), series: walk(conn, 30, 130.0, 60.0, 1.0), ..Default::default() },
        "search_console" => Feed { value: Some(834.0), unit: "clicks".into(), caption: "Last 28 days".into(), delta_pct: Some(18.0), series: walk(conn, 28, 30.0, 12.0, 0.3), ..Default::default() },
        "supabase" => Feed { value: Some(41_800.0), unit: "requests".into(), caption: "Last 24 hours".into(), series: walk(conn, 24, 1700.0, 600.0, 10.0), ..Default::default() },
        "resend" => Feed { value: Some(1208.0), unit: "emails".into(), caption: "Emails sent".into(), delta_pct: Some(4.0), ..Default::default() },
        "gmail" => Feed { value: Some(12.0), caption: "Unread".into(), rows: vec![row("Your invoice is ready", "", 0.0), row("Team sync notes", "", 0.0)], ..Default::default() },
        "x" => Feed { value: Some(2400.0), caption: "Followers".into(), ..Default::default() },
        "tiktok" => Feed { value: Some(8900.0), caption: "Followers".into(), ..Default::default() },
        "youtube" => Feed { value: Some(12_400.0), caption: "Subscribers".into(), ..Default::default() },
        "instagram" => Feed { value: Some(21_700.0), caption: "Followers".into(), ..Default::default() },
        "github" | "leetcode" => {
            let mut r = rng(conn);
            let grid: Vec<f32> = (0..371).map(|_| { let v = r(); if v < 0.35 { 0.0 } else { (v as f32 - 0.35) / 0.65 } }).collect();
            Feed {
                value: Some(if conn == "github" { 1186.0 } else { 312.0 }),
                caption: if conn == "github" { "Contributions this year".into() } else { "Problems solved".into() },
                grid,
                rows: if conn == "github" { vec![row("Repositories", "42", 0.0), row("Stars", "1.3K", 0.0), row("Followers", "318", 0.0)] } else { vec![row("Easy", "160", 0.0), row("Medium", "128", 0.0), row("Hard", "24", 0.0)] },
                extra: [("streak".to_string(), "7".to_string())].into(),
                ..Default::default()
            }
        }
        "linear" => Feed { value: Some(6.0), caption: "Assigned to you".into(), rows: vec![row("ENG-142 Dock widgets", "In Progress", 0.0), row("ENG-139 Flip clock", "Todo", 0.0), row("ENG-131 Tray hover", "Todo", 0.0)], ..Default::default() },
        "vercel" => Feed { value: Some(14.0), caption: "Deploys this week".into(), rows: vec![row("site", "Ready", 0.0), row("docs", "Ready", 0.0), row("api", "Building", 0.0)], ..Default::default() },
        "stocks" => Feed {
            value: Some(248.31),
            currency: Some("USD".into()),
            delta_pct: Some(1.42),
            series: walk(conn, 78, 245.0, 1.4, 0.04),
            rows: vec![
                FeedRow { label: "AAPL".into(), value: "$228.10".into(), frac: None, delta: Some(0.62) },
                FeedRow { label: "MSFT".into(), value: "$431.75".into(), frac: None, delta: Some(-0.38) },
                FeedRow { label: "NVDA".into(), value: "$142.09".into(), frac: None, delta: Some(2.11) },
                FeedRow { label: "TSLA".into(), value: "$248.31".into(), frac: None, delta: Some(1.42) },
            ],
            extra: [("symbol".to_string(), "TSLA".to_string())].into(),
            ..Default::default()
        },
        "crypto" => Feed {
            value: Some(96_412.0),
            currency: Some("USD".into()),
            delta_pct: Some(2.3),
            series: walk(conn, 96, 94_000.0, 500.0, 25.0),
            extra: [("symbol".to_string(), "BTC".to_string())].into(),
            ..Default::default()
        },
        "calendar" => Feed {
            value: Some(3.0),
            rows: vec![row("Design review", "Today 2:00 PM", 0.0), row("1:1 with Sam", "Today 4:30 PM", 0.0), row("Dentist", "Tue 9:00 AM", 0.0)],
            extra: [("next_title".to_string(), "Design review".to_string()), ("next_time".to_string(), "Today 2:00 PM".to_string()), ("next_minutes".to_string(), "42".to_string())].into(),
            ..Default::default()
        },
        "top_cpu" => Feed { value: Some(23.0), unit: "%".into(), rows: vec![row("Microsoft Edge", "8.4%", 0.36), row("Visual Studio Code", "5.1%", 0.22), row("Slack", "2.2%", 0.1), row("Laminar Dock", "0.3%", 0.02)], ..Default::default() },
        "top_memory" => Feed { value: Some(14.2), unit: "GB".into(), rows: vec![row("Microsoft Edge", "2.1 GB", 0.13), row("Visual Studio Code", "1.4 GB", 0.09), row("Slack", "640 MB", 0.04), row("Explorer", "180 MB", 0.01)], ..Default::default() },
        "recent_files" => Feed { value: Some(3.0), rows: vec![row("Quarterly plan.docx", "", 0.0), row("Invoice-1042.pdf", "", 0.0), row("photo.png", "", 0.0)], ..Default::default() },
        "battery" => Feed { value: Some(82.0), frac: Some(0.82), unit: "%".into(), extra: [("charging".to_string(), "false".to_string()), ("time_left".to_string(), "4h 10m".to_string())].into(), ..Default::default() },
        "storage" => Feed { value: Some(240.8), unit: "GB".into(), caption: "Free on C:".into(), frac: Some(0.74), rows: vec![row("C:", "240 GB free of 931 GB", 0.74)], ..Default::default() },
        "language" => Feed { text: Some("ENG".into()), caption: "English (Canada)".into(), ..Default::default() },
        "uptime" => Feed { value: Some(52.0), text: Some("2d 4h".into()), caption: "Uptime".into(), ..Default::default() },
        "wallpaper" => Feed { text: Some("Today's image".into()), ..Default::default() },
        _ => Feed { value: Some(42.0), ..Default::default() },
    };
    f.extra.insert("sample".into(), "1".into());
    f
}
