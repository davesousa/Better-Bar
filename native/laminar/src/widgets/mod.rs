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

use std::collections::HashMap;

use laminar_sys::stats::Stats;
use laminar_sys::weather::Weather;
use windows::core::Result;
use windows::UI::Composition::CompositionDrawingSurface;

use crate::config::Config;
use crate::gfx::Gfx;
use crate::services::ControlState;
pub use catalog::{Def, TimerKind};
pub use hub::Hub;

/// Logical height of a Launchpad widget card (a tall one is twice this plus a gap).
pub const CARD_H: f32 = 96.0;
pub const GAP: f32 = 12.0;

pub fn card_h(def: &Def) -> f32 {
    if def.tall {
        CARD_H * 2.0 + GAP
    } else {
        CARD_H
    }
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
    Launchpad,
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

/// Width in the dock for a widget at icon size `b`.
pub fn dock_width(def: &Def, b: f32) -> f32 {
    (def.dock_w * b).round()
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

/// Money: "$5,336", "$83.46", "$17.4K".
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
    let body = if a >= 100_000.0 {
        compact(a)
    } else if a >= 1000.0 {
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
