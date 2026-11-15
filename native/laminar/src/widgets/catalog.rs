//! Every widget Laminar offers: name, category, what it draws and where its data comes from.
//!
//! Widget ids are saved in config.json (Launchpad list and dock pins), so don't rename them.

use super::canvas::{gly, mono, Badge};
use serde::{Deserialize, Serialize};

/// A widget's size. In Launchpad: small = half a column wide, medium = a full column,
/// large = a full column and twice as tall. In the dock: one icon, the widget's own
/// width, or wider.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Medium, Size::Large];
    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "Small",
            Size::Medium => "Medium",
            Size::Large => "Large",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat {
    Highlights,
    Ai,
    Productivity,
    Finance,
    System,
    Media,
    Analytics,
    Developer,
    Online,
    Social,
    Apps,
    Weather,
    Stocks,
    Clocks,
    Control,
}

pub const CATS: &[(Cat, &str, &str)] = &[
    (Cat::Highlights, "Highlights", "Favourites to start with."),
    (Cat::Ai, "AI", "Your AI tools' usage and spend."),
    (Cat::Productivity, "Productivity", "Timers, focus, notes and your day."),
    (Cat::Finance, "Finance", "Revenue from your payment providers."),
    (Cat::System, "System", "What your PC is doing, and quick controls."),
    (Cat::Media, "Media & Audio", "What's playing, sound and microphone."),
    (Cat::Analytics, "Analytics", "Visitors and search traffic."),
    (Cat::Developer, "Developer", "Deploys, issues and services you run."),
    (Cat::Online, "Online & Mail", "Mail, GitHub, LeetCode and a daily wallpaper."),
    (Cat::Social, "Social", "Follower counts."),
    (Cat::Apps, "Apps & Files", "Launch apps, files and tools."),
    (Cat::Weather, "Weather", "Conditions and forecasts."),
    (Cat::Stocks, "Stocks", "Markets and crypto."),
    (Cat::Clocks, "Clocks", "Time, dates and calendars, every style."),
    (Cat::Control, "Control Center", "Wi-Fi, sound, background apps and Windows Update."),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockKind {
    Digital,
    Flip,
    World,
    WeekStrip,
    WorldTime,
    Today,
    Date,
    DayPage,
    Analog,
    /// Analog face in a rounded square.
    AnalogSquare,
    Segment,
    ClockWeather,
    Month,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeatherKind {
    Now,
    Detail,
    Hourly,
    Banner,
    Week,
    Sun,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TimerKind {
    Pomodoro,
    Countdown,
    Stopwatch,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MediaKind {
    Full,
    Compact,
    Disc,
    Bar,
    Output,
    Mic,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SysKind {
    Resource,
    Cpu,
    Memory,
    Network,
    Battery,
    Disk,
    Storage,
    Language,
    Uptime,
    Brightness,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolKind {
    Tray,
    Toggles,
    DarkMode,
    Focus,
    Notifications,
    ShowDesktop,
    KeepAwake,
    Apps,
    Search,
    Run,
    Screenshot,
    Trash,
    Notes,
    ScreenTime,
    Timeline,
    WhatsApp,
    /// Wi-Fi and Bluetooth.
    Connectivity,
    /// Dark mode, mute, snip and lock.
    ControlTiles,
    /// Windows Update status.
    WinUpdate,
    /// Opens Windows Settings.
    WinSettings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Span {
    Day,
    Month,
    Year,
}

/// How a widget draws.
#[derive(Clone, Copy, Debug)]
pub enum Face {
    /// Badge, big number, caption (revenue totals, followers, unread mail).
    Stat { conn: &'static str, badge: Badge, open: &'static str },
    /// Title, number, change and an area chart.
    Chart { conn: &'static str, badge: Badge, open: &'static str },
    /// Usage with a meter (AI tools, credits).
    Usage { conn: &'static str, badge: Badge, open: &'static str },
    /// Contribution-style heat map.
    Heat { conn: &'static str, badge: Badge, color: u32, open: &'static str },
    /// Rows with values (top processes, issues, agenda, recent files).
    List { conn: &'static str, badge: Badge, open: &'static str },
    /// Price rows with day change (watchlist).
    Watch { conn: &'static str },
    /// Next calendar event.
    Next,
    /// All AI tools in one card.
    AiAll,
    /// Revenue goal across connected providers.
    Goal,
    /// Bing image of the day.
    Wallpaper,
    Clock(ClockKind),
    Weather(WeatherKind),
    Timer(TimerKind),
    Media(MediaKind),
    Sys(SysKind),
    Tool(ToolKind),
    Progress(Span),
}

impl Face {
    /// The connector this face reads, if any.
    pub fn conn(&self) -> Option<&'static str> {
        match self {
            Face::Stat { conn, .. } | Face::Chart { conn, .. } | Face::Usage { conn, .. } | Face::Heat { conn, .. } | Face::List { conn, .. } | Face::Watch { conn } => Some(conn),
            Face::Next => Some("calendar"),
            Face::Wallpaper => Some("wallpaper"),
            Face::Sys(SysKind::Battery) => Some("battery"),
            Face::Sys(SysKind::Disk | SysKind::Storage) => Some("storage"),
            Face::Sys(SysKind::Language) => Some("language"),
            Face::Sys(SysKind::Uptime) => Some("uptime"),
            _ => None,
        }
    }
    /// Extra connectors read by combined widgets.
    pub fn extra_conns(&self) -> &'static [&'static str] {
        match self {
            Face::AiAll => &["claude_code", "codex", "anthropic", "openai", "openrouter", "cursor"],
            Face::Goal => &["stripe", "lemonsqueezy", "paddle", "shopify", "gumroad", "polar"],
            Face::Clock(ClockKind::Today) => &["calendar"],
            _ => &[],
        }
    }
    /// Seconds between redraws while visible (0: only when data changes).
    pub fn live_secs(&self) -> u32 {
        match self {
            Face::Clock(ClockKind::Digital | ClockKind::Analog | ClockKind::Segment | ClockKind::World | ClockKind::Flip) => 1,
            Face::Clock(_) => 30,
            Face::Timer(_) => 1,
            Face::Sys(SysKind::Resource | SysKind::Cpu | SysKind::Memory | SysKind::Network) => 2,
            Face::Progress(Span::Day) | Face::Tool(ToolKind::ScreenTime | ToolKind::Timeline) => 30,
            Face::Next => 30,
            _ => 0,
        }
    }
}

pub struct Def {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub cats: &'static [Cat],
    pub face: Face,
    /// Size when first added.
    pub size: Size,
    /// Width in the dock, in icon widths.
    pub dock_w: f32,
}

const fn w(id: &'static str, name: &'static str, about: &'static str, cats: &'static [Cat], face: Face) -> Def {
    Def { id, name, about, cats, face, size: Size::Medium, dock_w: 2.6 }
}
const fn tall(d: Def) -> Def {
    Def { size: Size::Large, ..d }
}
/// Width in the dock (icon widths) at medium size. Icon-sized widgets start small.
const fn dock(d: Def, dock_w: f32) -> Def {
    Def { dock_w, size: if dock_w <= 1.0 { Size::Small } else { d.size }, ..d }
}
const fn small(d: Def) -> Def {
    Def { size: Size::Small, ..d }
}

use Cat::*;

// Service marks: original letter/glyph tiles in each service's colours (not their logos).
const B_CLAUDE: Badge = mono("C", 0xD97757, 0xFFFFFF);
const B_OPENAI: Badge = mono("AI", 0x10A37F, 0xFFFFFF);
const B_CODEX: Badge = gly("\u{E943}", 0x1F1F24, 0xFFFFFF);
const B_OPENROUTER: Badge = mono("OR", 0x6467F2, 0xFFFFFF);
const B_CURSOR: Badge = gly("\u{E7C9}", 0x111114, 0xFFFFFF);
const B_STRIPE: Badge = mono("S", 0x635BFF, 0xFFFFFF);
const B_LEMON: Badge = mono("L", 0xFFC233, 0x1B1B1B);
const B_PADDLE: Badge = mono("P", 0xFDDD35, 0x1B1B1B);
const B_SHOPIFY: Badge = mono("S", 0x5E8E3E, 0xFFFFFF);
const B_GUMROAD: Badge = mono("G", 0xFF90E8, 0x1B1B1B);
const B_POLAR: Badge = mono("P", 0x0062FF, 0xFFFFFF);
const B_REVENUECAT: Badge = mono("RC", 0xF25A5A, 0xFFFFFF);
const B_SEARCH: Badge = gly("\u{E721}", 0x4285F4, 0xFFFFFF);
const B_PLAUSIBLE: Badge = mono("P", 0x5850EC, 0xFFFFFF);
const B_UMAMI: Badge = mono("U", 0x1F1F24, 0xFFFFFF);
const B_RESEND: Badge = mono("R", 0x0B0B0D, 0xFFFFFF);
const B_LINEAR: Badge = mono("L", 0x5E6AD2, 0xFFFFFF);
const B_SUPABASE: Badge = gly("\u{E945}", 0x1C1C1C, 0x3ECF8E);
const B_VERCEL: Badge = gly("\u{E70E}", 0x000000, 0xFFFFFF);
const B_GITHUB: Badge = gly("\u{E943}", 0x24292F, 0xFFFFFF);
const B_LEETCODE: Badge = mono("LC", 0xFFA116, 0x1B1B1B);
const B_GMAIL: Badge = gly("\u{E715}", 0xEA4335, 0xFFFFFF);
const B_X: Badge = mono("X", 0x000000, 0xFFFFFF);
const B_TIKTOK: Badge = gly("\u{E8D6}", 0x000000, 0xFFFFFF);
const B_YOUTUBE: Badge = gly("\u{E768}", 0xFF0033, 0xFFFFFF);
const B_INSTAGRAM: Badge = gly("\u{E722}", 0xD62976, 0xFFFFFF);
const B_STOCKS: Badge = gly("\u{E9D2}", 0x1F2A1F, 0x34C759);
const B_CRYPTO: Badge = mono("₿", 0xF7931A, 0xFFFFFF);
const B_CPU: Badge = gly("\u{E950}", 0x2C2C34, 0x0A84FF);
const B_MEM: Badge = gly("\u{E964}", 0x2C2C34, 0xBF5AF2);
const B_CAL: Badge = gly("\u{E787}", 0xFF3B30, 0xFFFFFF);
const B_FILES: Badge = gly("\u{E8B7}", 0xF2B84B, 0x1B1B1B);

pub static ALL: &[Def] = &[
    // ------------------------------------------------------------ AI
    w("ai-usage", "AI Usage", "Claude Code, Codex and your AI API spend together.", &[Highlights, Ai], Face::AiAll),
    w("claude-code", "Claude Code", "Tokens used today and in the current 5-hour window, from Claude Code's local logs.", &[Ai], Face::Usage { conn: "claude_code", badge: B_CLAUDE, open: "" }),
    w("claude-code-week", "Claude Code Week", "Daily tokens for the last two weeks.", &[Ai], Face::Chart { conn: "claude_code", badge: B_CLAUDE, open: "" }),
    w("anthropic-cost", "Claude API Spend", "This month's Anthropic API cost (admin key).", &[Ai], Face::Chart { conn: "anthropic", badge: B_CLAUDE, open: "https://console.anthropic.com/usage" }),
    w("openai-cost", "OpenAI API Spend", "This month's OpenAI API cost (admin key).", &[Highlights, Ai], Face::Chart { conn: "openai", badge: B_OPENAI, open: "https://platform.openai.com/usage" }),
    w("codex", "Codex Usage", "Tokens used today, from the Codex CLI's local logs.", &[Ai], Face::Usage { conn: "codex", badge: B_CODEX, open: "" }),
    w("openrouter", "OpenRouter Credits", "Credits left and how much you've used.", &[Ai], Face::Usage { conn: "openrouter", badge: B_OPENROUTER, open: "https://openrouter.ai/settings/credits" }),
    w("cursor", "Cursor Spend", "Your team's Cursor spend this billing cycle.", &[Ai], Face::Stat { conn: "cursor", badge: B_CURSOR, open: "https://cursor.com/dashboard" }),
    // ------------------------------------------------------------ Productivity
    dock(w("day-timeline", "Day Timeline", "When you were at your PC today, hour by hour.", &[Productivity], Face::Tool(ToolKind::Timeline)), 3.0),
    w("screen-time", "Screen Time", "Time at your PC today and the apps you used most.", &[Highlights, Productivity], Face::Tool(ToolKind::ScreenTime)),
    dock(w("pomodoro", "Pomodoro", "25-minute focus sessions with 5-minute breaks. Click to start or pause.", &[Productivity], Face::Timer(TimerKind::Pomodoro)), 2.2),
    dock(w("timer", "Timer", "A countdown. Click to start or pause; right-click to choose its length.", &[Productivity], Face::Timer(TimerKind::Countdown)), 2.2),
    dock(w("stopwatch", "Stopwatch", "Click to start or pause; right-click to reset.", &[Productivity], Face::Timer(TimerKind::Stopwatch)), 2.2),
    w("notes", "Notes", "Your notes file. Click to edit it.", &[Productivity], Face::Tool(ToolKind::Notes)),
    w("next-event", "Up Next", "Your next calendar event (any calendar with a private iCal link).", &[Productivity], Face::Next),
    tall(w("agenda", "Agenda", "Your upcoming events.", &[Productivity], Face::List { conn: "calendar", badge: B_CAL, open: "" })),
    dock(w("whatsapp", "WhatsApp", "Shows when WhatsApp has new messages. Click to open it.", &[Productivity], Face::Tool(ToolKind::WhatsApp)), 1.0),
    // ------------------------------------------------------------ Finance
    w("revenue-goal", "Revenue Goal", "Revenue across your connected providers against a monthly goal.", &[Finance], Face::Goal),
    w("stripe", "Stripe", "Net revenue in the last 30 days.", &[Finance], Face::Stat { conn: "stripe", badge: B_STRIPE, open: "https://dashboard.stripe.com" }),
    w("stripe-chart", "Stripe Chart", "Daily net revenue for 30 days.", &[Highlights, Finance], Face::Chart { conn: "stripe", badge: B_STRIPE, open: "https://dashboard.stripe.com" }),
    w("lemonsqueezy", "Lemon Squeezy", "Revenue in the last 30 days.", &[Finance], Face::Stat { conn: "lemonsqueezy", badge: B_LEMON, open: "https://app.lemonsqueezy.com" }),
    w("lemonsqueezy-chart", "Lemon Squeezy Chart", "Daily revenue for 30 days.", &[Finance], Face::Chart { conn: "lemonsqueezy", badge: B_LEMON, open: "https://app.lemonsqueezy.com" }),
    w("paddle", "Paddle", "Revenue in the last 30 days.", &[Finance], Face::Stat { conn: "paddle", badge: B_PADDLE, open: "https://vendors.paddle.com" }),
    w("paddle-chart", "Paddle Chart", "Daily revenue for 30 days.", &[Finance], Face::Chart { conn: "paddle", badge: B_PADDLE, open: "https://vendors.paddle.com" }),
    w("shopify", "Shopify", "Sales in the last 30 days.", &[Finance], Face::Stat { conn: "shopify", badge: B_SHOPIFY, open: "https://admin.shopify.com" }),
    w("shopify-chart", "Shopify Chart", "Daily sales for 30 days.", &[Finance], Face::Chart { conn: "shopify", badge: B_SHOPIFY, open: "https://admin.shopify.com" }),
    w("gumroad", "Gumroad", "Sales in the last 30 days.", &[Finance], Face::Stat { conn: "gumroad", badge: B_GUMROAD, open: "https://app.gumroad.com/dashboard" }),
    w("gumroad-chart", "Gumroad Chart", "Daily sales for 30 days.", &[Finance], Face::Chart { conn: "gumroad", badge: B_GUMROAD, open: "https://app.gumroad.com/dashboard" }),
    w("polar", "Polar", "Revenue in the last 30 days.", &[Finance], Face::Stat { conn: "polar", badge: B_POLAR, open: "https://polar.sh/dashboard" }),
    w("polar-chart", "Polar Chart", "Daily revenue for 30 days.", &[Finance], Face::Chart { conn: "polar", badge: B_POLAR, open: "https://polar.sh/dashboard" }),
    w("revenuecat", "RevenueCat", "Monthly recurring revenue and active subscriptions.", &[Finance], Face::Stat { conn: "revenuecat", badge: B_REVENUECAT, open: "https://app.revenuecat.com" }),
    // ------------------------------------------------------------ System
    tall(w("top-cpu", "Top CPU", "The apps using the most processor time.", &[System], Face::List { conn: "top_cpu", badge: B_CPU, open: "taskmgr" })),
    tall(w("top-memory", "Top Memory", "The apps using the most memory.", &[System], Face::List { conn: "top_memory", badge: B_MEM, open: "taskmgr" })),
    w("resource-monitor", "Resource Monitor", "CPU and memory at a glance.", &[Highlights, System], Face::Sys(SysKind::Resource)),
    dock(w("cpu", "CPU", "Processor load.", &[System], Face::Sys(SysKind::Cpu)), 2.0),
    dock(w("memory", "Memory", "Memory in use.", &[System], Face::Sys(SysKind::Memory)), 2.0),
    w("network", "Network", "Download and upload speed.", &[System], Face::Sys(SysKind::Network)),
    dock(w("battery", "Battery", "Charge and time remaining.", &[System], Face::Sys(SysKind::Battery)), 2.0),
    dock(w("disk", "Disk", "How full drive C: is.", &[System], Face::Sys(SysKind::Disk)), 2.0),
    dock(w("language", "Keyboard Language", "The input language. Click to switch (Win + Space).", &[System], Face::Sys(SysKind::Language)), 1.0),
    dock(w("uptime", "Uptime", "Time since Windows started.", &[System], Face::Sys(SysKind::Uptime)), 2.0),
    w("brightness", "Brightness", "Monitor brightness (monitors that support DDC/CI). Click the bar to set it.", &[System], Face::Sys(SysKind::Brightness)),
    w("quick-toggles", "Quick Toggles", "Wi-Fi, Bluetooth, dark mode and mute.", &[System], Face::Tool(ToolKind::Toggles)),
    dock(w("dark-mode", "Dark Mode", "Switch Windows between light and dark.", &[System], Face::Tool(ToolKind::DarkMode)), 1.0),
    dock(w("focus", "Focus", "Opens Windows' Do Not Disturb and Focus settings.", &[System], Face::Tool(ToolKind::Focus)), 1.0),
    dock(w("notifications", "Notifications", "Opens the notification centre.", &[System], Face::Tool(ToolKind::Notifications)), 1.0),
    dock(w("show-desktop", "Show Desktop", "Minimise everything; click again to bring it back.", &[System], Face::Tool(ToolKind::ShowDesktop)), 1.0),
    dock(w("keep-awake", "Keep Awake", "Stop the PC and screen from sleeping while it's on.", &[System], Face::Tool(ToolKind::KeepAwake)), 1.0),
    w("day-progress", "Day Progress", "How much of today has gone.", &[System], Face::Progress(Span::Day)),
    w("month-progress", "Month Progress", "How much of this month has gone.", &[System], Face::Progress(Span::Month)),
    w("year-progress", "Year Progress", "How much of this year has gone.", &[System], Face::Progress(Span::Year)),
    // ------------------------------------------------------------ Media & Audio
    w("now-playing", "Now Playing", "What's playing, with controls.", &[Highlights, Media], Face::Media(MediaKind::Full)),
    w("audio-output", "Audio Output", "Volume and output device. Click the bar to set the volume.", &[Media], Face::Media(MediaKind::Output)),
    dock(w("microphone", "Microphone", "Mute or unmute your microphone.", &[Media], Face::Media(MediaKind::Mic)), 1.0),
    w("now-playing-compact", "Now Playing Compact", "Track and a play button.", &[Media], Face::Media(MediaKind::Compact)),
    dock(w("now-playing-disc", "Now Playing Disc", "Album art spinning while it plays.", &[Media], Face::Media(MediaKind::Disc)), 1.0),
    w("now-playing-bar", "Now Playing Bar", "Track with a progress bar.", &[Media], Face::Media(MediaKind::Bar)),
    // ------------------------------------------------------------ Analytics
    w("search-console", "Search Console", "Google search clicks in the last 28 days.", &[Analytics], Face::Stat { conn: "search_console", badge: B_SEARCH, open: "https://search.google.com/search-console" }),
    w("plausible-chart", "Visitors Chart", "Daily visitors from Plausible.", &[Highlights, Analytics], Face::Chart { conn: "plausible", badge: B_PLAUSIBLE, open: "https://plausible.io/sites" }),
    w("plausible", "Plausible", "Visitors in the last 30 days.", &[Analytics], Face::Stat { conn: "plausible", badge: B_PLAUSIBLE, open: "https://plausible.io/sites" }),
    w("umami", "Umami", "Visitors in the last 30 days.", &[Analytics], Face::Chart { conn: "umami", badge: B_UMAMI, open: "https://cloud.umami.is" }),
    // ------------------------------------------------------------ Developer
    w("resend", "Resend", "Emails sent recently.", &[Developer], Face::Stat { conn: "resend", badge: B_RESEND, open: "https://resend.com/emails" }),
    w("linear", "Linear", "Open issues assigned to you.", &[Developer], Face::List { conn: "linear", badge: B_LINEAR, open: "https://linear.app" }),
    w("supabase", "Supabase", "API requests to your project.", &[Developer], Face::Chart { conn: "supabase", badge: B_SUPABASE, open: "https://supabase.com/dashboard" }),
    w("vercel", "Vercel", "Your latest deployments.", &[Developer], Face::List { conn: "vercel", badge: B_VERCEL, open: "https://vercel.com/dashboard" }),
    // ------------------------------------------------------------ Online & Mail
    w("wallpaper", "Wallpaper Gallery", "Bing's image of the day. Click to make it your wallpaper.", &[Online], Face::Wallpaper),
    w("gmail", "Mail", "Unread mail in your Gmail inbox.", &[Online], Face::Stat { conn: "gmail", badge: B_GMAIL, open: "https://mail.google.com" }),
    w("github-activity", "GitHub Activity", "Your contribution graph.", &[Highlights, Online], Face::Heat { conn: "github", badge: B_GITHUB, color: 0x39D353, open: "https://github.com" }),
    w("github-streak", "GitHub Streak", "Days in a row with contributions.", &[Online], Face::Stat { conn: "github", badge: B_GITHUB, open: "https://github.com" }),
    w("github-overview", "GitHub Overview", "Repositories, stars and followers.", &[Online], Face::List { conn: "github", badge: B_GITHUB, open: "https://github.com" }),
    w("leetcode", "LeetCode", "Problems solved and your submission graph.", &[Online], Face::Heat { conn: "leetcode", badge: B_LEETCODE, color: 0xFFA116, open: "https://leetcode.com" }),
    // ------------------------------------------------------------ Social
    w("x-followers", "X Followers", "Your follower count.", &[Highlights, Social], Face::Stat { conn: "x", badge: B_X, open: "https://x.com" }),
    w("tiktok", "TikTok", "Your follower count.", &[Social], Face::Stat { conn: "tiktok", badge: B_TIKTOK, open: "https://www.tiktok.com" }),
    w("youtube", "YouTube", "Your subscriber count.", &[Social], Face::Stat { conn: "youtube", badge: B_YOUTUBE, open: "https://studio.youtube.com" }),
    w("instagram", "Instagram", "Your follower count.", &[Social], Face::Stat { conn: "instagram", badge: B_INSTAGRAM, open: "https://www.instagram.com" }),
    // ------------------------------------------------------------ Apps & Files
    dock(w("apps", "Apps", "Your Launchpad pinned apps in a row.", &[Apps], Face::Tool(ToolKind::Apps)), 3.4),
    tall(w("recent-files", "Recent Files", "Files you opened recently.", &[Apps], Face::List { conn: "recent_files", badge: B_FILES, open: "" })),
    dock(w("search", "Search", "Search apps, files and settings.", &[Apps], Face::Tool(ToolKind::Search)), 1.0),
    dock(w("run", "Run", "Windows' Run box.", &[Apps], Face::Tool(ToolKind::Run)), 1.0),
    w("storage", "Storage", "Free space on your drives.", &[Apps], Face::Sys(SysKind::Storage)),
    w("screenshot", "Screenshot Tools", "Snip a region, a window or the full screen.", &[Apps], Face::Tool(ToolKind::Screenshot)),
    dock(w("recycle-bin", "Recycle Bin", "Open the Recycle Bin.", &[Apps], Face::Tool(ToolKind::Trash)), 1.0),
    // ------------------------------------------------------------ Weather
    w("weather", "Weather", "Current conditions.", &[Weather], Face::Weather(WeatherKind::Now)),
    w("weather-detail", "Weather Details", "Feels like, humidity, wind and UV.", &[Weather], Face::Weather(WeatherKind::Detail)),
    w("weather-hourly", "Today's Forecast", "The next hours.", &[Weather], Face::Weather(WeatherKind::Hourly)),
    w("weather-banner", "Weather Banner", "Conditions on a sky-coloured card.", &[Weather], Face::Weather(WeatherKind::Banner)),
    w("weather-week", "Weekly Forecast", "The next seven days.", &[Weather], Face::Weather(WeatherKind::Week)),
    w("sunrise-sunset", "Sunrise & Sunset", "Where the sun is in today's daylight.", &[Weather], Face::Weather(WeatherKind::Sun)),
    // ------------------------------------------------------------ Stocks
    w("stock-chart", "Stock Chart", "Today's price chart for your first symbol.", &[Stocks], Face::Chart { conn: "stocks", badge: B_STOCKS, open: "https://finance.yahoo.com" }),
    w("watchlist", "Watchlist", "Your symbols and today's change.", &[Stocks], Face::Watch { conn: "stocks" }),
    w("crypto", "Crypto", "Today's price chart for your first coin.", &[Stocks], Face::Chart { conn: "crypto", badge: B_CRYPTO, open: "https://www.coingecko.com" }),
    // ------------------------------------------------------------ Clocks
    w("clock", "Clock", "Large time with seconds and the date.", &[Clocks], Face::Clock(ClockKind::Digital)),
    w("flip-clock", "Flip Clock", "Split-flap style digits.", &[Clocks], Face::Clock(ClockKind::Flip)),
    w("world-clocks", "World Clocks", "Three cities on analog faces.", &[Clocks], Face::Clock(ClockKind::World)),
    w("week", "7 Days", "This week at a glance.", &[Clocks], Face::Clock(ClockKind::WeekStrip)),
    w("world-time", "World Time", "Another city's time next to yours.", &[Clocks], Face::Clock(ClockKind::WorldTime)),
    w("today", "Today", "The date with your next event.", &[Clocks], Face::Clock(ClockKind::Today)),
    w("date", "Date", "Today's date, large.", &[Clocks], Face::Clock(ClockKind::Date)),
    dock(w("day-page", "Day Page", "A calendar page for today.", &[Clocks], Face::Clock(ClockKind::DayPage)), 1.0),
    dock(w("analog", "Analog Clock", "A clock face with a sweeping second hand.", &[Clocks], Face::Clock(ClockKind::Analog)), 1.0),
    dock(w("analog-square", "Analog Square", "The analog clock in a rounded square.", &[Clocks], Face::Clock(ClockKind::AnalogSquare)), 1.0),
    w("segment-clock", "Segment Clock", "Seven-segment digits.", &[Clocks], Face::Clock(ClockKind::Segment)),
    w("clock-weather", "Clock & Weather", "Time and temperature together.", &[Clocks], Face::Clock(ClockKind::ClockWeather)),
    tall(w("month", "Calendar", "This month, with today marked.", &[Clocks], Face::Clock(ClockKind::Month))),
];

/// Control Center widgets (the right column). Background Apps, Windows Update and
/// Windows Settings are always shown unless turned off in Settings → Widgets.
pub static CONTROL: &[Def] = &[
    small(w("connectivity", "Wi-Fi & Bluetooth", "Turn Wi-Fi and Bluetooth on or off; click a name for networks or devices.", &[Control], Face::Tool(ToolKind::Connectivity))),
    w("control-tiles", "Quick Controls", "Dark mode, mute, snip and lock.", &[Control], Face::Tool(ToolKind::ControlTiles)),
    tall(w("background-apps", "Background Apps", "The icons apps put beside the clock. Click to open, right-click for the app's menu.", &[Control], Face::Tool(ToolKind::Tray))),
    w("windows-update", "Windows Update", "Updates waiting or a restart needed. Click for Windows Update.", &[Control], Face::Tool(ToolKind::WinUpdate)),
    small(w("windows-settings", "Windows Settings", "Opens Windows Settings.", &[Control], Face::Tool(ToolKind::WinSettings))),
];

/// Widgets that stay in Launchpad unless turned off in Settings → Widgets (config key, id).
pub const PINNED: &[(&str, &str)] = &[("show_background_apps", "background-apps"), ("show_windows_update", "windows-update"), ("show_windows_settings", "windows-settings")];

pub fn all() -> impl Iterator<Item = &'static Def> {
    ALL.iter().chain(CONTROL.iter())
}

pub fn get(id: &str) -> Option<&'static Def> {
    if let Some(d) = CONTROL.iter().find(|d| d.id == id) {
        return Some(d);
    }
    ALL.iter().find(|d| d.id == id)
}

pub fn in_cat(c: Cat) -> impl Iterator<Item = &'static Def> {
    all().filter(move |d| d.cats.contains(&c))
}

/// Launchpad's widgets before the user changes anything (the four it always had).
pub fn default_launchpad() -> Vec<crate::config::WRef> {
    ["clock", "weather-hourly", "resource-monitor", "month"].iter().map(|s| crate::config::WRef::Id(s.to_string())).collect()
}

/// The right column before the user changes anything: what Control Center always had.
pub fn default_right() -> Vec<crate::config::WRef> {
    ["connectivity", "now-playing", "control-tiles", "audio-output", "background-apps", "windows-update", "windows-settings"]
        .iter()
        .map(|s| crate::config::WRef::Id(s.to_string()))
        .collect()
}
