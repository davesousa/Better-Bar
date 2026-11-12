//! Laminar's settings file: `%APPDATA%\Laminar\config.json`.
//!
//! This is the same idea as the prototype's settings object. Until the native
//! Settings window exists, edit the file (right-click the dock, choose
//! "Edit settings file…"); Laminar notices the save and applies it live.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Marks a separator in `pins`.
pub const SEPARATOR: &str = "|";

/// One entry in the dock: an app id, a separator ("|"), or a stack of apps.
/// In config.json: `"MSEdge"`, `"|"`, or `{ "stack": "Creative", "apps": ["...", "..."] }`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Pin {
    Id(String),
    Stack { stack: String, apps: Vec<String> },
}

impl Pin {
    pub fn sep() -> Self {
        Pin::Id(SEPARATOR.to_string())
    }
    pub fn is_sep(&self) -> bool {
        matches!(self, Pin::Id(s) if s == SEPARATOR)
    }
    /// The app id, for plain app pins.
    pub fn app(&self) -> Option<&str> {
        match self {
            Pin::Id(s) if s != SEPARATOR => Some(s),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    // Dock appearance (device-independent pixels)
    pub icon_size: f32,
    pub magnify: bool,
    pub magnified_size: f32,
    /// How many icons either side grow, roughly.
    pub magnify_range: f32,
    pub icon_spacing: f32,
    pub padding: f32,
    pub corner_radius: f32,
    /// 0–0.8 opacity of the dark tint over the blurred desktop.
    pub glass_tint: f32,
    /// 0–1 strength of the specular rim.
    pub edge_highlight: f32,
    /// Light glass when Windows' apps use light mode, dark glass when they use dark mode.
    /// Off: always dark.
    pub match_windows_theme: bool,
    /// Extra blur (pixels) on top of Windows' own background blur. 0: Windows' blur only.
    pub extra_blur: f32,

    // Behaviour
    /// Slide the dock off-screen; bring it back by pushing the pointer into the bottom edge.
    pub auto_hide: bool,
    /// Height in pixels of the bottom-edge band that reveals a hidden dock.
    pub reveal_zone: f32,
    pub reveal_delay_ms: u32,
    pub hide_delay_ms: u32,
    /// Hide the Windows taskbar while Laminar runs (a watchdog restores it on exit or crash).
    pub hide_windows_taskbar: bool,
    /// Clicking the app that is already in front minimizes it (Windows behaviour).
    pub click_to_minimize: bool,
    /// Pressing the Windows key on its own opens Launchpad instead of Start.
    pub win_key_launchpad: bool,
    /// Show a dock on every display (otherwise only the main display).
    pub show_on_all_displays: bool,
    /// Each display's dock lists only the apps with windows on that display.
    pub per_display_windows: bool,
    /// Apps shown under "Pinned" in Launchpad (empty: use the dock's pinned apps).
    pub launchpad_pins: Vec<String>,
    /// Weather location (default Toronto).
    pub weather_city: String,
    pub weather_lat: f32,
    pub weather_lon: f32,

    /// Pinned apps, left to right. Each entry is an AppUserModelID (as used by
    /// `shell:AppsFolder`) or a full .exe path; `"|"` is a separator; an object is a stack.
    pub pins: Vec<Pin>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            icon_size: 54.0,
            magnify: true,
            magnified_size: 88.0,
            magnify_range: 2.6,
            icon_spacing: 8.0,
            padding: 8.0,
            corner_radius: 24.0,
            glass_tint: 0.24,
            edge_highlight: 0.55,
            match_windows_theme: true,
            extra_blur: 0.0,
            auto_hide: false,
            reveal_zone: 4.0,
            reveal_delay_ms: 120,
            hide_delay_ms: 450,
            hide_windows_taskbar: true,
            click_to_minimize: true,
            win_key_launchpad: true,
            show_on_all_displays: true,
            per_display_windows: false,
            launchpad_pins: Vec::new(),
            weather_city: "Toronto".into(),
            weather_lat: 43.65,
            weather_lon: -79.38,
            pins: Vec::new(),
        }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Laminar")
}

pub fn path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    /// Load the settings file. Returns None if it doesn't exist or can't be parsed;
    /// a running dock then keeps its current settings until the file is fixed.
    /// (Never write anything here: the dock watches this folder, so a write would retrigger a reload.)
    pub fn load() -> Option<Config> {
        let text = std::fs::read_to_string(path()).ok()?;
        // Some editors (and PowerShell) save UTF-8 with a byte-order mark.
        let text = text.trim_start_matches('\u{feff}');
        serde_json::from_str::<Config>(text).ok().map(Config::sanitized)
    }

    /// True if a settings file exists (even if it currently fails to parse).
    pub fn exists() -> bool {
        path().exists()
    }

    /// Clamp values so a typo in the file can't break the dock.
    pub fn sanitized(mut self) -> Self {
        self.icon_size = self.icon_size.clamp(24.0, 128.0);
        self.magnified_size = self.magnified_size.clamp(self.icon_size, 192.0);
        self.magnify_range = self.magnify_range.clamp(1.0, 8.0);
        self.icon_spacing = self.icon_spacing.clamp(0.0, 32.0);
        self.padding = self.padding.clamp(2.0, 24.0);
        self.corner_radius = self.corner_radius.clamp(0.0, 64.0);
        self.glass_tint = self.glass_tint.clamp(0.0, 0.9);
        self.edge_highlight = self.edge_highlight.clamp(0.0, 1.0);
        self.reveal_zone = self.reveal_zone.clamp(1.0, 40.0);
        self.extra_blur = self.extra_blur.clamp(0.0, 40.0);
        // Pins saved before version-folder handling point into a folder an update deletes.
        // (In memory only: the next save writes the stable path.)
        let fix = |s: &mut String| {
            if crate::apps::is_path(s) {
                *s = crate::apps::stable_path(s).to_lowercase();
            }
        };
        for p in &mut self.pins {
            match p {
                Pin::Id(s) => fix(s),
                Pin::Stack { apps, .. } => apps.iter_mut().for_each(fix),
            }
        }
        self
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(dir());
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path(), text);
        }
    }

    /// True if anything that changes the dock's layout or look differs.
    pub fn visuals_differ(&self, o: &Config) -> bool {
        self.icon_size != o.icon_size
            || self.magnify != o.magnify
            || self.magnified_size != o.magnified_size
            || self.magnify_range != o.magnify_range
            || self.icon_spacing != o.icon_spacing
            || self.padding != o.padding
            || self.corner_radius != o.corner_radius
            || self.glass_tint != o.glass_tint
            || self.edge_highlight != o.edge_highlight
            || self.match_windows_theme != o.match_windows_theme
            || self.extra_blur != o.extra_blur
            || self.pins != o.pins
            || self.auto_hide != o.auto_hide
            || self.per_display_windows != o.per_display_windows
    }
}
