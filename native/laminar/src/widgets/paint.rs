//! How each widget face draws. All coordinates are logical pixels inside the widget.
//! `dense` is the compact version for the dock (one icon tall).

use std::borrow::Cow;

use laminar_sys::feed::{Feed, FeedError};
use windows::Win32::Graphics::Imaging::IWICBitmapSource;

use super::canvas::{alpha, hex, Align, Badge, Canvas, Col, Font};
use super::catalog::*;
use super::{compact, dur, headline, hms, money, Ctx, Painted, WAct, DAYS, MONTHS};

const BLUE: Col = hex(0x0A84FF);
const GREEN: Col = hex(0x30D158);
const RED: Col = hex(0xFF453A);
const ORANGE: Col = hex(0xFF9F0A);
const PURPLE: Col = hex(0xBF5AF2);
const YELLOW: Col = hex(0xFFD60A);

// Segoe Fluent Icons
const G_PLAY: &str = "\u{E768}";
const G_PAUSE: &str = "\u{E769}";
const G_PREV: &str = "\u{E892}";
const G_NEXT: &str = "\u{E893}";
const G_RESET: &str = "\u{E72C}";
const G_WIFI: &str = "\u{E701}";
const G_BT: &str = "\u{E702}";
const G_MOON: &str = "\u{E708}";
const G_SUN: &str = "\u{E706}";
const G_VOL: &str = "\u{E767}";
const G_MUTE: &str = "\u{E74F}";
const G_MIC: &str = "\u{E720}";
const G_UP: &str = "\u{E70E}";
const G_BELL: &str = "\u{EA8F}";
const G_DESKTOP: &str = "\u{E7F4}";
const G_SEARCH: &str = "\u{E721}";
const G_RUN: &str = "\u{E756}";
const G_TRASH: &str = "\u{E74D}";
const G_NOTE: &str = "\u{E70B}";
const G_CHAT: &str = "\u{E8BD}";
const G_KEYBOARD: &str = "\u{E765}";
const G_MUSIC: &str = "\u{E8D6}";
const G_CLOCK: &str = "\u{E823}";
const G_PLUG: &str = "\u{E83E}";

/// Data for a connector face: its feed, or why there isn't one.
enum Got<'a> {
    Feed(Cow<'a, Feed>),
    Setup(String),
    Loading,
    Error(String),
}

fn got<'a>(x: &'a Ctx, conn: &str) -> Got<'a> {
    match (x.hub.feed(conn), x.hub.err(conn)) {
        (Some(f), _) if !x.sample || x.hub.err(conn).is_none() => Got::Feed(Cow::Borrowed(f)),
        _ if x.sample => Got::Feed(Cow::Owned(super::hub::sample(conn))),
        (_, Some(FeedError::NeedsSetup(m))) => Got::Setup(m.clone()),
        (_, Some(FeedError::Unauthorized(m))) => Got::Setup(m.clone()),
        (None, Some(FeedError::Failed(m))) => Got::Error(m.clone()),
        (Some(f), _) => Got::Feed(Cow::Borrowed(f)),
        (None, None) => Got::Loading,
    }
}

fn bg(c: &Canvas, dense: bool) {
    let r = if dense { 12.0 } else { 18.0 };
    c.rr(0.0, 0.0, c.w, c.h, r, c.card());
}

fn up_down(d: f64) -> Col {
    if d >= 0.0 {
        GREEN
    } else {
        RED
    }
}

fn delta_str(d: f64) -> String {
    format!("{}{:.1}%", if d >= 0.0 { "▲ " } else { "▼ " }, d.abs())
}

/// The whole-card fallback states for connector faces.
fn placeholder(c: &Canvas, out: &mut Painted, name: &str, badge: &Badge, conn: &'static str, g: &Got, dense: bool) -> bool {
    let (title, sub, connect) = match g {
        Got::Feed(_) => return false,
        Got::Loading => (name.to_string(), "Loading…".to_string(), false),
        Got::Setup(m) => (name.to_string(), m.clone(), true),
        Got::Error(m) => (name.to_string(), m.clone(), false),
    };
    if dense {
        let s = c.h * 0.56;
        c.badge(badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        let x = c.h;
        c.tw(&title, x, c.h * 0.2, (c.h * 0.24).max(10.0), 600, c.text1(), c.w - x - 6.0);
        c.tw(if connect { "Click to connect" } else { &sub }, x, c.h * 0.54, (c.h * 0.2).max(9.5), 400, c.text2(), c.w - x - 6.0);
    } else {
        c.badge(badge, 16.0, 16.0, 34.0);
        c.tw(&title, 62.0, 18.0, 14.0, 600, c.text1(), c.w - 78.0);
        c.tw(&sub, 62.0, 38.0, 11.5, 400, c.text2(), c.w - 78.0);
        if connect {
            let (bw, bh) = (86.0, 26.0);
            let (bx, by) = (c.w - bw - 14.0, c.h - bh - 12.0);
            c.rr(bx, by, bw, bh, bh / 2.0, BLUE);
            c.tc("Connect", bx + bw / 2.0, by + 5.0, 12.0, 600, (1.0, 1.0, 1.0, 1.0), bw);
        }
    }
    if connect {
        out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Connect(conn)));
    }
    true
}

pub fn paint(c: &Canvas, def: &Def, x: &Ctx, dense: bool, wic: &windows::Win32::Graphics::Imaging::IWICImagingFactory) -> Painted {
    let mut out = Painted::default();
    let custom_bg = matches!(def.face, Face::Weather(WeatherKind::Banner) | Face::Wallpaper | Face::Clock(ClockKind::DayPage) | Face::Media(MediaKind::Disc));
    if !custom_bg || dense {
        bg(c, dense);
    }
    match def.face {
        Face::Stat { conn, badge, open } => stat(c, &mut out, def, x, conn, &badge, open, dense),
        Face::Chart { conn, badge, open } => chart(c, &mut out, def, x, conn, &badge, open, dense),
        Face::Usage { conn, badge, open } => usage(c, &mut out, def, x, conn, &badge, open, dense),
        Face::Heat { conn, badge, color, open } => heat(c, &mut out, def, x, conn, &badge, hex(color), open, dense),
        Face::List { conn, badge, open } => list(c, &mut out, def, x, conn, &badge, open, dense),
        Face::Watch { conn } => watch(c, &mut out, def, x, conn, dense),
        Face::Next => next_event(c, &mut out, def, x, dense),
        Face::AiAll => ai_all(c, &mut out, x, dense),
        Face::Goal => goal(c, &mut out, x, dense),
        Face::Wallpaper => wallpaper(c, &mut out, def, x, wic, dense),
        Face::Clock(k) => clock(c, &mut out, k, x, dense),
        Face::Weather(k) => weather(c, &mut out, k, x, dense),
        Face::Timer(k) => timer(c, &mut out, k, x, dense),
        Face::Media(k) => media(c, &mut out, k, x, wic, dense),
        Face::Sys(k) => sys(c, &mut out, k, x, dense),
        Face::Tool(k) => tool(c, &mut out, k, x, dense),
        Face::Progress(s) => progress(c, s, x, dense),
    }
    out
}

fn open_hit(out: &mut Painted, c: &Canvas, open: &str) {
    if !open.is_empty() {
        out.hits.insert(0, ((0.0, 0.0, c.w, c.h), WAct::Open(open.to_string())));
    }
}

/// Compact layout used by most dock widgets: a visual on the left, value and caption.
fn dense_face(c: &Canvas, value: &str, caption: &str, value_col: Col) -> f32 {
    let x = c.h * 0.98;
    let vs = (c.h * 0.36).max(13.0);
    c.text(value, x, c.h * 0.14, vs, 600, value_col, Align::Left, c.w - x - 6.0, Font::Display);
    c.tw(caption, x, c.h * 0.14 + vs * 1.28, (c.h * 0.19).max(9.5), 400, c.text2(), c.w - x - 6.0);
    x
}

// ------------------------------------------------------------------ connector faces

#[allow(clippy::too_many_arguments)]
fn stat(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, conn: &'static str, badge: &Badge, open: &str, dense: bool) {
    let g = got(x, conn);
    if placeholder(c, out, def.name, badge, conn, &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    open_hit(out, c, open);
    let (value, caption) = if def.id == "github-streak" {
        (f.extra.get("streak").cloned().unwrap_or_else(|| "0".into()), "Day streak".to_string())
    } else {
        let cap = if f.caption.is_empty() { def.name.to_string() } else { f.caption.clone() };
        (headline(&f), cap)
    };
    if dense {
        let s = c.h * 0.56;
        c.badge(badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        dense_face(c, &value, &caption, c.text1());
        return;
    }
    c.badge(badge, c.w - 16.0 - 34.0, 16.0, 34.0);
    c.num(&value, 16.0, 14.0, 32.0, 600, c.text1());
    let cw = c.tw(&caption, 16.0, 58.0, 12.0, 400, c.text2(), c.w - 120.0);
    if let Some(d) = f.delta_pct {
        c.tw(&delta_str(d), 22.0 + cw, 58.0, 11.5, 600, up_down(d), 90.0);
    }
    if let Some(n) = f.extra.get("active_subscriptions") {
        c.tr(&format!("{n} subscribers"), c.w - 16.0, 70.0, 11.0, 400, c.text3());
    } else if def.name.contains("Mail") {
        if let Some(r) = f.rows.first() {
            c.tw(&r.label, 16.0, 74.0, 11.0, 400, c.text3(), c.w - 32.0);
        }
    }
    if f.series.len() > 2 {
        c.area(c.w - 120.0, 64.0, 104.0, 20.0, &f.series, GREEN, false);
    }
}

#[allow(clippy::too_many_arguments)]
fn chart(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, conn: &'static str, badge: &Badge, open: &str, dense: bool) {
    let g = got(x, conn);
    if placeholder(c, out, def.name, badge, conn, &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    open_hit(out, c, open);
    let col = match f.delta_pct {
        Some(d) if d < 0.0 => RED,
        _ => GREEN,
    };
    let title = match f.extra.get("symbol") {
        Some(s) => s.clone(),
        None if !f.caption.is_empty() && def.id.contains("chart") => def.name.trim_end_matches(" Chart").to_string(),
        None => def.name.to_string(),
    };
    let value = headline(&f);
    if dense {
        let s = c.h * 0.5;
        c.badge(badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        let tx = dense_face(c, &value, &title, c.text1());
        let _ = tx;
        return;
    }
    c.badge(badge, 14.0, 13.0, 20.0);
    c.tw(&title, 42.0, 14.0, 12.5, 600, c.text1(), c.w - 160.0);
    c.tr(&value, c.w - 14.0, 9.0, 16.0, 600, c.text1());
    match f.delta_pct {
        Some(d) => {
            c.tr(&delta_str(d), c.w - 14.0, 30.0, 10.5, 600, up_down(d));
        }
        None => {
            c.tr(&f.caption, c.w - 14.0, 30.0, 10.5, 400, c.text3());
        }
    }
    if f.series.len() > 1 {
        c.area(14.0, 48.0, c.w - 28.0, c.h - 60.0, &f.series, col, true);
    } else {
        c.tw("No chart data yet", 14.0, 60.0, 11.5, 400, c.text3(), c.w - 28.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn usage(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, conn: &'static str, badge: &Badge, open: &str, dense: bool) {
    let g = got(x, conn);
    if placeholder(c, out, def.name, badge, conn, &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    open_hit(out, c, open);
    let tokens = f.unit == "tokens";
    let value = if tokens { format!("{} tokens", compact(f.value.unwrap_or(0.0))) } else { headline(&f) };
    let (meter_label, frac) = if tokens {
        let w5 = f.extra.get("tokens_5h").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let wk = f.extra.get("tokens_week").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        // Share of the week's use that happened in the last 5 hours (a sense of "pace").
        (format!("5h window {} · week {}", compact(w5), compact(wk)), if wk > 0.0 { (w5 / wk) as f32 } else { 0.0 })
    } else {
        let lim = f.extra.get("total_credits").cloned().unwrap_or_default();
        (if lim.is_empty() { f.caption.clone() } else { format!("{} of ${lim} used", (f.frac.unwrap_or(0.0) * 100.0).round() as i32).replace(" of", "% of") }, f.frac.unwrap_or(0.0))
    };
    if dense {
        let s = c.h * 0.56;
        c.badge(badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        let tx = c.h * 0.98;
        let vs = (c.h * 0.3).max(12.0);
        c.text(&if tokens { compact(f.value.unwrap_or(0.0)) } else { headline(&f) }, tx, c.h * 0.16, vs, 600, c.text1(), Align::Left, c.w - tx - 6.0, Font::Display);
        c.bar(tx, c.h * 0.66, c.w - tx - 10.0, (c.h * 0.1).max(4.0), frac, hex(0xD97757));
        return;
    }
    c.badge(badge, 16.0, 16.0, 36.0);
    c.tw(def.name, 64.0, 15.0, 12.5, 600, c.text2(), c.w - 80.0);
    c.num(&value, 64.0, 32.0, 19.0, 600, c.text1());
    c.bar(16.0, 70.0, c.w - 32.0, 6.0, frac, hex(0xD97757));
    c.tw(&meter_label, 16.0, 78.0, 10.0, 400, c.text3(), c.w - 32.0);
    if tokens {
        if let Some(cost) = f.extra.get("cost_today_usd").and_then(|v| v.parse::<f64>().ok()) {
            c.tr(&format!("≈ {}", money(cost, None)), c.w - 16.0, 20.0, 11.0, 400, c.text3());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn heat(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, conn: &'static str, badge: &Badge, color: Col, open: &str, dense: bool) {
    let g = got(x, conn);
    if placeholder(c, out, def.name, badge, conn, &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    let open = match (conn, x.cfg.widget_settings.get(conn).and_then(|m| m.get("username"))) {
        ("github", Some(u)) => format!("https://github.com/{u}"),
        ("leetcode", Some(u)) => format!("https://leetcode.com/u/{u}"),
        _ => open.to_string(),
    };
    open_hit(out, c, &open);
    let (gx, gy, gw, gh) = if dense { (6.0, 6.0, c.w - 12.0, c.h - 12.0) } else { (14.0, 40.0, c.w - 28.0, c.h - 50.0) };
    if !dense {
        c.badge(badge, 14.0, 12.0, 20.0);
        c.tw(def.name, 42.0, 14.0, 12.5, 600, c.text1(), c.w - 150.0);
        let streak = f.extra.get("streak").map(|s| format!("{s}-day streak")).unwrap_or_default();
        c.tr(&format!("{}  {}", headline(&f), streak), c.w - 14.0, 15.0, 11.5, 500, c.text2());
    }
    let cell_gap = if dense { 1.5 } else { 2.5 };
    let cell = ((gh - 6.0 * cell_gap) / 7.0).max(2.0);
    let cols = (((gw + cell_gap) / (cell + cell_gap)).floor() as usize).max(1);
    let n = f.grid.len();
    let today_row = x.now.weekday as usize;
    // The last column ends today; earlier days fill backwards.
    for col in 0..cols {
        for row in 0..7 {
            let back = (cols - 1 - col) * 7 + today_row;
            if row > today_row && col == cols - 1 {
                continue;
            }
            let k = back as isize - row as isize;
            let idx = n as isize - 1 - k;
            let v = if idx >= 0 { f.grid[idx as usize] } else { 0.0 };
            let fill = if v <= 0.0 { c.ink(0.08) } else { alpha(color, 0.3 + 0.7 * v) };
            c.rr(gx + col as f32 * (cell + cell_gap), gy + row as f32 * (cell + cell_gap), cell, cell, cell * 0.25, fill);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn list(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, conn: &'static str, badge: &Badge, open: &str, dense: bool) {
    let g = got(x, conn);
    if placeholder(c, out, def.name, badge, conn, &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    open_hit(out, c, open);
    let value = if conn == "calendar" { format!("{} today", f.value.unwrap_or(0.0)) } else { headline(&f) };
    if dense {
        let s = c.h * 0.56;
        c.badge(badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        let first = f.rows.first().map(|r| r.label.clone()).unwrap_or_default();
        dense_face(c, &value, &first, c.text1());
        return;
    }
    c.badge(badge, 14.0, 12.0, 22.0);
    c.tw(def.name, 44.0, 15.0, 12.5, 600, c.text1(), c.w - 140.0);
    c.tr(&value, c.w - 14.0, 15.0, 12.0, 500, c.text2());
    let top = 44.0;
    let rows: Vec<_> = f.rows.iter().take(if def.tall { 6 } else { 3 }).collect();
    if rows.is_empty() {
        c.tw("Nothing to show", 14.0, top + 4.0, 11.5, 400, c.text3(), c.w - 28.0);
        return;
    }
    let rh = ((c.h - top - 8.0) / rows.len().max(1) as f32).min(28.0);
    for (k, r) in rows.iter().enumerate() {
        let y = top + k as f32 * rh;
        let vw = c.measure(&r.value, 11.5, 500, Font::Text).0.min(c.w * 0.45);
        c.tw(&r.label, 14.0, y, 12.0, 400, c.text1(), c.w - 40.0 - vw);
        c.tr(&r.value, c.w - 14.0, y, 11.5, 500, c.text2());
        if let Some(fr) = r.frac.filter(|v| *v > 0.0) {
            c.bar(14.0, y + 17.0, c.w - 28.0, 3.0, fr, if conn == "top_memory" { PURPLE } else { BLUE });
        }
        if conn == "recent_files" && !r.value.is_empty() {
            out.hits.push(((8.0, y - 3.0, c.w - 16.0, rh), WAct::File(r.value.clone())));
        }
    }
}

fn watch(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, conn: &'static str, dense: bool) {
    let badge = super::canvas::gly("\u{E9D2}", 0x1F2A1F, 0x34C759);
    let g = got(x, conn);
    if placeholder(c, out, def.name, &badge, conn, &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    open_hit(out, c, "https://finance.yahoo.com");
    if dense {
        if let Some(r) = f.rows.first() {
            let s = c.h * 0.5;
            c.badge(&badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
            let d = r.delta.unwrap_or(0.0);
            dense_face(c, &r.value, &format!("{} {}", r.label, delta_str(d)), c.text1());
        }
        return;
    }
    let n = f.rows.len().min(4).max(1);
    let rh = (c.h - 16.0) / n as f32;
    for (k, r) in f.rows.iter().take(4).enumerate() {
        let y = 8.0 + k as f32 * rh + (rh - 16.0) / 2.0;
        c.tw(&r.label, 16.0, y, 12.5, 600, c.text1(), 80.0);
        c.text(&r.value, c.w * 0.62, y, 12.5, 500, c.text1(), Align::Right, 120.0, Font::Text);
        let d = r.delta.unwrap_or(0.0);
        let (pw, ph) = (66.0, 18.0);
        c.rr(c.w - 16.0 - pw, y - 1.0, pw, ph, 5.0, alpha(up_down(d), 0.9));
        c.tc(&format!("{}{:.2}%", if d >= 0.0 { "+" } else { "" }, d), c.w - 16.0 - pw / 2.0, y + 1.0, 11.0, 600, (1.0, 1.0, 1.0, 1.0), pw);
    }
}

fn next_event(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, dense: bool) {
    let badge = super::canvas::gly("\u{E787}", 0xFF3B30, 0xFFFFFF);
    let g = got(x, "calendar");
    if placeholder(c, out, def.name, &badge, "calendar", &g, dense) {
        return;
    }
    let Got::Feed(f) = g else { return };
    let title = f.extra.get("next_title").cloned().unwrap_or_else(|| "Nothing scheduled".into());
    let when = f.extra.get("next_time").cloned().unwrap_or_default();
    let mins = f.extra.get("next_minutes").and_then(|m| m.parse::<i64>().ok());
    let soon = match mins {
        Some(m) if m <= 0 => "Now".to_string(),
        Some(m) if m < 60 => format!("in {m} min"),
        Some(m) if m < 24 * 60 => format!("in {} h {} min", m / 60, m % 60),
        _ => String::new(),
    };
    if dense {
        let s = c.h * 0.56;
        c.badge(&badge, (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        dense_face(c, &title, if soon.is_empty() { &when } else { &soon }, c.text1());
        return;
    }
    c.badge(&badge, 16.0, 16.0, 34.0);
    c.tw("Up next", 62.0, 16.0, 11.0, 600, RED, 100.0);
    c.tw(&title, 62.0, 32.0, 15.0, 600, c.text1(), c.w - 78.0);
    c.tw(&when, 62.0, 56.0, 11.5, 400, c.text2(), c.w - 160.0);
    if !soon.is_empty() {
        let pw = c.measure(&soon, 11.0, 600, Font::Text).0 + 16.0;
        c.rr(c.w - 14.0 - pw, 54.0, pw, 20.0, 10.0, alpha(BLUE, 0.22));
        c.tc(&soon, c.w - 14.0 - pw / 2.0, 56.0, 11.0, 600, BLUE, pw);
    }
}

fn ai_all(c: &Canvas, out: &mut Painted, x: &Ctx, dense: bool) {
    let src: [(&'static str, &str, Col); 6] = [
        ("claude_code", "Claude Code", hex(0xD97757)),
        ("codex", "Codex", hex(0x9AA0A6)),
        ("anthropic", "Claude API", hex(0xD97757)),
        ("openai", "OpenAI API", hex(0x10A37F)),
        ("openrouter", "OpenRouter", hex(0x6467F2)),
        ("cursor", "Cursor", hex(0xBBBBBB)),
    ];
    let mut rows: Vec<(&str, String, f32, Col)> = Vec::new();
    for (conn, name, col) in src {
        let f = match x.hub.feed(conn) {
            Some(f) => Cow::Borrowed(f),
            None if x.sample && (conn == "claude_code" || conn == "openai" || conn == "codex") => Cow::Owned(super::hub::sample(conn)),
            None => continue,
        };
        let v = if f.unit == "tokens" { format!("{} today", compact(f.value.unwrap_or(0.0))) } else { headline(&f) };
        rows.push((name, v, f.frac.unwrap_or(0.0), col));
    }
    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Connect("anthropic")));
    if dense {
        let s = c.h * 0.56;
        c.badge(&super::canvas::gly("\u{E99A}", 0x2C2C34, 0xD97757), (c.h - s) / 2.0, (c.h - s) / 2.0, s);
        match rows.first() {
            Some(r) => dense_face(c, &r.1, r.0, c.text1()),
            None => dense_face(c, "AI Usage", "Click to connect", c.text1()),
        };
        return;
    }
    c.tw("AI Usage", 16.0, 12.0, 12.5, 600, c.text2(), 140.0);
    if rows.is_empty() {
        c.tw("Use Claude Code or Codex on this PC, or connect an API key.", 16.0, 40.0, 11.5, 400, c.text3(), c.w - 32.0);
        return;
    }
    out.hits.clear();
    for (k, (name, v, frac, col)) in rows.iter().take(3).enumerate() {
        let y = 34.0 + k as f32 * 20.0;
        c.circle(20.0, y + 8.0, 4.0, *col);
        c.tw(name, 30.0, y, 12.0, 500, c.text1(), 110.0);
        c.bar(140.0, y + 5.0, c.w - 140.0 - 96.0, 5.0, if *frac > 0.0 { *frac } else { 0.5 }, *col);
        c.tr(v, c.w - 14.0, y, 11.5, 500, c.text2());
    }
}

fn goal(c: &Canvas, out: &mut Painted, x: &Ctx, dense: bool) {
    let goal: f64 = x.cfg.widget_settings.get("revenue-goal").and_then(|m| m.get("goal")).and_then(|v| v.parse().ok()).unwrap_or(10_000.0);
    let mut total = 0.0;
    let mut n = 0;
    for conn in ["stripe", "lemonsqueezy", "paddle", "shopify", "gumroad", "polar"] {
        let f = match x.hub.feed(conn) {
            Some(f) => Cow::Borrowed(f),
            None if x.sample && conn == "stripe" => Cow::Owned(super::hub::sample(conn)),
            None => continue,
        };
        total += f.value.unwrap_or(0.0);
        n += 1;
    }
    let frac = (total / goal) as f32;
    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Connect("stripe")));
    let (cx, cy, r) = if dense { (c.h / 2.0, c.h / 2.0, c.h * 0.3) } else { (52.0, c.h / 2.0, 30.0) };
    c.ring(cx, cy, r, if dense { 4.0 } else { 7.0 }, frac, GREEN);
    let pct = format!("{:.0}%", (frac * 100.0).min(999.0));
    if dense {
        dense_face(c, &money(total, Some("USD")), &format!("{pct} of goal"), c.text1());
        return;
    }
    c.tc(&pct, cx, cy - 8.0, 13.0, 600, c.text1(), 60.0);
    c.num(&money(total, Some("USD")), 100.0, 22.0, 26.0, 600, c.text1());
    let sub = if n == 0 { "Connect a payment provider".to_string() } else { format!("of {} goal · last 30 days", money(goal, Some("USD"))) };
    c.tw(&sub, 100.0, 58.0, 11.5, 400, c.text2(), c.w - 116.0);
}

fn wallpaper(c: &Canvas, out: &mut Painted, def: &Def, x: &Ctx, wic: &windows::Win32::Graphics::Imaging::IWICImagingFactory, dense: bool) {
    let badge = super::canvas::gly("\u{E91B}", 0x2C6BED, 0xFFFFFF);
    let g = got(x, "wallpaper");
    if let Got::Feed(f) = &g {
        let img = f.extra.get("image_path").and_then(|p| x.hub.wallpaper_img.as_ref().filter(|w| &w.0 == p).map(|w| w.1.clone()).or_else(|| load_image(wic, p)));
        let r = if dense { 12.0 } else { 18.0 };
        match img {
            Some(src) => c.image(&src, 0.0, 0.0, c.w, c.h, r),
            None => c.rr_grad(0.0, 0.0, c.w, c.h, r, hex(0x5B8DEF), hex(0x1C3D7A)),
        }
        if !dense {
            c.rr_grad(0.0, c.h * 0.45, c.w, c.h * 0.55, r, (0.0, 0.0, 0.0, 0.0), (0.0, 0.0, 0.0, 0.55));
            let title = f.extra.get("title").cloned().or_else(|| f.text.clone()).unwrap_or_else(|| "Image of the day".into());
            c.tw(&title, 14.0, c.h - 30.0, 12.5, 600, (1.0, 1.0, 1.0, 1.0), c.w - 130.0);
            let (bw, bh) = (104.0, 24.0);
            c.rr(c.w - bw - 12.0, c.h - bh - 10.0, bw, bh, 12.0, (1.0, 1.0, 1.0, 0.92));
            c.tc("Set wallpaper", c.w - 12.0 - bw / 2.0, c.h - bh - 6.0, 11.0, 600, (0.1, 0.1, 0.12, 1.0), bw);
        }
        if let Some(p) = f.extra.get("image_path") {
            if !x.sample {
                out.hits.push(((0.0, 0.0, c.w, c.h), WAct::SetWallpaper));
                let _ = p;
            }
        }
        return;
    }
    bg(c, dense);
    placeholder(c, out, def.name, &badge, "wallpaper", &g, dense);
}

/// Decode an image file (cached by the hub on the UI thread when possible).
pub fn load_image(wic: &windows::Win32::Graphics::Imaging::IWICImagingFactory, path: &str) -> Option<IWICBitmapSource> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::Foundation::GENERIC_READ;
    unsafe {
        let dec = wic.CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnLoad).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        // Scale down first: the wallpaper is 1920x1080 and the widget is small.
        let scaler = wic.CreateBitmapScaler().ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let k = (640.0 / w.max(1) as f32).min(1.0);
        scaler.Initialize(&frame, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), WICBitmapInterpolationModeFant).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmp = wic.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

pub fn load_image_bytes(wic: &windows::Win32::Graphics::Imaging::IWICImagingFactory, bytes: &[u8]) -> Option<IWICBitmapSource> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Imaging::*;
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmp = wic.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

// ------------------------------------------------------------------ clocks

fn h12(h: u32) -> u32 {
    if h % 12 == 0 {
        12
    } else {
        h % 12
    }
}
fn ampm(h: u32) -> &'static str {
    if h < 12 {
        "AM"
    } else {
        "PM"
    }
}

fn dial(c: &Canvas, cx: f32, cy: f32, r: f32, h: u32, m: u32, s: Option<u32>, dark: bool) {
    let (face, ink) = if dark { ((0.11, 0.11, 0.13, 1.0), (1.0, 1.0, 1.0, 0.95)) } else { ((0.97, 0.97, 0.98, 1.0), (0.08, 0.08, 0.1, 1.0)) };
    c.circle(cx, cy, r, face);
    for i in 0..12 {
        let a = (i as f32 * 30.0).to_radians();
        let (r0, r1) = if i % 3 == 0 { (r * 0.78, r * 0.9) } else { (r * 0.83, r * 0.9) };
        c.line(cx + r0 * a.sin(), cy - r0 * a.cos(), cx + r1 * a.sin(), cy - r1 * a.cos(), if i % 3 == 0 { r * 0.06 } else { r * 0.035 }, alpha(ink, 0.7));
    }
    let hand = |deg: f32, len: f32, t: f32, col: Col| {
        let a = deg.to_radians();
        c.line(cx - len * 0.15 * a.sin(), cy + len * 0.15 * a.cos(), cx + len * a.sin(), cy - len * a.cos(), t, col);
    };
    hand((h % 12) as f32 * 30.0 + m as f32 * 0.5, r * 0.5, r * 0.09, ink);
    hand(m as f32 * 6.0 + s.unwrap_or(0) as f32 * 0.1, r * 0.74, r * 0.065, ink);
    if let Some(s) = s {
        hand(s as f32 * 6.0, r * 0.8, r * 0.03, ORANGE);
    }
    c.circle(cx, cy, r * 0.06, ORANGE);
}

fn seg_digit(c: &Canvas, x: f32, y: f32, w: f32, h: f32, d: u32, on: Col, off: Col) {
    // a b c d e f g
    const SEGS: [u8; 10] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F];
    let m = SEGS[d as usize % 10];
    let t = w * 0.16;
    let hh = h / 2.0;
    let rects = [
        (x + t * 0.6, y, w - t * 1.2, t),
        (x + w - t, y + t * 0.6, t, hh - t * 0.9),
        (x + w - t, y + hh + t * 0.3, t, hh - t * 0.9),
        (x + t * 0.6, y + h - t, w - t * 1.2, t),
        (x, y + hh + t * 0.3, t, hh - t * 0.9),
        (x, y + t * 0.6, t, hh - t * 0.9),
        (x + t * 0.6, y + hh - t / 2.0, w - t * 1.2, t),
    ];
    for (i, r) in rects.iter().enumerate() {
        c.rr(r.0, r.1, r.2, r.3, t / 2.0, if m & (1 << i) != 0 { on } else { off });
    }
}

fn clock(c: &Canvas, out: &mut Painted, k: ClockKind, x: &Ctx, dense: bool) {
    let n = x.now;
    let time = format!("{}:{:02}", h12(n.hour), n.minute);
    let date_long = format!("{}, {} {}", DAYS[n.weekday as usize], MONTHS[n.month as usize - 1], n.day);
    let date_short = format!("{} {} {}", &DAYS[n.weekday as usize][..3], &MONTHS[n.month as usize - 1][..3], n.day);
    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open("ms-settings:dateandtime".into())));
    match k {
        ClockKind::Digital | ClockKind::ClockWeather if dense => {
            let x0 = 10.0;
            let ts = (c.h * 0.4).max(15.0);
            let tw = c.text(&time, x0, c.h * 0.12, ts, 600, c.text1(), Align::Left, c.w, Font::Display);
            c.tw(ampm(n.hour), x0 + tw + 3.0, c.h * 0.12 + ts * 0.42, ts * 0.42, 500, c.text2(), 30.0);
            let cap = if k == ClockKind::ClockWeather { x.weather.map(|w| format!("{} {:.0}°", weather_emoji(w.code, w.is_day), d0(w.temp_c))).unwrap_or(date_short) } else { date_short };
            c.tw(&cap, x0, c.h * 0.12 + ts * 1.24, (c.h * 0.19).max(9.5), 400, c.text2(), c.w - x0 - 4.0);
        }
        ClockKind::Digital => {
            let tw = c.num(&time, 16.0, 6.0, 50.0, 300, c.text1());
            c.num(&format!(":{:02}", n.second), 18.0 + tw, 30.0, 22.0, 300, c.text3());
            c.tw(ampm(n.hour), 20.0 + tw + 34.0, 34.0, 15.0, 500, c.text2(), 40.0);
            c.tw(&date_long, 18.0, 68.0, 13.0, 400, c.text2(), c.w - 36.0);
        }
        ClockKind::Flip => {
            let digits = [h12(n.hour) / 10, h12(n.hour) % 10, n.minute / 10, n.minute % 10];
            let pad = if dense { 5.0 } else { 14.0 };
            let gap = if dense { 3.0 } else { 6.0 };
            let mid = if dense { 6.0 } else { 14.0 };
            let tw = (c.w - 2.0 * pad - 3.0 * gap - mid) / 4.0;
            let th = c.h - 2.0 * pad - if dense { 0.0 } else { 0.0 };
            let mut tx = pad;
            for (i, d) in digits.iter().enumerate() {
                if i == 2 {
                    tx += mid - gap;
                }
                c.rr(tx, pad, tw, th, tw * 0.14, if c.light { (0.15, 0.15, 0.17, 1.0) } else { (0.16, 0.16, 0.19, 1.0) });
                c.text(&d.to_string(), tx + tw / 2.0, pad + th / 2.0 - th * 0.42, th * 0.62, 600, (0.97, 0.97, 0.98, 1.0), Align::Center, tw, Font::Display);
                c.line(tx, pad + th / 2.0, tx + tw, pad + th / 2.0, 1.2, (0.0, 0.0, 0.0, 0.6));
                tx += tw + gap;
            }
        }
        ClockKind::World => {
            let cities = super::world_cities(x.cfg);
            let n_c = cities.len().min(if dense { 2 } else { 3 });
            let slot = c.w / n_c as f32;
            for (i, (label, key)) in cities.iter().take(n_c).enumerate() {
                let (h, m, s, _) = super::tz_now(key).unwrap_or((n.hour, n.minute, n.second, n.weekday));
                let cx = slot * (i as f32 + 0.5);
                let r = if dense { c.h * 0.4 } else { 27.0 };
                let cy = if dense { c.h / 2.0 } else { 38.0 };
                dial(c, cx, cy, r, h, m, Some(s), h < 6 || h >= 18);
                if !dense {
                    let short: String = label.chars().take(3).collect::<String>().to_uppercase();
                    c.tc(&short, cx, 70.0, 11.0, 600, c.text2(), slot);
                }
            }
        }
        ClockKind::WeekStrip => {
            let pad = if dense { 6.0 } else { 14.0 };
            if !dense {
                c.tw(&format!("{} {}", MONTHS[n.month as usize - 1], n.year), pad, 10.0, 12.5, 600, c.text1(), c.w - 30.0);
            }
            let top = if dense { 4.0 } else { 34.0 };
            let colw = (c.w - 2.0 * pad) / 7.0;
            for i in 0..7u32 {
                let off = i as i64 - n.weekday as i64;
                let d = (n.day as i64 + off).rem_euclid(1000);
                let dim = super::days_in_month(n.year, n.month) as i64;
                let shown = if d < 1 { d + super::days_in_month(n.year, if n.month == 1 { 12 } else { n.month - 1 }) as i64 } else if d > dim { d - dim } else { d };
                let cx = pad + colw * (i as f32 + 0.5);
                let today = off == 0;
                c.tc(&DAYS[i as usize][..1], cx, top, if dense { 9.5 } else { 11.0 }, 600, if today { BLUE } else { c.text3() }, colw);
                let r = if dense { c.h * 0.2 } else { 15.0 };
                let cy = if dense { c.h * 0.64 } else { 68.0 };
                if today {
                    c.circle(cx, cy, r, BLUE);
                }
                c.tc(&shown.to_string(), cx, cy - r * 0.62, r * 0.95, if today { 700 } else { 500 }, if today { (1.0, 1.0, 1.0, 1.0) } else { c.text1() }, colw);
            }
        }
        ClockKind::WorldTime => {
            let cities = super::world_cities(x.cfg);
            let (label, key) = cities.into_iter().nth(2).unwrap_or(("Tokyo".into(), "Tokyo Standard Time".into()));
            let (h, m, _, wd) = super::tz_now(&key).unwrap_or((n.hour, n.minute, 0, n.weekday));
            let day = if wd == n.weekday { "Today" } else if wd == (n.weekday + 1) % 7 { "Tomorrow" } else { "Yesterday" };
            let diff = {
                let mut d = (h as i32 * 60 + m as i32) - (n.hour as i32 * 60 + n.minute as i32);
                if wd == (n.weekday + 1) % 7 {
                    d += 24 * 60;
                } else if wd != n.weekday {
                    d -= 24 * 60;
                }
                d as f32 / 60.0
            };
            if dense {
                c.glyph(G_CLOCK, c.h / 2.0, c.h / 2.0, c.h * 0.36, c.text2());
                dense_face(c, &format!("{}:{:02}", h12(h), m), &label, c.text1());
                return;
            }
            c.circle(22.0, 24.0, 5.0, if (6..18).contains(&h) { YELLOW } else { hex(0x8E8EFF) });
            c.tw(&label, 34.0, 15.0, 13.0, 600, c.text1(), c.w * 0.5);
            c.tw(&format!("{day}, {}{:.0}h", if diff >= 0.0 { "+" } else { "" }, diff), 16.0, 40.0, 11.5, 400, c.text2(), c.w * 0.5);
            let tw = c.measure(ampm(h), 14.0, 500, Font::Text).0;
            c.text(&format!("{}:{:02}", h12(h), m), c.w - 22.0 - tw, 14.0, 40.0, 300, c.text1(), Align::Right, c.w, Font::Display);
            c.tr(ampm(h), c.w - 16.0, 36.0, 14.0, 500, c.text2());
        }
        ClockKind::Today => {
            let next = x.hub.feed("calendar").and_then(|f| f.extra.get("next_title").map(|t| format!("{t} · {}", f.extra.get("next_time").cloned().unwrap_or_default())));
            if dense {
                c.tw(&DAYS[n.weekday as usize].to_uppercase(), 10.0, c.h * 0.12, (c.h * 0.18).max(9.0), 700, RED, c.w);
                c.num(&format!("{} {}", &MONTHS[n.month as usize - 1][..3], n.day), 10.0, c.h * 0.34, c.h * 0.36, 600, c.text1());
                return;
            }
            c.tw(&DAYS[n.weekday as usize].to_uppercase(), 16.0, 12.0, 11.5, 700, RED, 160.0);
            c.num(&format!("{} {}", MONTHS[n.month as usize - 1], n.day), 16.0, 28.0, 26.0, 600, c.text1());
            c.tr(&format!("{} {}", time, ampm(n.hour)), c.w - 16.0, 14.0, 13.0, 500, c.text2());
            c.tw(&next.unwrap_or_else(|| "No more events today".into()), 16.0, 68.0, 11.5, 400, c.text2(), c.w - 32.0);
        }
        ClockKind::Date => {
            if dense {
                c.tw(DAYS[n.weekday as usize], 10.0, c.h * 0.12, (c.h * 0.19).max(9.5), 400, c.text2(), c.w);
                c.num(&format!("{} {}", &MONTHS[n.month as usize - 1][..3], n.day), 10.0, c.h * 0.36, c.h * 0.36, 600, c.text1());
                return;
            }
            c.tw(DAYS[n.weekday as usize], 18.0, 18.0, 13.0, 400, c.text2(), c.w);
            c.num(&format!("{} {}", MONTHS[n.month as usize - 1], n.day), 18.0, 36.0, 30.0, 600, c.text1());
        }
        ClockKind::DayPage => {
            let s = c.h.min(c.w) - if dense { 6.0 } else { 16.0 };
            let (x0, y0) = ((c.w - s) / 2.0, (c.h - s) / 2.0);
            c.rr(x0, y0, s, s, s * 0.2, (0.98, 0.98, 0.99, 1.0));
            let hh = s * 0.28;
            c.rr(x0, y0, s, hh + s * 0.2, s * 0.2, RED);
            c.rr(x0, y0 + hh, s, s * 0.2, 0.0, (0.98, 0.98, 0.99, 1.0));
            c.tc(&MONTHS[n.month as usize - 1][..3].to_uppercase(), x0 + s / 2.0, y0 + hh * 0.18, hh * 0.62, 700, (1.0, 1.0, 1.0, 1.0), s);
            c.text(&n.day.to_string(), x0 + s / 2.0, y0 + hh + s * 0.02, s * 0.5, 600, (0.1, 0.1, 0.12, 1.0), Align::Center, s, Font::Display);
        }
        ClockKind::Analog => {
            let r = c.h.min(c.w) / 2.0 - if dense { 4.0 } else { 10.0 };
            dial(c, c.w / 2.0, c.h / 2.0, r, n.hour, n.minute, Some(n.second), !c.light);
        }
        ClockKind::Segment => {
            let on = RED;
            let off = c.ink(0.06);
            let pad = if dense { 6.0 } else { 16.0 };
            let top = if dense { pad } else { 26.0 };
            let dh = c.h - top - pad;
            let dw = dh * 0.55;
            let colon = dw * 0.45;
            let total = 4.0 * dw + 3.0 * dw * 0.25 + colon;
            let mut sx = (c.w - total) / 2.0;
            if !dense {
                c.tw(ampm(n.hour), 16.0, 8.0, 10.5, 700, RED, 40.0);
                c.tr(&date_short.to_uppercase(), c.w - 16.0, 8.0, 10.5, 700, c.text3());
            }
            let d = [h12(n.hour) / 10, h12(n.hour) % 10, n.minute / 10, n.minute % 10];
            for (i, v) in d.iter().enumerate() {
                if i == 2 {
                    let blink = if n.second % 2 == 0 { on } else { off };
                    c.circle(sx + colon / 2.0 - dw * 0.12, top + dh * 0.3, dw * 0.09, blink);
                    c.circle(sx + colon / 2.0 - dw * 0.12, top + dh * 0.7, dw * 0.09, blink);
                    sx += colon;
                }
                if !(i == 0 && *v == 0) {
                    seg_digit(c, sx, top, dw, dh, *v, on, off);
                } else {
                    seg_digit(c, sx, top, dw, dh, 8, off, off);
                }
                sx += dw * 1.25;
            }
        }
        ClockKind::ClockWeather => {
            let tw = c.num(&time, 16.0, 12.0, 40.0, 400, c.text1());
            c.tw(ampm(n.hour), 20.0 + tw, 34.0, 13.0, 500, c.text2(), 40.0);
            c.tw(&date_long, 18.0, 66.0, 12.0, 400, c.text2(), c.w * 0.6);
            if let Some(w) = x.weather {
                c.text(weather_emoji(w.code, w.is_day), c.w - 60.0, 16.0, 28.0, 400, c.text1(), Align::Center, 60.0, Font::Text);
                c.tr(&format!("{:.0}°", d0(w.temp_c)), c.w - 16.0, 56.0, 20.0, 500, c.text1());
            }
        }
        ClockKind::Month => {
            let pad = 14.0;
            c.tw(MONTHS[n.month as usize - 1], pad + 2.0, 12.0, 14.0, 600, c.text1(), 160.0);
            c.tr(&n.year.to_string(), c.w - pad - 2.0, 12.0, 14.0, 600, RED);
            let colw = (c.w - 2.0 * pad) / 7.0;
            for (i, d) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
                c.tc(d, pad + colw * (i as f32 + 0.5), 40.0, 10.5, 600, c.text3(), colw);
            }
            let first = n.first_weekday();
            let rows = (first + n.days_in_month()).div_ceil(7) as f32;
            let rh = ((c.h - 62.0 - 6.0) / rows).min(26.0);
            for day in 1..=n.days_in_month() {
                let idx = first + day - 1;
                let cx = pad + colw * ((idx % 7) as f32 + 0.5);
                let cy = 62.0 + rh * (idx / 7) as f32;
                let today = day == n.day;
                if today {
                    c.circle(cx, cy + 8.0, 11.0, RED);
                }
                c.tc(&day.to_string(), cx, cy, 12.0, if today { 700 } else { 400 }, if today { (1.0, 1.0, 1.0, 1.0) } else { c.text1() }, colw);
            }
        }
    }
}

// ------------------------------------------------------------------ weather

pub fn weather_emoji(code: i32, day: bool) -> &'static str {
    match code {
        0 => {
            if day {
                "☀️"
            } else {
                "🌙"
            }
        }
        1 => {
            if day {
                "🌤️"
            } else {
                "🌙"
            }
        }
        2 => "⛅",
        3 => "☁️",
        45 | 48 => "🌫️",
        51..=57 | 80..=82 => "🌦️",
        61..=67 => "🌧️",
        71..=77 | 85 | 86 => "🌨️",
        95..=99 => "⛈️",
        _ => "🌡️",
    }
}

fn weather(c: &Canvas, out: &mut Painted, k: WeatherKind, x: &Ctx, dense: bool) {
    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open(format!("https://www.msn.com/en-ca/weather/forecast/in-{}", x.cfg.weather_city.replace(' ', "-")))));
    let Some(w) = x.weather else {
        if k == WeatherKind::Banner && !dense {
            bg(c, false);
        }
        if dense {
            c.glyph(G_SUN, c.h / 2.0, c.h / 2.0, c.h * 0.4, c.text3());
            dense_face(c, "—", &x.cfg.weather_city, c.text1());
        } else {
            c.tw(&x.cfg.weather_city, 16.0, 16.0, 13.0, 600, c.text1(), c.w);
            c.tw("Fetching weather…", 16.0, 40.0, 12.0, 400, c.text2(), c.w);
        }
        return;
    };
    let city = &x.cfg.weather_city;
    let temp = format!("{:.0}°", d0(w.temp_c));
    let emoji = weather_emoji(w.code, w.is_day);
    if dense {
        c.text(emoji, c.h / 2.0, c.h * 0.2, c.h * 0.42, 400, c.text1(), Align::Center, c.h, Font::Text);
        dense_face(c, &temp, &w.description, c.text1());
        return;
    }
    match k {
        WeatherKind::Now => {
            c.text(emoji, 46.0, 18.0, 40.0, 400, c.text1(), Align::Center, 80.0, Font::Text);
            c.num(&temp, 90.0, 10.0, 40.0, 300, c.text1());
            c.tw(&format!("H:{:.0}°  L:{:.0}°", d0(w.high_c), d0(w.low_c)), 92.0, 62.0, 12.0, 400, c.text2(), 140.0);
            c.tr(city, c.w - 16.0, 16.0, 12.5, 600, c.text1());
            c.tr(&w.description, c.w - 16.0, 36.0, 11.5, 400, c.text2());
        }
        WeatherKind::Detail => {
            c.text(emoji, 34.0, 16.0, 30.0, 400, c.text1(), Align::Center, 60.0, Font::Text);
            c.num(&temp, 62.0, 10.0, 34.0, 300, c.text1());
            c.tw(&w.description, 18.0, 62.0, 12.0, 400, c.text2(), c.w * 0.5);
            let rows = [("Feels like", format!("{:.0}°", d0(w.feels_c))), ("Humidity", format!("{:.0}%", w.humidity)), ("Wind", format!("{:.0} km/h", w.wind_kmh)), ("UV", format!("{:.0}", w.uv))];
            for (i, (l, v)) in rows.iter().enumerate() {
                let y = 12.0 + i as f32 * 18.0;
                c.tw(l, c.w * 0.55, y, 11.0, 400, c.text3(), 80.0);
                c.tr(v, c.w - 16.0, y, 11.5, 600, c.text1());
            }
        }
        WeatherKind::Hourly | WeatherKind::Week => {
            c.tw(city, 16.0, 10.0, 12.0, 600, c.text1(), c.w * 0.5);
            c.tr(&format!("{temp}  {}", w.description), c.w - 16.0, 10.0, 11.5, 400, c.text2());
            let items: Vec<(String, &str, String)> = if k == WeatherKind::Hourly {
                w.hourly.iter().take(6).enumerate().map(|(i, (h, t, code))| (if i == 0 { "Now".to_string() } else { format!("{}{}", h12(*h as u32), if *h < 12 { "a" } else { "p" }) }, weather_emoji(*code, (6..19).contains(h)), format!("{:.0}°", d0(*t)))).collect()
            } else {
                w.daily.iter().take(7).enumerate().map(|(i, (d, hi, lo, code))| (if i == 0 { "Today".to_string() } else { weekday_of(d) }, weather_emoji(*code, true), format!("{:.0}° {:.0}°", d0(*hi), d0(*lo)))).collect()
            };
            let n = items.len().max(1);
            let colw = (c.w - 24.0) / n as f32;
            for (i, (l, e, t)) in items.iter().enumerate() {
                let cx = 12.0 + colw * (i as f32 + 0.5);
                c.tc(l, cx, 32.0, 10.5, 500, c.text2(), colw);
                c.text(e, cx, 47.0, 16.0, 400, c.text1(), Align::Center, colw, Font::Text);
                c.tc(t, cx, 72.0, if k == WeatherKind::Week { 10.0 } else { 12.0 }, 600, c.text1(), colw);
            }
        }
        WeatherKind::Banner => {
            let (top, bottom) = if w.is_day { (hex(0x4A90E2), hex(0x2D6CC0)) } else { (hex(0x1B2550), hex(0x0E1430)) };
            c.rr_grad(0.0, 0.0, c.w, c.h, 18.0, top, bottom);
            let white = (1.0, 1.0, 1.0, 1.0);
            c.tw(&w.description, 18.0, 18.0, 16.0, 600, white, c.w * 0.55);
            c.tw(city, 18.0, 42.0, 12.0, 400, (1.0, 1.0, 1.0, 0.8), c.w * 0.55);
            c.tw(&format!("H:{:.0}°  L:{:.0}°", d0(w.high_c), d0(w.low_c)), 18.0, 64.0, 11.5, 400, (1.0, 1.0, 1.0, 0.8), c.w * 0.55);
            c.text(&temp, c.w - 70.0, 18.0, 44.0, 300, white, Align::Right, c.w, Font::Display);
            c.text(emoji, c.w - 40.0, 44.0, 30.0, 400, white, Align::Center, 60.0, Font::Text);
        }
        WeatherKind::Sun => {
            let (rise, set) = (mins(&w.sunrise), mins(&w.sunset));
            let now_m = (x.now.hour * 60 + x.now.minute) as f32;
            let frac = if set > rise { ((now_m - rise) / (set - rise)).clamp(0.0, 1.0) } else { 0.5 };
            let (cx, cy, r) = (c.w / 2.0, c.h - 22.0, (c.w * 0.32).min(56.0));
            c.arc(cx, cy, r, 2.0, 180.0, 360.0, c.ink(0.2));
            c.arc(cx, cy, r, 2.5, 180.0, 180.0 + 180.0 * frac, ORANGE);
            let a = (180.0 + 180.0 * frac).to_radians();
            c.circle(cx + r * a.cos(), cy + r * a.sin(), 6.0, YELLOW);
            c.line(16.0, cy, c.w - 16.0, cy, 1.0, c.ink(0.15));
            c.tw(&format!("↑ {}", w.sunrise), 16.0, cy - 18.0, 11.5, 500, c.text2(), 80.0);
            c.tr(&format!("{} ↓", w.sunset), c.w - 16.0, cy - 18.0, 11.5, 500, c.text2());
            c.tw("Sunrise & Sunset", 16.0, 10.0, 11.5, 600, c.text2(), 140.0);
            let left = (set - now_m).max(0.0) as u64 * 60;
            c.tr(&if left > 0 { format!("{} of daylight left", dur(left)) } else { "After sunset".into() }, c.w - 16.0, 10.0, 11.0, 400, c.text3());
        }
    }
}

/// Rounded temperature without a "-0".
fn d0(t: f32) -> f32 {
    let r = t.round();
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

fn mins(hm: &str) -> f32 {
    let (h, m) = hm.split_once(':').unwrap_or(("0", "0"));
    h.parse::<f32>().unwrap_or(0.0) * 60.0 + m.parse::<f32>().unwrap_or(0.0)
}

fn weekday_of(ymd: &str) -> String {
    // Sakamoto's algorithm.
    let p: Vec<i32> = ymd.split('-').filter_map(|s| s.parse().ok()).collect();
    if p.len() != 3 {
        return String::new();
    }
    let (mut y, m, d) = (p[0], p[1], p[2]);
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if m < 3 {
        y -= 1;
    }
    let wd = (y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d).rem_euclid(7);
    DAYS[wd as usize][..3].to_string()
}

// ------------------------------------------------------------------ timers

fn timer(c: &Canvas, out: &mut Painted, k: TimerKind, x: &Ctx, dense: bool) {
    let (col, label, text, running, frac) = match k {
        TimerKind::Stopwatch => {
            let e = x.hub.stopwatch.elapsed().as_secs_f32();
            (ORANGE, "Stopwatch".to_string(), hms(e), x.hub.stopwatch.start.is_some(), None)
        }
        _ => {
            let t = &x.hub.timers[&k];
            let left = t.left();
            let label = match (k, t.phase, t.done) {
                (_, _, true) => "Done!".to_string(),
                (TimerKind::Pomodoro, 0, _) => "Focus".to_string(),
                (TimerKind::Pomodoro, _, _) => "Break".to_string(),
                _ => "Timer".to_string(),
            };
            (if k == TimerKind::Pomodoro { RED } else { BLUE }, label, hms(left), t.running(), Some(1.0 - left / t.total.max(1) as f32))
        }
    };
    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Timer(k)));
    if running || x.hub.timers.get(&k).map(|t| t.done).unwrap_or(false) {
        c.rr_stroke(0.0, 0.0, c.w, c.h, if dense { 12.0 } else { 18.0 }, alpha(col, 0.9), 1.6);
    }
    if dense {
        let r = c.h * 0.3;
        let (cx, cy) = (c.h / 2.0, c.h / 2.0);
        match frac {
            Some(f) => c.ring(cx, cy, r, 3.5, f, col),
            None => c.circle(cx, cy, r, alpha(col, 0.9)),
        }
        c.glyph(if running { G_PAUSE } else { G_PLAY }, cx, cy, r * 0.8, if frac.is_some() { col } else { (1.0, 1.0, 1.0, 1.0) });
        dense_face(c, &text, &label, c.text1());
        return;
    }
    c.tw(&label, 18.0, 16.0, 12.0, 600, col, 140.0);
    c.text(&text, 18.0, 34.0, 38.0, 400, c.text1(), Align::Left, c.w * 0.6, Font::Display);
    let r = 22.0;
    let (bx, by) = (c.w - 18.0 - r, c.h / 2.0);
    c.circle(bx, by, r, col);
    c.glyph(if running { G_PAUSE } else { G_PLAY }, bx, by, 16.0, (1.0, 1.0, 1.0, 1.0));
    let (rx, ry) = (bx - r - 26.0, by);
    c.circle(rx, ry, 15.0, c.ink(0.12));
    c.glyph(G_RESET, rx, ry, 12.0, c.text1());
    out.hits.push(((rx - 16.0, ry - 16.0, 32.0, 32.0), WAct::TimerReset(k)));
}

// ------------------------------------------------------------------ media

fn art(c: &Canvas, x: &Ctx, wic: &windows::Win32::Graphics::Imaging::IWICImagingFactory, ax: f32, ay: f32, s: f32, r: f32) {
    let src = x.hub.art_img.as_ref().map(|a| a.1.clone()).or_else(|| x.control.media.as_ref().and_then(|m| m.thumbnail_png.as_ref()).and_then(|b| load_image_bytes(wic, b)));
    match src {
        Some(img) => c.image(&img, ax, ay, s, s, r),
        None => {
            c.rr_grad(ax, ay, s, s, r, hex(0xFF5F6D), hex(0xC2185B));
            c.glyph(G_MUSIC, ax + s / 2.0, ay + s / 2.0, s * 0.42, (1.0, 1.0, 1.0, 0.95));
        }
    }
}

fn media(c: &Canvas, out: &mut Painted, k: MediaKind, x: &Ctx, wic: &windows::Win32::Graphics::Imaging::IWICImagingFactory, dense: bool) {
    let m = x.control.media.as_ref();
    let (title, artist, playing) = match m {
        Some(m) => (m.title.clone(), m.artist.clone(), m.playing),
        None => ("Nothing playing".to_string(), String::new(), false),
    };
    match k {
        MediaKind::Output => {
            let vol = x.control.volume.unwrap_or(0.0);
            let muted = x.control.muted == Some(true);
            if dense {
                c.glyph(if muted { G_MUTE } else { G_VOL }, c.h / 2.0, c.h / 2.0, c.h * 0.36, c.text1());
                dense_face(c, &format!("{:.0}%", vol * 100.0), x.hub.output_name.as_deref().unwrap_or("Sound"), c.text1());
                out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Mute));
                return;
            }
            c.glyph(if muted { G_MUTE } else { G_VOL }, 26.0, 26.0, 16.0, c.text1());
            out.hits.push(((10.0, 10.0, 32.0, 32.0), WAct::Mute));
            c.tw(x.hub.output_name.as_deref().unwrap_or("Sound"), 46.0, 17.0, 12.5, 600, c.text1(), c.w - 110.0);
            c.tr(&format!("{:.0}%", vol * 100.0), c.w - 16.0, 17.0, 12.0, 500, c.text2());
            let (bx, by, bw, bh) = (16.0, 52.0, c.w - 32.0, 24.0);
            c.rr(bx, by, bw, bh, bh / 2.0, c.ink(0.12));
            c.rr(bx, by, (bw * vol).max(bh), bh, bh / 2.0, if muted { c.ink(0.3) } else { BLUE });
            out.hits.push(((bx, by - 4.0, bw, bh + 8.0), WAct::Volume));
            return;
        }
        MediaKind::Mic => {
            let muted = x.hub.mic_muted == Some(true);
            out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Mic));
            let s = c.h.min(c.w);
            let (cx, cy) = (c.w / 2.0, if dense { c.h / 2.0 } else { c.h / 2.0 - 8.0 });
            c.circle(cx, cy, s * 0.3, if muted { alpha(RED, 0.9) } else { c.ink(0.12) });
            c.glyph(G_MIC, cx, cy, s * 0.24, if muted { (1.0, 1.0, 1.0, 1.0) } else { c.text1() });
            if muted {
                c.line(cx - s * 0.14, cy - s * 0.14, cx + s * 0.14, cy + s * 0.14, 2.0, (1.0, 1.0, 1.0, 1.0));
            }
            if !dense {
                c.tc(if muted { "Microphone muted" } else { "Microphone on" }, cx, c.h - 24.0, 11.5, 500, c.text2(), c.w);
            }
            return;
        }
        _ => {}
    }
    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::MediaPlay));
    if m.is_none() {
        if dense && k == MediaKind::Disc {
            bg(c, true);
        }
        if dense {
            c.glyph(G_MUSIC, c.h / 2.0, c.h / 2.0, c.h * 0.36, c.text3());
            if k != MediaKind::Disc {
                dense_face(c, "Not playing", "Media", c.text2());
            }
        } else {
            if k == MediaKind::Disc {
                bg(c, false);
            }
            c.glyph(G_MUSIC, 40.0, c.h / 2.0, 22.0, c.text3());
            c.tw("Nothing playing", 70.0, c.h / 2.0 - 10.0, 13.0, 500, c.text2(), c.w - 90.0);
        }
        return;
    }
    match k {
        MediaKind::Disc => {
            if !dense {
                bg(c, false);
            }
            let s = c.h.min(c.w) - if dense { 6.0 } else { 16.0 };
            let (ax, ay) = ((c.w - s) / 2.0, (c.h - s) / 2.0);
            art(c, x, wic, ax, ay, s, s / 2.0);
            c.circle(c.w / 2.0, c.h / 2.0, s * 0.1, (0.1, 0.1, 0.12, 1.0));
            c.circle_stroke(c.w / 2.0, c.h / 2.0, s / 2.0 - 0.5, (1.0, 1.0, 1.0, 0.2), 1.0);
            if !playing {
                c.glyph(G_PLAY, c.w / 2.0, c.h / 2.0, s * 0.12, (1.0, 1.0, 1.0, 1.0));
            }
        }
        _ if dense => {
            let s = c.h - 10.0;
            art(c, x, wic, 5.0, 5.0, s, 8.0);
            let tx = s + 12.0;
            c.tw(&title, tx, c.h * 0.16, (c.h * 0.24).max(10.5), 600, c.text1(), c.w - tx - 6.0);
            c.tw(&artist, tx, c.h * 0.52, (c.h * 0.2).max(9.5), 400, c.text2(), c.w - tx - 6.0);
        }
        MediaKind::Full => {
            art(c, x, wic, 14.0, 14.0, 68.0, 12.0);
            c.tw(&title, 94.0, 16.0, 13.5, 600, c.text1(), c.w - 108.0);
            c.tw(&artist, 94.0, 36.0, 11.5, 400, c.text2(), c.w - 108.0);
            out.hits.clear();
            let by = 70.0;
            let xs = [104.0, 140.0, 176.0];
            for (i, (g, a)) in [(G_PREV, WAct::MediaPrev), (if playing { G_PAUSE } else { G_PLAY }, WAct::MediaPlay), (G_NEXT, WAct::MediaNext)].into_iter().enumerate() {
                c.glyph(g, xs[i], by, if i == 1 { 18.0 } else { 14.0 }, c.text1());
                out.hits.push(((xs[i] - 16.0, by - 16.0, 32.0, 32.0), a));
            }
        }
        MediaKind::Compact => {
            art(c, x, wic, 16.0, (c.h - 48.0) / 2.0, 48.0, 10.0);
            c.tw(&title, 76.0, c.h / 2.0 - 18.0, 13.0, 600, c.text1(), c.w - 140.0);
            c.tw(&artist, 76.0, c.h / 2.0 + 2.0, 11.5, 400, c.text2(), c.w - 140.0);
            c.circle(c.w - 36.0, c.h / 2.0, 18.0, c.ink(0.12));
            c.glyph(if playing { G_PAUSE } else { G_PLAY }, c.w - 36.0, c.h / 2.0, 14.0, c.text1());
        }
        MediaKind::Bar => {
            art(c, x, wic, 16.0, 14.0, 40.0, 8.0);
            c.tw(&title, 66.0, 15.0, 13.0, 600, c.text1(), c.w - 120.0);
            c.tw(&artist, 66.0, 34.0, 11.5, 400, c.text2(), c.w - 120.0);
            c.glyph(if playing { G_PAUSE } else { G_PLAY }, c.w - 30.0, 34.0, 16.0, c.text1());
            // Level meter that sways while playing.
            let n = 28;
            let bw = (c.w - 32.0) / n as f32;
            for i in 0..n {
                let ph = (x.now.second * 7 + i * 13) % 17;
                let hgt = if playing { 3.0 + ph as f32 * 0.9 } else { 3.0 };
                c.rr(16.0 + i as f32 * bw, c.h - 12.0 - hgt, bw * 0.6, hgt, 1.5, if playing { alpha(PURPLE, 0.85) } else { c.ink(0.2) });
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------ system

fn sys(c: &Canvas, out: &mut Painted, k: SysKind, x: &Ctx, dense: bool) {
    let st = x.stats;
    let cpu = st.map(|s| s.cpu_percent).unwrap_or(0.0);
    let (mu, mt) = st.map(|s| (s.mem_used_gb, s.mem_total_gb)).unwrap_or((0.0, 0.0));
    let memf = if mt > 0.0 { mu / mt } else { 0.0 };
    match k {
        SysKind::Resource | SysKind::Cpu | SysKind::Memory => {
            out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open("taskmgr".into())));
        }
        _ => {}
    }
    let ring_face = |c: &Canvas, frac: f32, col: Col, big: &str, label: &str, sub: &str, series: Option<Vec<f64>>| {
        if dense {
            c.ring(c.h / 2.0, c.h / 2.0, c.h * 0.3, 4.0, frac, col);
            dense_face(c, big, label, c.text1());
            return;
        }
        c.ring(48.0, c.h / 2.0, 30.0, 7.0, frac, col);
        c.tc(big, 48.0, c.h / 2.0 - 9.0, 13.5, 600, c.text1(), 60.0);
        c.tw(label, 96.0, 20.0, 13.0, 600, c.text1(), c.w - 110.0);
        c.tw(sub, 96.0, 40.0, 11.5, 400, c.text2(), c.w - 110.0);
        if let Some(sv) = series {
            c.area(96.0, 60.0, c.w - 112.0, 22.0, &sv, col, true);
        }
    };
    match k {
        SysKind::Resource => {
            if dense {
                c.ring(c.h / 2.0, c.h / 2.0, c.h * 0.3, 4.0, cpu / 100.0, BLUE);
                let tx = c.h;
                c.tw(&format!("CPU {cpu:.0}%"), tx, c.h * 0.18, (c.h * 0.22).max(10.0), 600, c.text1(), c.w - tx);
                c.tw(&format!("RAM {:.0}%", memf * 100.0), tx, c.h * 0.52, (c.h * 0.22).max(10.0), 600, c.text2(), c.w - tx);
                return;
            }
            c.ring(48.0, c.h / 2.0, 30.0, 7.0, cpu / 100.0, GREEN);
            c.tc(&format!("{cpu:.0}%"), 48.0, c.h / 2.0 - 14.0, 13.5, 600, c.text1(), 60.0);
            c.tc("CPU", 48.0, c.h / 2.0 + 3.0, 9.5, 600, c.text3(), 60.0);
            for (i, (l, f, v, col)) in [("CPU", cpu / 100.0, format!("{cpu:.0}%"), BLUE), ("MEM", memf, format!("{:.0}%", memf * 100.0), PURPLE)].iter().enumerate() {
                let y = 26.0 + i as f32 * 26.0;
                c.tw(l, 96.0, y, 11.0, 600, c.text2(), 40.0);
                c.bar(132.0, y + 5.0, c.w - 132.0 - 56.0, 5.0, *f, *col);
                c.tr(v, c.w - 16.0, y, 12.0, 600, c.text1());
            }
        }
        SysKind::Cpu => ring_face(c, cpu / 100.0, BLUE, &format!("{cpu:.0}%"), "CPU", "Processor load", Some(x.hub.cpu_hist.iter().map(|v| *v as f64).collect())),
        SysKind::Memory => ring_face(c, memf, PURPLE, &format!("{:.0}%", memf * 100.0), "Memory", &format!("{mu:.1} of {mt:.0} GB"), None),
        SysKind::Network => {
            let (d, u) = st.map(|s| (s.net_down_bps, s.net_up_bps)).unwrap_or((0.0, 0.0));
            if dense {
                let s = (c.h * 0.24).max(10.0);
                c.tw(&format!("↓ {}", rate(d)), 10.0, c.h * 0.16, s, 600, BLUE, c.w);
                c.tw(&format!("↑ {}", rate(u)), 10.0, c.h * 0.52, s, 600, GREEN, c.w);
                return;
            }
            c.tw("Network", 16.0, 12.0, 12.0, 600, c.text2(), 100.0);
            c.tw(&format!("↓ {}", rate(d)), 16.0, 32.0, 15.0, 600, BLUE, c.w / 2.0);
            c.tw(&format!("↑ {}", rate(u)), c.w / 2.0, 32.0, 15.0, 600, GREEN, c.w / 2.0 - 16.0);
            let down: Vec<f64> = x.hub.net_hist.iter().map(|v| v.0).collect();
            let up: Vec<f64> = x.hub.net_hist.iter().map(|v| v.1).collect();
            c.area(16.0, 58.0, c.w - 32.0, 26.0, &down, BLUE, true);
            c.area(16.0, 58.0, c.w - 32.0, 26.0, &up, GREEN, false);
        }
        SysKind::Battery | SysKind::Disk | SysKind::Storage | SysKind::Language | SysKind::Uptime => {
            let conn = match k {
                SysKind::Battery => "battery",
                SysKind::Language => "language",
                SysKind::Uptime => "uptime",
                _ => "storage",
            };
            let badge = super::canvas::gly(G_PLUG, 0x2C2C34, 0x30D158);
            let g = got(x, conn);
            if let Got::Feed(f) = g {
                match k {
                    SysKind::Battery => {
                        let charging = f.extra.get("charging").map(|v| v == "true").unwrap_or(false);
                        let frac = f.frac.unwrap_or(0.0);
                        let col = if frac < 0.2 && !charging { RED } else { GREEN };
                        let sub = if charging { "Charging".to_string() } else { f.extra.get("time_left").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "On battery".into()) };
                        ring_face(c, frac, col, &format!("{:.0}%", frac * 100.0), "Battery", &sub, None);
                    }
                    SysKind::Disk => {
                        let used = f.frac.unwrap_or(0.0);
                        ring_face(c, used, ORANGE, &format!("{:.0}%", used * 100.0), "Disk C:", &format!("{} GB free", compact(f.value.unwrap_or(0.0))), None);
                        out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open("ms-settings:storagesense".into())));
                    }
                    SysKind::Storage => {
                        out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open("ms-settings:storagesense".into())));
                        let used = f.frac.unwrap_or(0.0);
                        let total = f.rows.first().map(|r| r.value.clone()).unwrap_or_default();
                        if dense {
                            c.glyph("\u{EDA2}", c.h / 2.0, c.h / 2.0, c.h * 0.36, c.text2());
                            dense_face(c, &format!("{:.0} GB", f.value.unwrap_or(0.0)), "Free on C:", c.text1());
                            return;
                        }
                        c.num(&format!("{:.1} GB", f.value.unwrap_or(0.0)), 16.0, 14.0, 28.0, 600, c.text1());
                        c.tr("Free on C:", c.w - 16.0, 22.0, 12.0, 400, c.text2());
                        c.bar(16.0, 58.0, c.w - 32.0, 8.0, used, if used > 0.9 { RED } else { BLUE });
                        c.tw(&total, 16.0, 72.0, 11.0, 400, c.text3(), c.w - 32.0);
                    }
                    SysKind::Language => {
                        out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Language));
                        let code = f.text.clone().unwrap_or_else(|| "—".into());
                        if dense {
                            c.text(&code, c.w / 2.0, c.h / 2.0 - c.h * 0.2, c.h * 0.3, 700, c.text1(), Align::Center, c.w, Font::Display);
                            return;
                        }
                        c.glyph(G_KEYBOARD, 34.0, c.h / 2.0, 22.0, c.text2());
                        c.num(&code, 64.0, 18.0, 28.0, 700, c.text1());
                        c.tw(&f.caption, 64.0, 58.0, 12.0, 400, c.text2(), c.w - 80.0);
                    }
                    _ => {
                        let t = f.text.clone().unwrap_or_else(|| headline(&f));
                        if dense {
                            c.glyph(G_CLOCK, c.h / 2.0, c.h / 2.0, c.h * 0.36, c.text2());
                            dense_face(c, &t, "Uptime", c.text1());
                            return;
                        }
                        c.glyph(G_CLOCK, 34.0, c.h / 2.0, 22.0, c.text2());
                        c.num(&t, 64.0, 20.0, 26.0, 600, c.text1());
                        c.tw("Since Windows started", 64.0, 58.0, 12.0, 400, c.text2(), c.w - 80.0);
                    }
                }
            } else {
                placeholder(c, out, match k {
                    SysKind::Battery => "Battery",
                    SysKind::Language => "Keyboard Language",
                    SysKind::Uptime => "Uptime",
                    _ => "Storage",
                }, &badge, conn, &g, dense);
            }
        }
        SysKind::Brightness => {
            let b = x.hub.brightness;
            if dense {
                c.glyph(G_SUN, c.h / 2.0, c.h / 2.0, c.h * 0.36, c.text1());
                dense_face(c, &b.map(|v| format!("{:.0}%", v * 100.0)).unwrap_or_else(|| "—".into()), "Brightness", c.text1());
                out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open("ms-settings:display".into())));
                return;
            }
            c.glyph(G_SUN, 26.0, 26.0, 16.0, c.text1());
            c.tw("Brightness", 46.0, 17.0, 12.5, 600, c.text1(), c.w - 110.0);
            match b {
                Some(v) => {
                    c.tr(&format!("{:.0}%", v * 100.0), c.w - 16.0, 17.0, 12.0, 500, c.text2());
                    let (bx, by, bw, bh) = (16.0, 52.0, c.w - 32.0, 24.0);
                    c.rr(bx, by, bw, bh, bh / 2.0, c.ink(0.12));
                    c.rr(bx, by, (bw * v).max(bh), bh, bh / 2.0, (1.0, 1.0, 1.0, 0.95));
                    c.glyph(G_SUN, bx + 14.0, by + bh / 2.0, 11.0, (0.1, 0.1, 0.12, 0.85));
                    out.hits.push(((bx, by - 4.0, bw, bh + 8.0), WAct::Brightness));
                }
                None => {
                    c.tw("This monitor doesn't report its brightness (DDC/CI). Click for Display settings.", 16.0, 48.0, 11.0, 400, c.text3(), c.w - 32.0);
                    out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Open("ms-settings:display".into())));
                }
            }
        }
    }
}

fn rate(bps: f64) -> String {
    if bps >= 1_000_000.0 {
        format!("{:.1} MB/s", bps / 1_000_000.0)
    } else {
        format!("{:.0} KB/s", bps / 1000.0)
    }
}

// ------------------------------------------------------------------ tools

fn tile(c: &Canvas, out: &mut Painted, g: &str, label: &str, on: bool, col: Col, act: WAct, dense: bool) {
    out.hits.push(((0.0, 0.0, c.w, c.h), act));
    let s = c.h.min(c.w);
    let (cx, cy) = (c.w / 2.0, if dense { c.h / 2.0 } else { c.h / 2.0 - 9.0 });
    let r = if dense { s * 0.32 } else { 26.0 };
    c.circle(cx, cy, r, if on { col } else { c.ink(0.10) });
    if g.chars().next().map(|ch| ch as u32 >= 0xE000 && (ch as u32) < 0xF900).unwrap_or(false) {
        c.glyph(g, cx, cy, r * 0.8, if on { (1.0, 1.0, 1.0, 1.0) } else { c.text1() });
    } else {
        c.text(g, cx, cy - r * 0.62, r * 0.9, 400, c.text1(), Align::Center, r * 2.0, Font::Text);
    }
    if !dense {
        c.tc(label, cx, c.h - 24.0, 11.5, 500, c.text2(), c.w);
    }
}

fn tool(c: &Canvas, out: &mut Painted, k: ToolKind, x: &Ctx, dense: bool) {
    match k {
        ToolKind::Tray => tile(c, out, G_UP, "Background apps", false, BLUE, WAct::Launchpad, dense),
        ToolKind::DarkMode => tile(c, out, G_MOON, if x.control.dark { "Dark mode on" } else { "Dark mode off" }, x.control.dark, hex(0x5E5CE6), WAct::Dark, dense),
        ToolKind::Focus => tile(c, out, G_MOON, "Focus", false, hex(0x5E5CE6), WAct::Open("ms-settings:quiethours".into()), dense),
        ToolKind::Notifications => tile(c, out, G_BELL, "Notifications", false, BLUE, WAct::Open("ms-actioncenter:".into()), dense),
        ToolKind::ShowDesktop => tile(c, out, G_DESKTOP, "Show desktop", x.hub.desktop_shown, BLUE, WAct::ShowDesktop, dense),
        ToolKind::KeepAwake => tile(c, out, "☕", if x.hub.keep_awake { "Staying awake" } else { "Keep awake" }, x.hub.keep_awake, ORANGE, WAct::KeepAwake, dense),
        ToolKind::Search if dense => tile(c, out, G_SEARCH, "Search", false, BLUE, WAct::Search, dense),
        ToolKind::Search => {
            out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Search));
            c.rr(16.0, c.h / 2.0 - 20.0, c.w - 32.0, 40.0, 20.0, c.ink(0.10));
            c.glyph(G_SEARCH, 38.0, c.h / 2.0, 14.0, c.text2());
            c.tw("Search apps, files and settings", 58.0, c.h / 2.0 - 9.0, 13.0, 400, c.text3(), c.w - 80.0);
        }
        ToolKind::Run => tile(c, out, G_RUN, "Run", false, BLUE, WAct::Open("shell:::{2559a1f3-21d7-11d4-bdaf-00c04f60b9f0}".into()), dense),
        ToolKind::Trash => tile(c, out, G_TRASH, "Recycle Bin", false, BLUE, WAct::Open("shell:RecycleBinFolder".into()), dense),
        ToolKind::WhatsApp => {
            let att = x.whatsapp == Some(true);
            tile(c, out, G_CHAT, if att { "New messages" } else if x.whatsapp.is_some() { "WhatsApp" } else { "Open WhatsApp" }, true, hex(0x25D366), WAct::WhatsApp, dense);
            if att {
                let s = c.h.min(c.w);
                c.circle(c.w / 2.0 + s * 0.24, if dense { c.h * 0.22 } else { c.h / 2.0 - 30.0 }, if dense { s * 0.1 } else { 7.0 }, RED);
            }
        }
        ToolKind::Toggles => {
            let ctl = x.control;
            let items = [
                (G_WIFI, ctl.wifi == Some(true), WAct::Wifi),
                (G_BT, ctl.bluetooth == Some(true), WAct::Bluetooth),
                (G_MOON, ctl.dark, WAct::Dark),
                (if ctl.muted == Some(true) { G_MUTE } else { G_VOL }, ctl.muted == Some(true), WAct::Mute),
            ];
            let n = items.len() as f32;
            let slot = c.w / n;
            let r = if dense { c.h * 0.3 } else { 20.0 };
            for (i, (g, on, a)) in items.into_iter().enumerate() {
                let cx = slot * (i as f32 + 0.5);
                let cy = c.h / 2.0;
                c.circle(cx, cy, r, if on { BLUE } else { c.ink(0.10) });
                c.glyph(g, cx, cy, r * 0.75, if on { (1.0, 1.0, 1.0, 1.0) } else { c.text1() });
                out.hits.push(((cx - slot / 2.0, 0.0, slot, c.h), a));
            }
        }
        ToolKind::Screenshot => {
            let items = [("\u{F407}", "Region", "ms-screenclip:?clippingMode=Rectangle", BLUE), ("\u{E737}", "Window", "ms-screenclip:?clippingMode=Window", GREEN), ("\u{E740}", "Screen", "ms-screenclip:?clippingMode=Fullscreen", PURPLE)];
            let slot = c.w / 3.0;
            for (i, (g, l, uri, col)) in items.into_iter().enumerate() {
                let cx = slot * (i as f32 + 0.5);
                let s = if dense { c.h * 0.6 } else { 42.0 };
                let cy = if dense { c.h / 2.0 } else { c.h / 2.0 - 8.0 };
                c.rr(cx - s / 2.0, cy - s / 2.0, s, s, s * 0.28, alpha(col, 0.22));
                c.glyph(g, cx, cy, s * 0.42, col);
                if !dense {
                    c.tc(l, cx, c.h - 22.0, 11.0, 500, c.text2(), slot);
                }
                out.hits.push(((cx - slot / 2.0, 0.0, slot, c.h), WAct::Open(uri.into())));
            }
        }
        ToolKind::Apps => {
            let ids: Vec<&String> = x.pinned.iter().take(if dense { 4 } else { 7 }).collect();
            if ids.is_empty() {
                c.tw("Pin apps in Launchpad to see them here", 16.0, c.h / 2.0 - 8.0, 12.0, 400, c.text3(), c.w - 32.0);
                return;
            }
            let n = ids.len() as f32;
            let s = if dense { (c.h - 14.0).min((c.w - 8.0) / n - 6.0) } else { ((c.w - 24.0) / n - 8.0).min(40.0) };
            let slot = (c.w - 12.0) / n;
            for (i, id) in ids.iter().enumerate() {
                let cx = 6.0 + slot * (i as f32 + 0.5);
                let y = (c.h - s) / 2.0 - if dense { 0.0 } else { 6.0 };
                out.icons.push((cx - s / 2.0, y, s, (*id).clone()));
                out.hits.push(((cx - slot / 2.0, 0.0, slot, c.h), WAct::App((*id).clone())));
                if !dense {
                    let name = x.names.get(*id).cloned().unwrap_or_default();
                    c.tc(&name, cx, y + s + 4.0, 9.5, 400, c.text2(), slot - 2.0);
                }
            }
        }
        ToolKind::Notes => {
            out.hits.push(((0.0, 0.0, c.w, c.h), WAct::Notes));
            let lines: Vec<&str> = x.hub.notes.lines().filter(|l| !l.trim().is_empty()).take(4).collect();
            if dense {
                c.rr(c.h * 0.2, c.h * 0.2, c.h * 0.6, c.h * 0.6, 6.0, YELLOW);
                c.glyph(G_NOTE, c.h / 2.0, c.h / 2.0, c.h * 0.3, (0.2, 0.2, 0.2, 1.0));
                dense_face(c, lines.first().copied().unwrap_or("Notes"), &format!("{} lines", x.hub.notes.lines().count()), c.text1());
                return;
            }
            c.rr(14.0, 14.0, 30.0, 30.0, 8.0, YELLOW);
            c.glyph(G_NOTE, 29.0, 29.0, 14.0, (0.2, 0.2, 0.2, 1.0));
            if lines.is_empty() {
                c.tw("Notes", 54.0, 14.0, 13.0, 600, c.text1(), c.w);
                c.tw("Click to write a note.", 54.0, 34.0, 11.5, 400, c.text2(), c.w - 70.0);
            } else {
                c.tw(lines[0], 54.0, 14.0, 13.0, 600, c.text1(), c.w - 70.0);
                for (i, l) in lines.iter().skip(1).take(3).enumerate() {
                    c.tw(l, 54.0, 34.0 + i as f32 * 17.0, 11.5, 400, c.text2(), c.w - 70.0);
                }
            }
        }
        ToolKind::ScreenTime => {
            let total = x.hub.screen.total();
            let mut apps: Vec<(&String, &u64)> = x.hub.screen.apps.iter().collect();
            apps.sort_by(|a, b| b.1.cmp(a.1));
            if dense {
                c.glyph("\u{E7F4}", c.h / 2.0, c.h / 2.0, c.h * 0.34, BLUE);
                dense_face(c, &dur(total), apps.first().map(|a| a.0.as_str()).unwrap_or("Screen time"), c.text1());
                return;
            }
            c.tw("Screen Time", 16.0, 12.0, 11.5, 600, c.text2(), 120.0);
            c.num(&dur(total), 16.0, 28.0, 26.0, 600, c.text1());
            let cols = [BLUE, PURPLE, ORANGE];
            for (i, (name, secs)) in apps.iter().take(3).enumerate() {
                let y = 16.0 + i as f32 * 24.0;
                let x0 = c.w * 0.45;
                c.tw(name, x0, y, 11.0, 500, c.text1(), c.w - x0 - 60.0);
                c.tr(&dur(**secs), c.w - 16.0, y, 11.0, 400, c.text2());
                c.bar(x0, y + 16.0, c.w - x0 - 16.0, 3.0, if total > 0 { **secs as f32 / total as f32 } else { 0.0 }, cols[i]);
            }
            if apps.is_empty() {
                c.tw("Counting from now", 16.0, 66.0, 11.0, 400, c.text3(), c.w);
            }
        }
        ToolKind::Timeline => {
            let hours = &x.hub.screen.hours;
            let vals: Vec<f64> = (0..24).map(|h| *hours.get(h).unwrap_or(&0) as f64).collect();
            let (top, bot) = if dense { (6.0, 6.0) } else { (32.0, 22.0) };
            if !dense {
                c.tw("Day Timeline", 16.0, 10.0, 11.5, 600, c.text2(), 120.0);
                c.tr(&dur(x.hub.screen.total()), c.w - 16.0, 10.0, 11.5, 500, c.text1());
            }
            let w = c.w - 32.0;
            let slot = w / 24.0;
            let hi = vals.iter().cloned().fold(60.0, f64::max);
            for (h, v) in vals.iter().enumerate() {
                let ht = ((v / hi) as f32 * (c.h - top - bot)).max(2.0);
                let col = if h as u32 == x.now.hour { BLUE } else if *v > 0.0 { alpha(BLUE, 0.55) } else { c.ink(0.1) };
                c.rr(16.0 + slot * h as f32 + slot * 0.15, c.h - bot - ht, slot * 0.7, ht, 1.5, col);
            }
            if !dense {
                for (h, l) in [(0, "12a"), (6, "6a"), (12, "12p"), (18, "6p")] {
                    c.tw(l, 16.0 + slot * h as f32, c.h - 18.0, 9.5, 400, c.text3(), 40.0);
                }
            }
        }
    }
}

// ------------------------------------------------------------------ progress

fn progress(c: &Canvas, s: Span, x: &Ctx, dense: bool) {
    let n = x.now;
    let (total, done, label, frac) = match s {
        Span::Day => {
            let m = n.hour * 60 + n.minute;
            (24u32, n.hour, "Today", m as f32 / 1440.0)
        }
        Span::Month => (n.days_in_month(), n.day - 1, MONTHS[n.month as usize - 1], (n.day - 1) as f32 / n.days_in_month() as f32 + (n.hour as f32 / 24.0) / n.days_in_month() as f32),
        Span::Year => {
            let d = n.day_of_year();
            (52, (d - 1) / 7, "This year", (d - 1) as f32 / n.days_in_year() as f32)
        }
    };
    let pct = format!("{:.0}%", frac * 100.0);
    let col = match s {
        Span::Day => BLUE,
        Span::Month => GREEN,
        Span::Year => ORANGE,
    };
    let (gx, gy, gw, gh) = if dense { (8.0, 8.0, c.w * 0.6, c.h - 16.0) } else { (16.0, 14.0, c.w - 32.0, c.h - 44.0) };
    let cols = match s {
        Span::Day => 12,
        Span::Month => 11,
        Span::Year => 13,
    };
    let rows = total.div_ceil(cols);
    let cell = (gw / cols as f32).min(gh / rows as f32);
    let r = cell * 0.32;
    for i in 0..total {
        let (cx, cy) = (gx + cell * ((i % cols) as f32 + 0.5), gy + cell * ((i / cols) as f32 + 0.5));
        c.circle(cx, cy, r, if i < done { col } else if i == done { alpha(col, 0.5) } else { c.ink(0.14) });
    }
    if dense {
        c.text(&pct, c.w - 8.0, c.h / 2.0 - c.h * 0.2, c.h * 0.3, 600, c.text1(), Align::Right, c.w * 0.4, Font::Display);
        return;
    }
    c.tw(label, 16.0, c.h - 24.0, 11.5, 500, c.text2(), c.w * 0.6);
    c.tr(&pct, c.w - 16.0, c.h - 25.0, 12.5, 600, c.text1());
}
