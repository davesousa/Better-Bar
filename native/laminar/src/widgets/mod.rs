//! Widgets for Launchpad and the dock.
//!
//! - `catalog`: every widget (id, name, category, face, connector)
//! - `canvas`: Direct2D painting helpers
//! - `paint`: how each face draws, at Launchpad size or compact (dock) size
//! - `hub`: live state: connector feeds, timers, screen time, keep-awake
//!
//! A widget is one bitmap. `render` paints it into a Composition drawing surface and
//! returns the clickable areas inside it, so Launchpad and the dock can route clicks.

pub mod canvas;
pub mod catalog;
pub mod hub;
mod paint;
pub mod sheet;
pub mod style;

use std::collections::HashMap;

use laminar_sys::stats::Stats;
use laminar_sys::weather::Weather;
use windows::core::Result;
use windows::UI::Composition::CompositionDrawingSurface;

use crate::config::Config;
use crate::gfx::Gfx;
use crate::services::ControlState;
pub use catalog::{Def, Size, TimerKind};
pub use hub::Hub;

/// Logical height of a medium Launchpad widget card (a large one is twice this plus a gap).
pub const CARD_H: f32 = 96.0;
/// Small cards: half a column wide.
pub const SMALL_H: f32 = 120.0;
pub const GAP: f32 = 12.0;

/// Logical size of a card in a column `col_w` wide.
pub fn card_size(size: Size, col_w: f32) -> (f32, f32) {
    match size {
        Size::Small => (((col_w - GAP) / 2.0).floor(), SMALL_H),
        Size::Medium => (col_w, CARD_H),
        Size::Large => (col_w, CARD_H * 2.0 + GAP),
    }
}

/// Where each card goes in a column (logical px): small cards pair up side by side.
pub fn pack(sizes: &[Size], col_w: f32) -> (Vec<(f32, f32, f32, f32)>, f32) {
    let mut out = Vec::with_capacity(sizes.len());
    let mut y = 0.0f32;
    let mut half: Option<(f32, f32)> = None; // (row top, row height) with a free right half
    for &sz in sizes {
        let (w, h) = card_size(sz, col_w);
        match (sz, half) {
            (Size::Small, Some((top, rh))) => {
                out.push((col_w - w, top, w, h));
                y = top + rh.max(h) + GAP;
                half = None;
            }
            (Size::Small, None) => {
                out.push((0.0, y, w, h));
                half = Some((y, h));
            }
            (_, open) => {
                if let Some((top, rh)) = open {
                    y = top + rh + GAP;
                    half = None;
                }
                out.push((0.0, y, w, h));
                y += h + GAP;
            }
        }
    }
    if let Some((top, rh)) = half {
        y = top + rh + GAP;
    }
    (out, y)
}

/// Local date and time.
#[derive(Clone, Copy, Debug, Default)]
pub struct Now {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    /// 0 = Sunday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Now {
    pub fn get() -> Self {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        Self {
            year: t.wYear as u32,
            month: t.wMonth as u32,
            day: t.wDay as u32,
            weekday: t.wDayOfWeek as u32,
            hour: t.wHour as u32,
            minute: t.wMinute as u32,
            second: t.wSecond as u32,
        }
    }
    pub fn ymd(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
    pub fn days_in_month(&self) -> u32 {
        days_in_month(self.year, self.month)
    }
    pub fn day_of_year(&self) -> u32 {
        (1..self.month).map(|m| days_in_month(self.year, m)).sum::<u32>() + self.day
    }
    pub fn days_in_year(&self) -> u32 {
        if leap(self.year) {
            366
        } else {
            365
        }
    }
    /// 0 = Sunday, for the 1st of this month.
    pub fn first_weekday(&self) -> u32 {
        (self.weekday + 35 - ((self.day - 1) % 7)) % 7
    }
}

fn leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
pub fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if leap(y) {
                29
            } else {
                28
            }
        }
    }
}
pub const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Everything widgets read, borrowed for one paint.
pub struct Ctx<'a> {
    pub hub: &'a Hub,
    pub cfg: &'a Config,
    pub weather: Option<&'a Weather>,
    pub stats: Option<&'a Stats>,
    pub control: &'a ControlState,
    /// Launchpad's pinned apps (Apps widget) and display names.
    pub pinned: &'a [String],
    pub names: &'a HashMap<String, String>,
    /// WhatsApp: None = not running, Some(true) = has asked for attention.
    pub whatsapp: Option<bool>,
    /// Tray icons (key, name) for Background Apps, and the one under the pointer.
    pub tray: &'a [(u64, String)],
    pub hover_tray: Option<u64>,
    /// Windows Update status (None until checked).
    pub update: Option<&'a laminar_sys::update::UpdateStatus>,
    /// Use made-up data for connectors that aren't set up (gallery previews).
    pub sample: bool,
    pub now: Now,
}

/// Something a click on a widget does.
#[derive(Clone, Debug, PartialEq)]
pub enum WAct {
    /// Open a URL, a URI (ms-settings:…) or a program.
    Open(String),
    /// Show the connect form for a connector.
    Connect(&'static str),
    Search,
    Timer(TimerKind),
    TimerReset(TimerKind),
    Wifi,
    Bluetooth,
    Dark,
    Mute,
    Mic,
    KeepAwake,
    ShowDesktop,
    Language,
    MediaPrev,
    MediaPlay,
    MediaNext,
    /// Set from the click position inside the hit rectangle (0..1 across it).
    Volume,
    Brightness,
    SetWallpaper,
    App(String),
    File(String),
    Notes,
    WhatsApp,
    /// Show the Wi-Fi network list.
    WifiMenu,
    Snip,
    Lock,
    /// The power menu (Restart for updates).
    Power,
    /// A tray icon (left click; right click is handled by the host).
    Tray(u64),
}

/// Clickable areas (logical px inside the widget) and app icons the host draws on top.
#[derive(Default, Clone)]
pub struct Painted {
    pub hits: Vec<((f32, f32, f32, f32), WAct)>,
    /// (x, y, size, app id)
    pub icons: Vec<(f32, f32, f32, String)>,
}

impl Painted {
    /// The action at a point (topmost first), else None.
    pub fn at(&self, x: f32, y: f32) -> Option<(WAct, f32)> {
        self.hits.iter().rev().find(|(r, _)| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3).map(|(r, a)| (a.clone(), ((x - r.0) / r.2).clamp(0.0, 1.0)))
    }
}

/// Paint a widget into `surface` (physical size w×h px at `scale`). `dense`: the compact
/// dock version. Returns its clickable areas in logical px.
pub fn render(gfx: &Gfx, surface: &CompositionDrawingSurface, def: &Def, ctx: &Ctx, w: f32, h: f32, scale: f32, dense: bool) -> Result<Painted> {
    let mut out = Painted::default();
    gfx.draw(surface, |dc| {
        let c = canvas::Canvas::new(dc, &gfx.factory, &gfx.dwrite, w / scale, h / scale, scale, crate::ui::theme::light());
        out = paint::paint(&c, def, ctx, dense, &gfx.wic);
        Ok(())
    })?;
    Ok(out)
}

/// Width in the dock for a widget of `size` at icon size `b`.
pub fn dock_width(def: &Def, size: Size, b: f32) -> f32 {
    // One width for medium widgets (narrower ones truncated their labels and looked uneven).
    let medium = def.dock_w.max(2.6);
    (match size {
        Size::Small => 1.0,
        Size::Medium => medium,
        Size::Large => (medium * 1.6).max(3.6),
    } * b)
        .round()
}

/// The on-this-PC settings form a widget uses (launchpad_widgets::LOCAL_FORMS), with the
/// menu label that opens it: weather location, world-clock cities, revenue goal.
pub fn local_form(def: &Def) -> Option<(&'static str, &'static str)> {
    use catalog::{ClockKind, Face};
    match def.face {
        Face::Weather(_) => Some(("weather", "Change Location…")),
        Face::Clock(ClockKind::World | ClockKind::WorldTime) => Some(("clocks", "Choose Cities…")),
        Face::Goal => Some(("revenue-goal", "Set Goal…")),
        _ => None,
    }
}

/// The connectors a widget needs fetched.
pub fn conns(def: &Def) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = def.face.conn().into_iter().collect();
    v.extend(def.face.extra_conns());
    v
}

// ------------------------------------------------------------------ formatting

/// "1,234", "12.4K", "1.2M".
pub fn compact(v: f64) -> String {
    let a = v.abs();
    if a >= 1e9 {
        format!("{:.1}B", v / 1e9)
    } else if a >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else if a >= 1e5 {
        format!("{:.0}K", v / 1e3)
    } else if a >= 1e4 {
        format!("{:.1}K", v / 1e3)
    } else {
        grouped(v.round() as i64)
    }
}

pub fn grouped(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Money (docs/widget-design.md §2.2): cents only under 100 ("$83.46"), whole amounts to
/// 9,999 ("$5,336"), compact from 10,000 ("$17.5K", "$1.2M").
pub fn money(v: f64, currency: Option<&str>) -> String {
    let sym = match currency.unwrap_or("USD") {
        "USD" | "CAD" | "AUD" | "NZD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        "INR" => "₹",
        other => return format!("{} {other}", compact(v)),
    };
    let a = v.abs();
    let body = if a >= 10_000.0 {
        compact(a)
    } else if a >= 100.0 {
        grouped(a.round() as i64)
    } else {
        format!("{a:.2}")
    };
    format!("{}{sym}{body}", if v < 0.0 { "-" } else { "" })
}

/// The headline of a feed as text.
pub fn headline(f: &laminar_sys::feed::Feed) -> String {
    if let Some(v) = f.value {
        if f.currency.is_some() {
            return money(v, f.currency.as_deref());
        }
        if f.unit == "%" {
            return format!("{v:.0}%");
        }
        return compact(v);
    }
    f.text.clone().unwrap_or_else(|| "—".into())
}

pub fn hms(secs: f32) -> String {
    let s = secs.max(0.0).round() as u32;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn dur(secs: u64) -> String {
    let (h, m) = (secs / 3600, secs / 60 % 60);
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Time in a Windows time zone ("Tokyo Standard Time"), as (hour, minute, second, weekday).
pub fn tz_now(key: &str) -> Option<(u32, u32, u32, u32)> {
    use windows::Win32::System::Time::*;
    unsafe {
        let mut i = 0u32;
        loop {
            let mut z = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if EnumDynamicTimeZoneInformation(i, &mut z) != 0 {
                return None;
            }
            let name = String::from_utf16_lossy(&z.TimeZoneKeyName[..z.TimeZoneKeyName.iter().position(|&c| c == 0).unwrap_or(z.TimeZoneKeyName.len())]);
            if name.eq_ignore_ascii_case(key) {
                let utc = windows::Win32::System::SystemInformation::GetSystemTime();
                let mut local = windows::Win32::Foundation::SYSTEMTIME::default();
                SystemTimeToTzSpecificLocalTimeEx(Some(&z), &utc, &mut local).ok()?;
                return Some((local.wHour as u32, local.wMinute as u32, local.wSecond as u32, local.wDayOfWeek as u32));
            }
            i += 1;
        }
    }
}

/// World clock cities: (label, Windows time-zone key), from settings or defaults.
pub fn world_cities(cfg: &Config) -> Vec<(String, String)> {
    let raw = cfg.widget_settings.get("clocks").and_then(|m| m.get("cities")).cloned().unwrap_or_default();
    let mut v: Vec<(String, String)> = raw
        .split(';')
        .filter_map(|p| {
            let (l, k) = p.split_once('=')?;
            Some((l.trim().to_string(), k.trim().to_string()))
        })
        .filter(|(l, k)| !l.is_empty() && !k.is_empty())
        .collect();
    if v.is_empty() {
        v = vec![
            ("San Francisco".into(), "Pacific Standard Time".into()),
            ("London".into(), "GMT Standard Time".into()),
            ("Tokyo".into(), "Tokyo Standard Time".into()),
        ];
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_cards_pair_up() {
        let (r, h) = pack(&[Size::Small, Size::Small, Size::Medium, Size::Small, Size::Large], 268.0);
        // Two smalls share the first row, side by side.
        assert_eq!(r[0].1, r[1].1);
        assert!(r[1].0 > r[0].0);
        // The medium starts a new row below them; a lone small then gets its own row.
        assert!(r[2].1 >= r[0].1 + SMALL_H);
        assert!(r[3].1 > r[2].1 && r[3].0 == 0.0);
        assert!(r[4].1 > r[3].1);
        assert!(h >= r[4].1 + CARD_H * 2.0);
    }

    #[test]
    fn every_widget_has_a_unique_id() {
        let mut ids: Vec<&str> = catalog::all().map(|d| d.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
}
