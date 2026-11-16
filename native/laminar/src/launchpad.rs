//! Launchpad: a full-screen glass overlay with three panels.
//!   Left:   widgets (clock, weather, system, calendar)
//!   Centre: search (apps, files, settings, web), pinned apps, All Apps — or Settings
//!   Right:  control center (Wi-Fi, Bluetooth, dark mode, mute, media, volume, tray icons)
//!
//! Each panel renders its visuals independently. Clicks are returned to the dock as
//! `Act` values, because most actions touch the dock or the system.

use std::collections::HashMap;

use laminar_sys::apps::AppEntry;
use laminar_sys::files::FileHit;
use laminar_sys::stats::Stats;
use laminar_sys::weather::Weather;
use windows::core::{Interface, Result};
use windows::UI::Composition::*;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use crate::config::Config;
use crate::gfx::{Gfx, TextStyle};
use crate::services::ControlState;
use crate::ui::{self, rgba, theme, v2, v3, Panel, Popup};

// Segoe Fluent Icons code points.
const G_SEARCH: &str = "\u{E721}";
const G_SETTINGS: &str = "\u{E713}";
const G_POWER: &str = "\u{E7E8}";
const G_LOCK: &str = "\u{E72E}";
const G_CHEV_R: &str = "\u{E76C}";
const G_CHEV_D: &str = "\u{E70D}";
const G_FOLDER: &str = "\u{E8B7}";
const G_DOC: &str = "\u{E8A5}";
const G_GLOBE: &str = "\u{E774}";
const G_CHEV_L: &str = "\u{E76B}";
const G_CHECK: &str = "\u{E73E}";
/// Wi-Fi signal: 1, 2, 3 bars, full.
const G_WIFI_BARS: [&str; 4] = ["\u{E872}", "\u{E873}", "\u{E874}", "\u{E701}"];

/// Something the user activated.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Close,
    App(String),
    File(String),
    Uri(String),
    Web(String),
    ToggleAll,
    OpenSettings,
    CloseSettings,
    Section(usize),
    Power,
    Wifi,
    /// Show the Wi-Fi network list in the right panel.
    WifiMenu,
    WifiBack,
    WifiJoin(String),
    Bluetooth,
    Dark,
    Mute,
    MediaPrev,
    MediaPlay,
    MediaNext,
    Toggle(&'static str),
    Slider(&'static str),
    OpenConfig,
    /// Check GitHub for a newer Laminar now.
    CheckUpdates,
    Snip,
    Lock,
    Search,
    /// Something inside a widget card (column, index), with the click's 0..1 position across the hit.
    Widget(WPos, crate::widgets::WAct, f32),
    /// A widget card itself (press and hold or drag to move it; right-click for its menu).
    Card(WPos),
    /// A widget preview in the gallery (drag it into a column or the dock).
    GalleryItem(&'static str),
    /// Open the widget gallery.
    AddWidgets,
    GalleryCat(usize),
    CloseGallery,
    /// Add a widget to Launchpad (false) or the dock (true).
    AddWidget(&'static str, bool),
    RemoveWidget(&'static str),
    /// Show the connect form for a connector.
    Connect(&'static str),
    FormFocus(usize),
    FormSave,
    FormCancel,
    FormDisconnect,
}

#[derive(Clone)]
struct Hit {
    /// Window-pixel rectangle (for scrolled content: content coordinates; see `scroll`).
    r: (f32, f32, f32, f32),
    act: Act,
    radius: f32,
    highlight: bool,
    scroll: bool,
    /// In a widget column's scrolling list (0 left, 1 right).
    col: Option<usize>,
}

/// A search result row.
#[derive(Clone)]
enum ResultItem {
    App(String, String),
    File(FileHit),
    Setting(&'static str, &'static str),
    Laminar(&'static str, usize),
    Web(String),
}

enum Ctl {
    Toggle,
    Slider { min: f32, max: f32, step: f32, unit: &'static str },
}

struct Row {
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    ctl: Ctl,
}

const SECTION_NAMES: [&str; 5] = ["Dock", "Glass", "Behavior", "About", "Widgets"];
/// The order the tabs are shown in (section numbers stay as above; About goes last).
const TAB_ORDER: [usize; 5] = [0, 1, 2, 4, 3];

fn section_rows(i: usize) -> Vec<Row> {
    use Ctl::*;
    let r = |key, label, hint, ctl| Row { key, label, hint, ctl };
    match i {
        0 => vec![
            r("icon_size", "Icon size", "", Slider { min: 32.0, max: 96.0, step: 1.0, unit: "px" }),
            r("magnify", "Magnification", "Icons grow as the pointer moves across the dock.", Toggle),
            r("magnified_size", "Magnified size", "", Slider { min: 40.0, max: 160.0, step: 1.0, unit: "px" }),
            r("magnify_range", "Magnification spread", "", Slider { min: 1.0, max: 6.0, step: 0.1, unit: "icons" }),
            r("icon_spacing", "Icon spacing", "", Slider { min: 0.0, max: 24.0, step: 1.0, unit: "px" }),
            r("padding", "Shelf padding", "", Slider { min: 4.0, max: 20.0, step: 1.0, unit: "px" }),
            r("corner_radius", "Corner radius", "", Slider { min: 6.0, max: 40.0, step: 1.0, unit: "px" }),
        ],
        1 => vec![
            r("glass_tint", "Tint", "Lower is more transparent.", Slider { min: 0.0, max: 0.8, step: 0.01, unit: "%" }),
            r("edge_highlight", "Edge highlight", "The bright rim around the glass.", Slider { min: 0.0, max: 1.0, step: 0.01, unit: "%" }),
            r("match_windows_theme", "Match Windows light/dark mode", "Light glass in light mode. Off: always dark.", Toggle),
            r("extra_blur", "Extra blur", "Added to Windows' own blur (it can't go lower than that).", Slider { min: 0.0, max: 40.0, step: 1.0, unit: "px" }),
            r("panel_shadow", "Panel shadow", "Depth under Launchpad's three panels.", Slider { min: 0.0, max: 1.0, step: 0.01, unit: "%" }),
            r("card_shadow", "Widget shadow", "Depth under widgets and tiles.", Slider { min: 0.0, max: 1.0, step: 0.01, unit: "%" }),
        ],
        2 => vec![
            r("auto_hide", "Automatically hide the dock", "Push the pointer into the bottom edge to bring it back.", Toggle),
            r("reveal_zone", "Reveal zone height", "", Slider { min: 1.0, max: 40.0, step: 1.0, unit: "px" }),
            r("reveal_delay_ms", "Reveal delay", "", Slider { min: 0.0, max: 1000.0, step: 10.0, unit: "ms" }),
            r("hide_delay_ms", "Hide delay", "", Slider { min: 0.0, max: 2000.0, step: 10.0, unit: "ms" }),
            r("click_to_minimize", "Click the focused app to minimize it", "Windows taskbar behaviour.", Toggle),
            r("win_key_launchpad", "Windows key opens Launchpad", "Win + other keys still work as usual.", Toggle),
            r("hide_windows_taskbar", "Hide the Windows taskbar", "A watchdog brings it back if Laminar exits.", Toggle),
            r("show_on_all_displays", "Show the dock on all displays", "Otherwise only on the main display.", Toggle),
            r("per_display_windows", "Each display shows only its own windows", "Pinned apps always show.", Toggle),
        ],
        4 => vec![
            r("show_background_apps", "Background Apps widget", "Always in Launchpad unless turned off here.", Toggle),
            r("show_windows_update", "Windows Update widget", "Always in Launchpad unless turned off here.", Toggle),
            r("show_windows_settings", "Windows Settings widget", "Always in Launchpad unless turned off here.", Toggle),
        ],
        3 => vec![
            r("start_at_login", "Start Laminar when you sign in", "Installed copies only.", Toggle),
            r("check_updates", "Check for updates automatically", "New versions download quietly; an icon in the dock offers to install them.", Toggle),
        ],
        _ => vec![],
    }
}

pub fn cfg_get(c: &Config, key: &str) -> f32 {
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    match key {
        "icon_size" => c.icon_size,
        "magnify" => b(c.magnify),
        "magnified_size" => c.magnified_size,
        "magnify_range" => c.magnify_range,
        "icon_spacing" => c.icon_spacing,
        "padding" => c.padding,
        "corner_radius" => c.corner_radius,
        "glass_tint" => c.glass_tint,
        "edge_highlight" => c.edge_highlight,
        "match_windows_theme" => b(c.match_windows_theme),
        "extra_blur" => c.extra_blur,
        "auto_hide" => b(c.auto_hide),
        "reveal_zone" => c.reveal_zone,
        "reveal_delay_ms" => c.reveal_delay_ms as f32,
        "hide_delay_ms" => c.hide_delay_ms as f32,
        "click_to_minimize" => b(c.click_to_minimize),
        "win_key_launchpad" => b(c.win_key_launchpad),
        "hide_windows_taskbar" => b(c.hide_windows_taskbar),
        "show_on_all_displays" => b(c.show_on_all_displays),
        "per_display_windows" => b(c.per_display_windows),
        "start_at_login" => b(c.start_at_login),
        "check_updates" => b(c.check_updates),
        "panel_shadow" => c.panel_shadow,
        "card_shadow" => c.card_shadow,
        "show_background_apps" => b(c.show_background_apps),
        "show_windows_update" => b(c.show_windows_update),
        "show_windows_settings" => b(c.show_windows_settings),
        _ => 0.0,
    }
}

pub fn cfg_set(c: &mut Config, key: &str, v: f32) {
    match key {
        "icon_size" => c.icon_size = v,
        "magnify" => c.magnify = v > 0.5,
        "magnified_size" => c.magnified_size = v,
        "magnify_range" => c.magnify_range = v,
        "icon_spacing" => c.icon_spacing = v,
        "padding" => c.padding = v,
        "corner_radius" => c.corner_radius = v,
        "glass_tint" => c.glass_tint = v,
        "edge_highlight" => c.edge_highlight = v,
        "match_windows_theme" => c.match_windows_theme = v > 0.5,
        "extra_blur" => c.extra_blur = v,
        "auto_hide" => c.auto_hide = v > 0.5,
        "reveal_zone" => c.reveal_zone = v,
        "reveal_delay_ms" => c.reveal_delay_ms = v as u32,
        "hide_delay_ms" => c.hide_delay_ms = v as u32,
        "click_to_minimize" => c.click_to_minimize = v > 0.5,
        "start_at_login" => c.start_at_login = v > 0.5,
        "check_updates" => c.check_updates = v > 0.5,
        "win_key_launchpad" => c.win_key_launchpad = v > 0.5,
        "hide_windows_taskbar" => c.hide_windows_taskbar = v > 0.5,
        "show_on_all_displays" => c.show_on_all_displays = v > 0.5,
        "per_display_windows" => c.per_display_windows = v > 0.5,
        "panel_shadow" => c.panel_shadow = v,
        "card_shadow" => c.card_shadow = v,
        "show_background_apps" => c.show_background_apps = v > 0.5,
        "show_windows_update" => c.show_windows_update = v > 0.5,
        "show_windows_settings" => c.show_windows_settings = v > 0.5,
        _ => {}
    }
}

/// Windows Settings pages offered in search.
const WIN_SETTINGS: &[(&str, &str)] = &[
    ("Bluetooth & devices", "ms-settings:bluetooth"),
    ("Display", "ms-settings:display"),
    ("Sound", "ms-settings:sound"),
    ("Wi-Fi", "ms-settings:network-wifi"),
    ("Network & internet", "ms-settings:network"),
    ("Windows Update", "ms-settings:windowsupdate"),
    ("Installed apps", "ms-settings:appsfeatures"),
    ("Default apps", "ms-settings:defaultapps"),
    ("Power & sleep", "ms-settings:powersleep"),
    ("Personalization", "ms-settings:personalization"),
    ("Background (wallpaper)", "ms-settings:personalization-background"),
    ("Colors & dark mode", "ms-settings:colors"),
    ("Notifications", "ms-settings:notifications"),
    ("Accounts", "ms-settings:yourinfo"),
    ("Date & time", "ms-settings:dateandtime"),
    ("Mouse", "ms-settings:mousetouchpad"),
    ("Printers & scanners", "ms-settings:printers"),
    ("Storage", "ms-settings:storagesense"),
    ("About this PC", "ms-settings:about"),
];

const LAMINAR_SETTINGS: &[(&str, usize)] = &[
    ("Laminar: Dock size & magnification", 0),
    ("Laminar: Glass & transparency", 1),
    ("Laminar: Auto-hide & behavior", 2),
    ("Laminar: Windows key & taskbar", 2),
    ("Laminar: Widgets & connections", 4),
];

/// Everything Launchpad shows, borrowed from the dock for one render.
pub struct Data<'a> {
    pub apps: &'a [AppEntry],
    pub brushes: &'a HashMap<String, CompositionSurfaceBrush>,
    pub pinned: &'a [String],
    pub names: &'a HashMap<String, String>,
    pub control: &'a ControlState,
    pub stats: Option<&'a Stats>,
    pub weather: Option<&'a Weather>,
    pub files: &'a [FileHit],
    pub cfg: &'a Config,
    pub user: &'a str,
    /// Visible Wi-Fi networks (None until the first answer), or the Windows error code.
    pub wifi: Option<&'a std::result::Result<Vec<laminar_sys::radios::WifiNetwork>, u32>>,
    /// Tray icons ("background apps"), in the order apps added them.
    pub tray: &'a [TrayView],
    /// The same icons as (key, name), for widgets.
    pub tray_list: &'a [(u64, String)],
    /// Widget state (feeds, timers) and whether WhatsApp wants attention.
    pub hub: &'a crate::widgets::Hub,
    pub whatsapp: Option<bool>,
    /// Windows Update status (None until the first check finishes).
    pub update: Option<&'a laminar_sys::update::UpdateStatus>,
    /// Laminar's version and its update status, for Settings → About.
    pub app_version: &'a str,
    pub app_update: &'a str,
}

/// A tray icon as Launchpad shows it.
pub struct TrayView {
    pub key: u64,
    pub brush: Option<CompositionSurfaceBrush>,
    pub tip: String,
}

#[derive(Clone, Copy, PartialEq)]
enum PanelId {
    Left,
    Center,
    Right,
}

pub struct Launchpad {
    pub popup: Popup,
    pub open: bool,
    scale: f32,
    size: (f32, f32),
    backdrop: Option<SpriteVisual>,
    panels: Vec<(PanelId, Panel, (f32, f32))>, // panel, its top-left in the window
    scroll_host: Option<ContainerVisual>,
    scroll_view: (f32, f32, f32, f32),
    scroll_y: f32,
    content_h: f32,
    hits: Vec<(PanelId, Hit)>,
    hover: Option<usize>,
    hover_shape: Option<(ShapeVisual, CompositionRoundedRectangleGeometry)>,
    pub query: String,
    pub results_len: usize,
    results: Vec<ResultItem>,
    pub sel: usize,
    pub all_open: bool,
    /// Some(section) while the centre panel shows Settings.
    pub settings: Option<usize>,
    pub dragging: Option<&'static str>,
    caret: Option<ShapeVisual>,
    /// The right panel shows the Wi-Fi network list.
    pub wifi_open: bool,

    /// The centre panel shows the widget gallery (category index).
    pub gallery: Option<usize>,
    /// The centre panel shows a connector's form.
    pub form: Option<Form>,
    /// The two widget columns (left, right).
    cols: [Column; 2],
    /// A widget being dragged (from a column or the gallery).
    wdrag: Option<WDrag>,
    /// The card under the pointer (its grip shows) and a pressed card.
    hover_card: Option<WPos>,
    pressed_card: Option<WPos>,
    /// The tray icon under the pointer (Background Apps names it).
    pub hover_tray: Option<u64>,
    /// Shadow under widget cards: (strength, scale, brush).
    shadow: Option<(f32, f32, CompositionNineGridBrush)>,
}

fn inside(r: (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

impl Launchpad {
    pub fn new(c: &Compositor) -> Result<Self> {
        Ok(Self {
            popup: Popup::new(c, true)?,
            open: false,
            scale: 1.0,
            size: (0.0, 0.0),
            backdrop: None,
            panels: Vec::new(),
            scroll_host: None,
            scroll_view: (0.0, 0.0, 0.0, 0.0),
            scroll_y: 0.0,
            content_h: 0.0,
            hits: Vec::new(),
            hover: None,
            hover_shape: None,
            query: String::new(),
            results_len: 0,
            results: Vec::new(),
            sel: 0,
            all_open: false,
            settings: None,
            dragging: None,
            caret: None,
            wifi_open: false,
            gallery: None,
            form: None,
            cols: [Column::default(), Column::default()],
            wdrag: None,
            hover_card: None,
            pressed_card: None,
            hover_tray: None,
            shadow: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.popup.hwnd
    }

    // ------------------------------------------------------------ open / close / layout

    /// Build the frame (backdrop + three glass panels) and show it over `monitor`,
    /// leaving `bottom_reserve` pixels free for the dock.
    pub fn show(&mut self, c: &Compositor, gfx: &Gfx, backdrop: &CompositionBrush, monitor: RECT, bottom_reserve: f32, scale: f32, d: &Data) -> Result<()> {
        let s = scale;
        self.scale = s;
        let (w, h) = ((monitor.right - monitor.left) as f32, (monitor.bottom - monitor.top) as f32);
        self.size = (w, h);
        self.query.clear();
        self.sel = 0;
        self.scroll_y = 0.0;
        self.settings = None;
        self.gallery = None;
        self.wdrag = None;
        self.cols.iter_mut().for_each(|c| c.scroll = 0.0);
        let root = &self.popup.root;
        root.Children()?.RemoveAll()?;
        self.panels.clear();

        // Full-screen frosted backdrop.
        let bd = c.CreateSpriteVisual()?;
        bd.SetBrush(backdrop)?;
        bd.SetSize(v2(w, h))?;
        root.Children()?.InsertAtTop(&bd)?;
        let tint = c.CreateSpriteVisual()?;
        tint.SetBrush(&c.CreateColorBrushWithColor(theme::scrim(0.30))?)?;
        tint.SetSize(v2(w, h))?;
        bd.Children()?.InsertAtTop(&tint)?;
        self.backdrop = Some(bd);

        // Panel geometry.
        let m0 = 24.0 * s;
        let gap = 18.0 * s;
        let top = m0;
        let ph = h - top - bottom_reserve - 16.0 * s;
        let (lw, rw) = (300.0 * s, 344.0 * s);
        let show_left = w - 2.0 * m0 - lw - rw - 2.0 * gap > 560.0 * s;
        let cw = if show_left { w - 2.0 * m0 - lw - rw - 2.0 * gap } else { w - 2.0 * m0 - rw - gap };
        let mut x = m0;
        let tint = theme::surface(0.52);
        let mut order = Vec::new();
        if show_left {
            order.push((PanelId::Left, x, lw));
            x += lw + gap;
        }
        order.push((PanelId::Center, x, cw));
        x += cw + gap;
        order.push((PanelId::Right, x, rw));
        for (k, (id, px, pw)) in order.into_iter().enumerate() {
            let p = ui::panel(c, gfx, backdrop, pw, ph, 28.0 * s, tint, s, d.cfg.panel_shadow)?;
            p.root.SetOffset(v3(px - p.margin, top - p.margin, 0.0))?;
            // Rise in, staggered.
            p.body.SetCenterPoint(v3(pw / 2.0, ph / 2.0, 0.0))?;
            let body: CompositionObject = p.body.cast()?;
            let _ = ui::animate_v3_from(c, &body, "Scale", v3(0.97, 0.97, 1.0), v3(1.0, 1.0, 1.0), 260 + 30 * k as i64);
            p.root.SetOpacity(0.0)?;
            let _ = ui::animate_scalar(c, &p.root.cast()?, "Opacity", 1.0, 180 + 40 * k as i64);
            root.Children()?.InsertAtTop(&p.root)?;
            self.panels.push((id, p, (px, top)));
        }
        // Hover + selection highlights live above the panels.
        let (hs_, hg, _) = ui::rounded(c, 10.0, 10.0, 10.0, theme::ink(0.10))?;
        hs_.SetOpacity(0.0)?;
        root.Children()?.InsertAtTop(&hs_)?;
        self.hover_shape = Some((hs_, hg));

        root.SetOpacity(0.0)?;
        ui::animate_scalar(c, &root.cast()?, "Opacity", 1.0, 160)?;
        self.render(c, gfx, d)?;
        // One pixel short of the monitor height on purpose: Windows treats a window that
        // exactly covers the monitor as "full screen" and rescales its composition content
        // (everything came out 1.5x too large). One pixel is invisible under the dock.
        self.popup.show(RECT { bottom: monitor.bottom - 1, ..monitor }, true);
        self.open = true;
        Ok(())
    }

    pub fn hide(&mut self) {
        if self.open {
            self.open = false;
            self.dragging = None;
            self.form = None;
            self.gallery = None;
            self.wdrag = None;
            self.hover_card = None;
            self.pressed_card = None;
            self.popup.hide();
        }
    }

    pub fn render(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        // Render every panel even if one fails, so one problem can't freeze the others.
        let a = self.render_left(c, gfx, d);
        let b = self.render_center(c, gfx, d);
        let r = self.render_right(c, gfx, d);
        a.and(b).and(r)
    }

    /// Redraw just the right panel (tray icons change often; the rest needn't redraw).
    pub fn render_right_panel(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        if self.open {
            self.render_right(c, gfx, d)
        } else {
            Ok(())
        }
    }

    fn panel(&self, id: PanelId) -> Option<(&Panel, (f32, f32))> {
        self.panels.iter().find(|p| p.0 == id).map(|p| (&p.1, p.2))
    }

    fn clear_hits(&mut self, id: PanelId) {
        self.hits.retain(|h| h.0 != id);
        self.hover = None;
        if let Some((s, _)) = &self.hover_shape {
            let _ = s.SetOpacity(0.0);
        }
    }

    // ------------------------------------------------------------ small builders

    fn txt(c: &Compositor, gfx: &Gfx, parent: &ContainerVisual, s: &str, st: TextStyle, x: f32, y: f32) -> Result<(f32, f32)> {
        let (v, w, h) = ui::text(c, gfx, s, &st)?;
        v.SetOffset(v3(x.round(), y.round(), 0.0))?;
        parent.Children()?.InsertAtTop(&v)?;
        Ok((w, h))
    }

    fn glyph(c: &Compositor, gfx: &Gfx, parent: &ContainerVisual, g: &str, px: f32, color: (f32, f32, f32, f32), cx: f32, cy: f32) -> Result<()> {
        let (v, w, h) = ui::text(c, gfx, g, &TextStyle { px, icons: true, color, ..Default::default() })?;
        v.SetOffset(v3((cx - w / 2.0).round(), (cy - h / 2.0).round(), 0.0))?;
        parent.Children()?.InsertAtTop(&v)?;
        Ok(())
    }

    fn rect(c: &Compositor, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, r: f32, col: windows::UI::Color) -> Result<ShapeVisual> {
        let (v, _, _) = ui::rounded(c, w, h, r, col)?;
        v.SetOffset(v3(x, y, 0.0))?;
        parent.Children()?.InsertAtTop(&v)?;
        Ok(v)
    }

    fn icon(c: &Compositor, parent: &ContainerVisual, b: Option<&CompositionSurfaceBrush>, x: f32, y: f32, size: f32) -> Result<()> {
        match b {
            Some(b) => {
                let sp = c.CreateSpriteVisual()?;
                sp.SetBrush(b)?;
                sp.SetSize(v2(size, size))?;
                sp.SetOffset(v3(x.round(), y.round(), 0.0))?;
                parent.Children()?.InsertAtTop(&sp)?;
            }
            None => {
                // Placeholder while the icon is being fetched.
                Self::rect(c, parent, x, y, size, size, size * 0.22, theme::ink(0.10))?;
            }
        }
        Ok(())
    }

    fn hit(&mut self, id: PanelId, r: (f32, f32, f32, f32), act: Act, radius: f32, highlight: bool, scroll: bool) {
        self.hits.push((id, Hit { r, act, radius, highlight, scroll, col: None }));
    }

    /// A hit in a widget column's scrolling list (content coordinates).
    fn hit_col(&mut self, ci: usize, r: (f32, f32, f32, f32), act: Act, radius: f32, highlight: bool) {
        let id = if ci == 0 { PanelId::Left } else { PanelId::Right };
        self.hits.push((id, Hit { r, act, radius, highlight, scroll: false, col: Some(ci) }));
    }

    // ------------------------------------------------------------ left: widgets

    fn render_left(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        self.render_widget_column(c, gfx, d)
    }

    // ------------------------------------------------------------ centre: search, apps, settings

    fn render_center(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        let Some((content, pw, ph, ox, oy)) = self.panel(PanelId::Center).map(|(p, (x, y))| (p.content.clone(), p.w, p.h, x, y)) else { return Ok(()) };
        self.clear_hits(PanelId::Center);
        content.Children()?.RemoveAll()?;
        let s = self.scale;
        let pad = 22.0 * s;

        if self.form.is_some() {
            return self.render_form(c, gfx, d, &content, (ox, oy), pw, ph);
        }
        if let Some(cat) = self.gallery {
            return self.render_gallery(c, gfx, d, &content, (ox, oy), pw, ph, cat);
        }
        if let Some(sec) = self.settings {
            return self.render_settings(c, gfx, d, &content, (ox, oy), pw, ph, sec);
        }

        // Search field
        let (fx, fy, fw, fh) = (pad, 18.0 * s, pw - 2.0 * pad, 46.0 * s);
        let (field, _, shape) = ui::rounded(c, fw, fh, fh / 2.0, theme::ink(0.10))?;
        shape.SetStrokeBrush(&c.CreateColorBrushWithColor(theme::ink(0.14))?)?;
        shape.SetStrokeThickness(1.0)?;
        field.SetOffset(v3(fx, fy, 0.0))?;
        content.Children()?.InsertAtTop(&field)?;
        Self::glyph(c, gfx, &content, G_SEARCH, 16.0 * s, theme::text_2(), fx + 24.0 * s, fy + fh / 2.0)?;
        let tx = fx + 44.0 * s;
        let (qw, qh) = if self.query.is_empty() {
            let (_, h) = Self::txt(c, gfx, &content, "Search apps, files and settings", TextStyle { px: 16.0 * s, color: theme::text_3(), ..Default::default() }, tx, fy + (fh - 22.0 * s) / 2.0)?;
            (0.0, h)
        } else {
            Self::txt(c, gfx, &content, &self.query, TextStyle { px: 16.0 * s, max_w: Some(fw - 80.0 * s), ..Default::default() }, tx, fy + (fh - 22.0 * s) / 2.0)?
        };
        // Blinking caret
        let (caret, _, _) = ui::rounded(c, (1.5 * s).max(1.0), 20.0 * s, 0.0, theme::accent())?;
        caret.SetOffset(v3(tx + qw + 1.0, fy + (fh - 20.0 * s) / 2.0, 0.0))?;
        let blink = c.CreateScalarKeyFrameAnimation()?;
        blink.InsertKeyFrame(0.0, 1.0)?;
        blink.InsertKeyFrame(0.49, 1.0)?;
        blink.InsertKeyFrame(0.5, 0.0)?;
        blink.InsertKeyFrame(0.99, 0.0)?;
        blink.SetDuration(ui::ms(1060))?;
        blink.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        caret.StartAnimation(&ui::hs("Opacity"), &blink)?;
        content.Children()?.InsertAtTop(&caret)?;
        self.caret = Some(caret);
        let _ = qh;
        self.hit(PanelId::Center, (ox + fx, oy + fy, fw, fh), Act::Search, fh / 2.0, false, false);

        // Scrolling area
        let top = fy + fh + 12.0 * s;
        let foot_h = 60.0 * s;
        let view_h = ph - top - foot_h;
        let host = c.CreateContainerVisual()?;
        host.SetOffset(v3(0.0, top, 0.0))?;
        host.SetSize(v2(pw, view_h))?;
        host.SetClip(&c.CreateInsetClip()?)?;
        content.Children()?.InsertAtTop(&host)?;
        let inner = c.CreateContainerVisual()?;
        host.Children()?.InsertAtTop(&inner)?;
        self.scroll_view = (ox, oy + top, pw, view_h);
        let mut y = 4.0 * s;

        if self.query.is_empty() {
            y = self.render_home(c, gfx, d, &inner, ox, oy + top, pw, y)?;
        } else {
            y = self.render_results(c, gfx, d, &inner, ox, oy + top, pw, y)?;
        }
        self.content_h = y + 12.0 * s;
        self.scroll_y = self.scroll_y.min((self.content_h - view_h).max(0.0));
        inner.SetOffset(v3(0.0, -self.scroll_y, 0.0))?;
        self.scroll_host = Some(inner);

        // Footer: user, settings, power
        let fy2 = ph - foot_h;
        Self::rect(c, &content, pad, fy2, pw - 2.0 * pad, 1.0, 0.0, theme::hair())?;
        let av = 32.0 * s;
        Self::rect(c, &content, pad, fy2 + (foot_h - av) / 2.0, av, av, av / 2.0, rgba(106, 77, 245, 1.0))?;
        let initial: String = d.user.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_else(|| "U".into());
        Self::txt(c, gfx, &content, &initial, TextStyle { px: 14.0 * s, weight: 600, center: true, max_w: Some(av), ..Default::default() }, pad, fy2 + (foot_h - 20.0 * s) / 2.0)?;
        Self::txt(c, gfx, &content, d.user, TextStyle { px: 13.0 * s, weight: 500, ..Default::default() }, pad + av + 10.0 * s, fy2 + (foot_h - 18.0 * s) / 2.0)?;
        let bs = 36.0 * s;
        let bx_power = pw - pad - bs;
        let bx_set = bx_power - bs - 6.0 * s;
        let by = fy2 + (foot_h - bs) / 2.0;
        Self::glyph(c, gfx, &content, G_SETTINGS, 16.0 * s, theme::text_2(), bx_set + bs / 2.0, by + bs / 2.0)?;
        Self::glyph(c, gfx, &content, G_POWER, 16.0 * s, theme::text_2(), bx_power + bs / 2.0, by + bs / 2.0)?;
        self.hit(PanelId::Center, (ox + bx_set, oy + by, bs, bs), Act::OpenSettings, bs / 2.0, true, false);
        self.hit(PanelId::Center, (ox + bx_power, oy + by, bs, bs), Act::Power, bs / 2.0, true, false);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_home(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, inner: &ContainerVisual, ox: f32, oy: f32, pw: f32, mut y: f32) -> Result<f32> {
        let s = self.scale;
        let pad = 22.0 * s;
        // Pinned header + "All apps" link
        Self::txt(c, gfx, inner, "Pinned", TextStyle { px: 13.0 * s, weight: 600, color: theme::text_2(), ..Default::default() }, pad + 4.0 * s, y)?;
        let link = if self.all_open { "Hide all apps" } else { "All apps" };
        let (lw, lh) = Self::txt(c, gfx, inner, link, TextStyle { px: 12.5 * s, weight: 500, color: (0.29, 0.62, 1.0, 1.0), ..Default::default() }, 0.0, -1000.0)?;
        let lx = pw - pad - lw - 6.0 * s;
        Self::txt(c, gfx, inner, link, TextStyle { px: 12.5 * s, weight: 500, color: (0.29, 0.62, 1.0, 1.0), ..Default::default() }, lx, y)?;
        self.hit(PanelId::Center, (ox + lx - 6.0 * s, oy + y - 3.0 * s, lw + 12.0 * s, lh + 6.0 * s), Act::ToggleAll, 6.0 * s, true, true);
        y += 30.0 * s;

        // Pinned grid
        let (cell_w, cell_h, icon) = (100.0 * s, 104.0 * s, 58.0 * s);
        let cols = (((pw - 2.0 * pad) / cell_w).floor() as usize).max(1);
        let gx = pad + ((pw - 2.0 * pad) - cols as f32 * cell_w) / 2.0;
        for (k, id) in d.pinned.iter().enumerate() {
            let (col, row) = (k % cols, k / cols);
            let x = gx + col as f32 * cell_w;
            let yy = y + row as f32 * cell_h;
            Self::icon(c, inner, d.brushes.get(id), x + (cell_w - icon) / 2.0, yy + 8.0 * s, icon)?;
            let name = d.names.get(id).cloned().unwrap_or_else(|| short_name(id));
            Self::txt(c, gfx, inner, &name, TextStyle { px: 12.0 * s, center: true, max_w: Some(cell_w - 8.0 * s), ..Default::default() }, x + 4.0 * s, yy + 8.0 * s + icon + 8.0 * s)?;
            self.hit(PanelId::Center, (ox + x, oy + yy, cell_w, cell_h - 4.0 * s), Act::App(id.clone()), 16.0 * s, true, true);
        }
        let rows = d.pinned.len().div_ceil(cols);
        y += rows as f32 * cell_h + 10.0 * s;

        // All Apps disclosure
        let dw = pw - 2.0 * pad;
        Self::glyph(c, gfx, inner, if self.all_open { G_CHEV_D } else { G_CHEV_R }, 11.0 * s, theme::text_2(), pad + dw - 18.0 * s, y + 19.0 * s)?;
        Self::txt(c, gfx, inner, &format!("All Apps   {}", d.apps.len()), TextStyle { px: 13.0 * s, weight: 600, color: theme::text_2(), ..Default::default() }, pad + 12.0 * s, y + 10.0 * s)?;
        self.hit(PanelId::Center, (ox + pad, oy + y, dw, 38.0 * s), Act::ToggleAll, 12.0 * s, true, true);
        y += 44.0 * s;

        if self.all_open {
            let col_w = 230.0 * s;
            let cols = (((pw - 2.0 * pad) / col_w).floor() as usize).max(1);
            let col_w = (pw - 2.0 * pad) / cols as f32;
            let row_h = 42.0 * s;
            let mut letter = ' ';
            let mut k = 0usize;
            for a in d.apps {
                let first = a.name.chars().next().map(|ch| ch.to_ascii_uppercase()).unwrap_or('#');
                let first = if first.is_ascii_alphabetic() { first } else { '#' };
                if first != letter {
                    if k % cols != 0 {
                        y += row_h;
                    }
                    k = 0;
                    letter = first;
                    Self::txt(c, gfx, inner, &letter.to_string(), TextStyle { px: 11.5 * s, weight: 700, color: theme::text_3(), ..Default::default() }, pad + 12.0 * s, y + 8.0 * s)?;
                    y += 28.0 * s;
                }
                let x = pad + (k % cols) as f32 * col_w;
                Self::icon(c, inner, d.brushes.get(&a.id), x + 10.0 * s, y + 6.0 * s, 30.0 * s)?;
                Self::txt(c, gfx, inner, &a.name, TextStyle { px: 13.0 * s, max_w: Some(col_w - 56.0 * s), ..Default::default() }, x + 50.0 * s, y + 11.0 * s)?;
                self.hit(PanelId::Center, (ox + x, oy + y, col_w - 6.0 * s, row_h - 2.0 * s), Act::App(a.id.clone()), 10.0 * s, true, true);
                k += 1;
                if k % cols == 0 {
                    y += row_h;
                }
            }
            if k % cols != 0 {
                y += row_h;
            }
        }
        Ok(y)
    }

    /// Recompute search results for the current query (apps ranked by fuzzy match).
    pub fn update_results(&mut self, d: &Data) {
        let q = self.query.trim().to_string();
        let mut out = Vec::new();
        if q.is_empty() {
            self.results.clear();
            self.results_len = 0;
            return;
        }
        // Fuzzy rank, then prefer apps where a whole word matches ("code" → Visual Studio
        // Code before "Codec …") and apps the user has pinned.
        let ql = q.to_lowercase();
        let mut ranked = laminar_sys::search::rank(&q, d.apps, 40);
        for r in ranked.iter_mut() {
            let name = r.0.name.to_lowercase();
            if name.split(|ch: char| !ch.is_alphanumeric()).any(|w| w == ql) {
                r.1 += 1500;
            }
            // Pinned apps win ties, but only for genuine matches (substring or better).
            if d.pinned.contains(&r.0.id) && r.1 > 500 {
                r.1 += 700;
            }
        }
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        for (a, _) in ranked.into_iter().take(8) {
            out.push(ResultItem::App(a.id.clone(), a.name.clone()));
        }
        let mut sets: Vec<(i32, ResultItem)> = Vec::new();
        for (name, uri) in WIN_SETTINGS {
            if let Some(sc) = laminar_sys::search::fuzzy_score(&q, name) {
                sets.push((sc, ResultItem::Setting(name, uri)));
            }
        }
        for (name, sec) in LAMINAR_SETTINGS {
            if let Some(sc) = laminar_sys::search::fuzzy_score(&q, &name["Laminar: ".len()..]).or_else(|| laminar_sys::search::fuzzy_score(&q, name)) {
                sets.push((sc, ResultItem::Laminar(name, *sec)));
            }
        }
        sets.sort_by(|a, b| b.0.cmp(&a.0));
        // Only reasonably strong settings matches.
        // Settings need a substring or word match (score > 500), not a loose letter sequence.
        let strong: Vec<ResultItem> = sets.into_iter().filter(|(sc, _)| *sc > 500).take(4).map(|(_, r)| r).collect();
        for f in d.files.iter().take(5) {
            out.push(ResultItem::File(f.clone()));
        }
        out.extend(strong);
        out.push(ResultItem::Web(q));
        self.results_len = out.len();
        self.sel = self.sel.min(out.len().saturating_sub(1));
        self.results = out;
    }

    #[allow(clippy::too_many_arguments)]
    fn render_results(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, inner: &ContainerVisual, ox: f32, oy: f32, pw: f32, mut y: f32) -> Result<f32> {
        let s = self.scale;
        let pad = 22.0 * s;
        let row_w = pw - 2.0 * pad;
        let mut last_section = "";
        let results = self.results.clone();
        for (i, r) in results.iter().enumerate() {
            let section = match r {
                _ if i == 0 => "Top Hit",
                ResultItem::App(..) => "Apps",
                ResultItem::File(_) => "Files",
                ResultItem::Setting(..) | ResultItem::Laminar(..) => "Settings",
                ResultItem::Web(_) => "Web",
            };
            if section != last_section {
                Self::txt(c, gfx, inner, section, TextStyle { px: 12.0 * s, weight: 600, color: theme::text_3(), ..Default::default() }, pad + 10.0 * s, y + 6.0 * s)?;
                y += 28.0 * s;
                last_section = section;
            }
            let top = i == 0;
            let rh = if top { 64.0 * s } else { 46.0 * s };
            let isz = if top { 44.0 * s } else { 30.0 * s };
            let (title, sub, act) = match r {
                ResultItem::App(id, name) => {
                    Self::icon(c, inner, d.brushes.get(id), pad + 10.0 * s, y + (rh - isz) / 2.0, isz)?;
                    (name.clone(), "Application".to_string(), Act::App(id.clone()))
                }
                ResultItem::File(f) => {
                    Self::glyph(c, gfx, inner, if f.is_folder { G_FOLDER } else { G_DOC }, if top { 26.0 } else { 18.0 } * s, theme::text_2(), pad + 10.0 * s + isz / 2.0, y + rh / 2.0)?;
                    let folder = std::path::Path::new(&f.path).parent().map(|p| p.display().to_string()).unwrap_or_default();
                    (f.name.clone(), folder, Act::File(f.path.clone()))
                }
                ResultItem::Setting(name, uri) => {
                    Self::glyph(c, gfx, inner, G_SETTINGS, if top { 24.0 } else { 17.0 } * s, theme::text_2(), pad + 10.0 * s + isz / 2.0, y + rh / 2.0)?;
                    (name.to_string(), "Windows Settings".to_string(), Act::Uri(uri.to_string()))
                }
                ResultItem::Laminar(name, sec) => {
                    Self::glyph(c, gfx, inner, G_SETTINGS, if top { 24.0 } else { 17.0 } * s, (0.55, 0.6, 1.0, 1.0), pad + 10.0 * s + isz / 2.0, y + rh / 2.0)?;
                    (name.to_string(), "Laminar Settings".to_string(), Act::Section(*sec))
                }
                ResultItem::Web(q) => {
                    Self::glyph(c, gfx, inner, G_GLOBE, if top { 24.0 } else { 17.0 } * s, theme::text_2(), pad + 10.0 * s + isz / 2.0, y + rh / 2.0)?;
                    (format!("Search the web for \u{201c}{q}\u{201d}"), "Opens your browser".to_string(), Act::Web(q.clone()))
                }
            };
            let tx = pad + 22.0 * s + isz;
            Self::txt(c, gfx, inner, &title, TextStyle { px: if top { 17.0 } else { 13.5 } * s, weight: if top { 600 } else { 500 }, max_w: Some(row_w - isz - 60.0 * s), ..Default::default() }, tx, y + rh / 2.0 - if top { 22.0 } else { 17.0 } * s)?;
            Self::txt(c, gfx, inner, &sub, TextStyle { px: 11.5 * s, color: theme::text_2(), max_w: Some(row_w - isz - 60.0 * s), ..Default::default() }, tx, y + rh / 2.0 + 1.0 * s)?;
            if i == self.sel {
                // A fresh highlight each render (a visual can only have one parent).
                let (ss, _, _) = ui::rounded(c, row_w, rh - 2.0 * s, 10.0 * s, rgba(10, 132, 255, 0.85))?;
                ss.SetOffset(v3(pad, y, 0.0))?;
                inner.Children()?.InsertAtBottom(&ss)?;
            }
            self.hit(PanelId::Center, (ox + pad, oy + y, row_w, rh - 2.0 * s), act, 10.0 * s, true, true);
            y += rh;
        }
        Ok(y)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_settings(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, content: &ContainerVisual, (ox, oy): (f32, f32), pw: f32, _ph: f32, sec: usize) -> Result<()> {
        let s = self.scale;
        let pad = 26.0 * s;
        Self::txt(c, gfx, content, "Laminar Settings", TextStyle { px: 22.0 * s, weight: 600, display: true, ..Default::default() }, pad, 22.0 * s)?;
        let (dw, dh) = Self::txt(c, gfx, content, "Done", TextStyle { px: 13.0 * s, weight: 600, color: (0.29, 0.62, 1.0, 1.0), ..Default::default() }, 0.0, -1000.0)?;
        let dx = pw - pad - dw - 8.0 * s;
        Self::txt(c, gfx, content, "Done", TextStyle { px: 13.0 * s, weight: 600, color: (0.29, 0.62, 1.0, 1.0), ..Default::default() }, dx, 30.0 * s)?;
        self.hit(PanelId::Center, (ox + dx - 10.0 * s, oy + 24.0 * s, dw + 20.0 * s, dh + 12.0 * s), Act::CloseSettings, 8.0 * s, true, false);

        // Section tabs (segmented control)
        let ty = 70.0 * s;
        let tab_w = 104.0 * s;
        let tw = tab_w * SECTION_NAMES.len() as f32 + 6.0 * s;
        Self::rect(c, content, pad, ty, tw, 34.0 * s, 10.0 * s, theme::ink(0.10))?;
        for (pos, &k) in TAB_ORDER.iter().enumerate() {
            let name = SECTION_NAMES[k];
            let x = pad + 3.0 * s + pos as f32 * tab_w;
            if k == sec {
                Self::rect(c, content, x, ty + 3.0 * s, tab_w, 28.0 * s, 8.0 * s, theme::ink(0.22))?;
            }
            Self::txt(c, gfx, content, name, TextStyle { px: 12.5 * s, weight: if k == sec { 600 } else { 400 }, center: true, max_w: Some(tab_w), color: if k == sec { theme::text() } else { theme::text_2() }, ..Default::default() }, x, ty + 9.0 * s)?;
            if k != sec {
                self.hit(PanelId::Center, (ox + x, oy + ty + 3.0 * s, tab_w, 28.0 * s), Act::Section(k), 8.0 * s, true, false);
            }
        }

        let mut y = ty + 56.0 * s;
        let row_w = pw - 2.0 * pad;
        if sec == 3 {
            let title = format!("Laminar Dock {}", d.app_version);
            let lines = [
                title.as_str(),
                d.app_update,
                "Rust · Windows Composition · Direct2D/DirectWrite",
                "Changes here apply immediately and are saved to config.json.",
            ];
            for (k, l) in lines.iter().enumerate() {
                Self::txt(c, gfx, content, l, TextStyle { px: if k == 0 { 15.0 } else { 13.0 } * s, weight: if k == 0 { 600 } else { 400 }, color: if k == 0 { theme::text() } else { theme::text_2() }, ..Default::default() }, pad, y)?;
                y += 28.0 * s;
            }
            y += 12.0 * s;
            let (bw, bh) = (180.0 * s, 34.0 * s);
            Self::rect(c, content, pad, y, bw, bh, 9.0 * s, theme::ink(0.14))?;
            Self::txt(c, gfx, content, "Open settings file", TextStyle { px: 12.5 * s, weight: 500, center: true, max_w: Some(bw), ..Default::default() }, pad, y + 8.0 * s)?;
            self.hit(PanelId::Center, (ox + pad, oy + y, bw, bh), Act::OpenConfig, 9.0 * s, true, false);
            let ux = pad + bw + 12.0 * s;
            Self::rect(c, content, ux, y, bw, bh, 9.0 * s, theme::ink(0.14))?;
            Self::txt(c, gfx, content, "Check for updates", TextStyle { px: 12.5 * s, weight: 500, center: true, max_w: Some(bw), ..Default::default() }, ux, y + 8.0 * s)?;
            self.hit(PanelId::Center, (ox + ux, oy + y, bw, bh), Act::CheckUpdates, 9.0 * s, true, false);
            y += bh + 20.0 * s;
        }
        // Widgets: a scrolling page (switches, then settings and every connection).
        let page = if sec == 4 {
            let top = y;
            let host = c.CreateContainerVisual()?;
            host.SetOffset(v3(0.0, top, 0.0))?;
            host.SetSize(v2(pw, _ph - top - 8.0 * s))?;
            host.SetClip(&c.CreateInsetClip()?)?;
            content.Children()?.InsertAtTop(&host)?;
            let inner = c.CreateContainerVisual()?;
            host.Children()?.InsertAtTop(&inner)?;
            self.scroll_view = (ox, oy + top, pw, _ph - top - 8.0 * s);
            y = 0.0;
            Some((inner, top))
        } else {
            None
        };
        let (content, oy, scroll) = match &page {
            Some((inner, top)) => (inner, oy + top, true),
            None => (content, oy, false),
        };
        let rows = section_rows(sec);
        Self::rect(c, content, pad, y, row_w, rows.len() as f32 * 56.0 * s, 14.0 * s, theme::ink(0.06))?;
        for (k, row) in rows.iter().enumerate() {
            let ry = y + k as f32 * 56.0 * s;
            if k > 0 {
                Self::rect(c, content, pad + 14.0 * s, ry, row_w - 28.0 * s, 1.0, 0.0, theme::hair())?;
            }
            let label_y = if row.hint.is_empty() { ry + 19.0 * s } else { ry + 11.0 * s };
            Self::txt(c, gfx, content, row.label, TextStyle { px: 13.0 * s, ..Default::default() }, pad + 16.0 * s, label_y)?;
            if !row.hint.is_empty() {
                Self::txt(c, gfx, content, row.hint, TextStyle { px: 11.0 * s, color: theme::text_2(), max_w: Some(row_w * 0.55), ..Default::default() }, pad + 16.0 * s, ry + 30.0 * s)?;
            }
            let v = cfg_get(d.cfg, row.key);
            match row.ctl {
                Ctl::Toggle => {
                    let (sw, sh) = (40.0 * s, 24.0 * s);
                    let x = pad + row_w - 16.0 * s - sw;
                    let yy = ry + (56.0 * s - sh) / 2.0;
                    Self::rect(c, content, x, yy, sw, sh, sh / 2.0, if v > 0.5 { theme::accent() } else { theme::ink(0.20) })?;
                    let k2 = sh - 4.0 * s;
                    Self::rect(c, content, if v > 0.5 { x + sw - k2 - 2.0 * s } else { x + 2.0 * s }, yy + 2.0 * s, k2, k2, k2 / 2.0, rgba(255, 255, 255, 1.0))?;
                    self.hit(PanelId::Center, (ox + x - 6.0 * s, oy + yy - 6.0 * s, sw + 12.0 * s, sh + 12.0 * s), Act::Toggle(row.key), sh / 2.0, false, scroll);
                }
                Ctl::Slider { min, max, unit, .. } => {
                    let tw2 = 220.0 * s;
                    let x = pad + row_w - 16.0 * s - tw2 - 64.0 * s;
                    let yy = ry + 56.0 * s / 2.0;
                    let frac = ((v - min) / (max - min)).clamp(0.0, 1.0);
                    Self::rect(c, content, x, yy - 3.0 * s, tw2, 6.0 * s, 3.0 * s, theme::ink(0.18))?;
                    Self::rect(c, content, x, yy - 3.0 * s, (tw2 * frac).max(6.0 * s), 6.0 * s, 3.0 * s, theme::accent())?;
                    let kd = 20.0 * s;
                    Self::rect(c, content, x + tw2 * frac - kd / 2.0, yy - kd / 2.0, kd, kd, kd / 2.0, rgba(255, 255, 255, 1.0))?;
                    let val = match unit {
                        "%" => format!("{:.0}%", v * 100.0),
                        "icons" => format!("{v:.1}"),
                        u => format!("{v:.0} {u}"),
                    };
                    Self::txt(c, gfx, content, &val, TextStyle { px: 12.0 * s, color: theme::text_2(), ..Default::default() }, x + tw2 + 14.0 * s, yy - 9.0 * s)?;
                    self.hit(PanelId::Center, (ox + x - 10.0 * s, oy + yy - 14.0 * s, tw2 + 20.0 * s, 28.0 * s), Act::Slider(row.key), 8.0 * s, false, scroll);
                }
            }
        }
        if let Some((inner, _)) = page {
            y += rows.len() as f32 * 56.0 * s + 24.0 * s;
            let end = self.render_widget_settings(c, gfx, d, &inner, (ox, oy), pw, y)?;
            self.content_h = end;
            self.scroll_y = self.scroll_y.min((end - self.scroll_view.3).max(0.0));
            inner.SetOffset(v3(0.0, -self.scroll_y, 0.0))?;
            self.scroll_host = Some(inner);
        }
        Ok(())
    }

    // ------------------------------------------------------------ right: control center

    fn render_right(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        let Some((content, pw, ph, ox, oy)) = self.panel(PanelId::Right).map(|(p, (x, y))| (p.content.clone(), p.w, p.h, x, y)) else { return Ok(()) };
        self.clear_hits(PanelId::Right);
        content.Children()?.RemoveAll()?;
        if self.wifi_open {
            return self.render_wifi(c, gfx, d, &content, pw, ph, ox, oy);
        }
        self.render_column(1, c, gfx, d)
    }

    /// The right panel's Wi-Fi view: on/off switch and the visible networks.
    #[allow(clippy::too_many_arguments)]
    fn render_wifi(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, content: &ContainerVisual, pw: f32, ph: f32, ox: f32, oy: f32) -> Result<()> {
        let s = self.scale;
        let pad = 16.0 * s;
        let cw = pw - 2.0 * pad;
        let card = theme::ink(0.07);
        let on = d.control.wifi;
        let mut y = pad;

        // Header: back to Control Center, title, switch.
        let hh = 40.0 * s;
        Self::glyph(c, gfx, content, G_CHEV_L, 13.0 * s, theme::text_2(), pad + 14.0 * s, y + hh / 2.0)?;
        Self::txt(c, gfx, content, "Wi-Fi", TextStyle { px: 16.0 * s, weight: 700, ..Default::default() }, pad + 30.0 * s, y + (hh - 22.0 * s) / 2.0)?;
        self.hit(PanelId::Right, (ox + pad, oy + y, cw * 0.55, hh), Act::WifiBack, 12.0 * s, true, false);
        if let Some(on) = on {
            let (tw, th) = (46.0 * s, 28.0 * s);
            let tx = pad + cw - tw - 6.0 * s;
            let ty = y + (hh - th) / 2.0;
            Self::rect(c, content, tx, ty, tw, th, th / 2.0, if on { theme::accent() } else { theme::ink(0.18) })?;
            let kd = th - 6.0 * s;
            Self::rect(c, content, if on { tx + tw - kd - 3.0 * s } else { tx + 3.0 * s }, ty + 3.0 * s, kd, kd, kd / 2.0, rgba(255, 255, 255, 0.95))?;
            self.hit(PanelId::Right, (ox + tx - 6.0 * s, oy + ty - 6.0 * s, tw + 12.0 * s, th + 12.0 * s), Act::Wifi, th / 2.0, false, false);
        }
        y += hh + 12.0 * s;

        // Networks card.
        let row_h = 40.0 * s;
        let foot_h = 2.0 * 44.0 * s + 12.0 * s;
        let max_rows = (((ph - y - foot_h - pad - 12.0 * s) / row_h).floor() as usize).max(1);
        let message: Option<String> = match d.wifi {
            _ if on == Some(false) => Some("Wi-Fi is off.".into()),
            None => Some("Looking for networks…".into()),
            Some(Err(5)) => Some("Windows needs Location access for desktop apps to list Wi-Fi networks.".into()),
            Some(Err(e)) => Some(format!("Couldn’t list networks (Windows error {e}).")),
            Some(Ok(v)) if v.is_empty() => Some("No networks found.".into()),
            Some(Ok(_)) => None,
        };
        match (&message, d.wifi) {
            (None, Some(Ok(nets))) => {
                let rows = nets.len().min(max_rows);
                Self::rect(c, content, pad, y, cw, rows as f32 * row_h + 12.0 * s, 18.0 * s, card)?;
                y += 6.0 * s;
                for n in nets.iter().take(rows) {
                    let bars = match n.signal_percent {
                        75.. => 3,
                        50..=74 => 2,
                        25..=49 => 1,
                        _ => 0,
                    };
                    let col = if n.connected { theme::text() } else { theme::text_2() };
                    Self::glyph(c, gfx, content, G_WIFI_BARS[bars], 14.0 * s, col, pad + 22.0 * s, y + row_h / 2.0)?;
                    let st = TextStyle { px: 12.5 * s, weight: if n.connected { 600 } else { 400 }, max_w: Some(cw - 120.0 * s), ..Default::default() };
                    Self::txt(c, gfx, content, &n.ssid, st, pad + 44.0 * s, y + (row_h - 17.0 * s) / 2.0)?;
                    let mut rx = pad + cw - 20.0 * s;
                    if n.connected {
                        Self::glyph(c, gfx, content, G_CHECK, 12.0 * s, theme::text(), rx, y + row_h / 2.0)?;
                        rx -= 22.0 * s;
                    }
                    if n.secured {
                        Self::glyph(c, gfx, content, G_LOCK, 11.0 * s, theme::text_3(), rx, y + row_h / 2.0)?;
                    }
                    if !n.connected {
                        self.hit(PanelId::Right, (ox + pad + 4.0 * s, oy + y, cw - 8.0 * s, row_h), Act::WifiJoin(n.ssid.clone()), 12.0 * s, true, false);
                    }
                    y += row_h;
                }
                y += 6.0 * s;
            }
            _ => {
                let text = message.unwrap_or_default();
                let mh = 72.0 * s;
                Self::rect(c, content, pad, y, cw, mh, 18.0 * s, card)?;
                Self::txt(c, gfx, content, &text, TextStyle { px: 12.0 * s, color: theme::text_2(), center: true, max_w: Some(cw - 32.0 * s), ..Default::default() }, pad + 16.0 * s, y + (mh - 17.0 * s) / 2.0)?;
                y += mh;
            }
        }
        y += 12.0 * s;

        // Footer: Windows' own list (for new networks and passwords), and Wi-Fi settings.
        let foot: [(&str, &str); 2] = if matches!(d.wifi, Some(Err(5))) {
            [("Open Location Settings", "ms-settings:privacy-location"), ("Wi-Fi Settings", "ms-settings:network-wifi")]
        } else {
            [("Other Networks…", "ms-availablenetworks:"), ("Wi-Fi Settings", "ms-settings:network-wifi")]
        };
        for (label, uri) in foot {
            let bh = 40.0 * s;
            Self::rect(c, content, pad, y, cw, bh, 14.0 * s, card)?;
            Self::txt(c, gfx, content, label, TextStyle { px: 12.5 * s, ..Default::default() }, pad + 16.0 * s, y + (bh - 18.0 * s) / 2.0)?;
            Self::glyph(c, gfx, content, G_CHEV_R, 10.0 * s, theme::text_3(), pad + cw - 18.0 * s, y + bh / 2.0)?;
            self.hit(PanelId::Right, (ox + pad, oy + y, cw, bh), Act::Uri(uri.into()), 14.0 * s, true, false);
            y += bh + 4.0 * s;
        }
        Ok(())
    }

    // ------------------------------------------------------------ input

    fn hit_at(&self, x: f32, y: f32) -> Option<usize> {
        let (vx, vy, vw, vh) = self.scroll_view;
        self.hits.iter().position(|(_, h)| {
            if let Some(ci) = h.col {
                if !inside(self.cols[ci].view, x, y) {
                    return false;
                }
                inside((h.r.0, h.r.1 - self.cols[ci].scroll, h.r.2, h.r.3), x, y)
            } else if h.scroll {
                if !inside((vx, vy, vw, vh), x, y) {
                    return false;
                }
                inside((h.r.0, h.r.1 - self.scroll_y, h.r.2, h.r.3), x, y)
            } else {
                inside(h.r, x, y)
            }
        })
    }

    pub fn on_move(&mut self, c: &Compositor, x: f32, y: f32) {
        let h = self.hit_at(x, y);
        if h == self.hover {
            return;
        }
        self.hover = h;
        // The card under the pointer shows its grip.
        let card = match h.map(|i| &self.hits[i].1.act) {
            Some(Act::Card(p) | Act::Widget(p, _, _)) => Some(*p),
            _ => None,
        };
        self.set_hover_card(c, card);
        // Background Apps names the tray icon under the pointer.
        self.hover_tray = match h.map(|i| &self.hits[i].1.act) {
            Some(Act::Widget(_, crate::widgets::WAct::Tray(k), _)) => Some(*k),
            _ => None,
        };
        let Some((shape, geom)) = &self.hover_shape else { return };
        let o: CompositionObject = shape.cast().unwrap();
        match h.map(|i| self.hits[i].1.clone()) {
            Some(hit) if hit.highlight => {
                let r = if let Some(ci) = hit.col {
                    (hit.r.0, hit.r.1 - self.cols[ci].scroll, hit.r.2, hit.r.3)
                } else if hit.scroll {
                    (hit.r.0, hit.r.1 - self.scroll_y, hit.r.2, hit.r.3)
                } else {
                    hit.r
                };
                let _ = geom.SetSize(v2(r.2, r.3));
                let _ = geom.SetCornerRadius(v2(hit.radius, hit.radius));
                let _ = shape.SetSize(v2(r.2, r.3));
                let _ = shape.SetOffset(v3(r.0, r.1, 0.0));
                let _ = ui::animate_scalar(c, &o, "Opacity", 1.0, 70);
            }
            _ => {
                let _ = ui::animate_scalar(c, &o, "Opacity", 0.0, 90);
            }
        }
    }

    /// Which action is under (x, y); clicks outside every panel close Launchpad.
    pub fn act_at(&self, x: f32, y: f32) -> Option<Act> {
        if let Some(i) = self.hit_at(x, y) {
            let h = &self.hits[i].1;
            if let Act::Widget(k, a, _) = &h.act {
                let frac = ((x - h.r.0) / h.r.2).clamp(0.0, 1.0);
                return Some(Act::Widget(*k, a.clone(), frac));
            }
            return Some(h.act.clone());
        }
        let in_panel = self.panels.iter().any(|(_, p, (px, py))| inside((*px, *py, p.w, p.h), x, y));
        if in_panel {
            None
        } else {
            Some(Act::Close)
        }
    }

    /// Slider value for a pointer x over the given control (window px), or None.
    pub fn slider_value(&self, act: &Act, x: f32) -> Option<f32> {
        let h = self.hits.iter().find(|(_, h)| &h.act == act)?.1.clone();
        let inset = 10.0 * self.scale;
        let frac = ((x - h.r.0 - inset) / (h.r.2 - 2.0 * inset)).clamp(0.0, 1.0);
        Some(frac)
    }

    pub fn slider_range(key: &str) -> Option<(f32, f32, f32)> {
        for sec in 0..SECTION_NAMES.len() {
            for r in section_rows(sec) {
                if r.key == key {
                    if let Ctl::Slider { min, max, step, .. } = r.ctl {
                        return Some((min, max, step));
                    }
                }
            }
        }
        None
    }

    pub fn on_wheel(&mut self, delta: f32) {
        let (_, _, _, vh) = self.scroll_view;
        let max = (self.content_h - vh).max(0.0);
        self.scroll_y = (self.scroll_y - delta).clamp(0.0, max);
        if let Some(inner) = &self.scroll_host {
            let _ = inner.SetOffset(v3(0.0, -self.scroll_y, 0.0));
        }
        self.hover = None;
        if let Some((s, _)) = &self.hover_shape {
            let _ = s.SetOpacity(0.0);
        }
    }

    /// The result row's action (Enter in search).
    pub fn selected_act(&self) -> Option<Act> {
        self.hits
            .iter()
            .filter(|(id, h)| *id == PanelId::Center && h.scroll && h.highlight)
            .map(|(_, h)| h.act.clone())
            .nth(self.sel)
    }

    pub fn key_char(&mut self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        self.settings = None;
        self.gallery = None;
        self.query.push(ch);
        self.sel = 0;
        self.scroll_y = 0.0;
        true
    }

    /// Returns true if the query changed.
    pub fn key_down(&mut self, vk: u16) -> bool {
        if vk == VK_BACK.0 {
            let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
            if ctrl {
                let t = self.query.trim_end().to_string();
                let cut = t.rfind(' ').map(|i| i + 1).unwrap_or(0);
                self.query.truncate(cut);
            } else {
                self.query.pop();
            }
            self.sel = 0;
            return true;
        }
        if vk == VK_DOWN.0 && self.results_len > 0 {
            self.sel = (self.sel + 1).min(self.results_len - 1);
        } else if vk == VK_UP.0 {
            self.sel = self.sel.saturating_sub(1);
        }
        false
    }
}

fn short_name(id: &str) -> String {
    let base = id.rsplit(['\\', '!']).next().unwrap_or(id);
    base.trim_end_matches(".exe").to_string()
}

// Widget column, gallery and connect form.
#[path = "launchpad_widgets.rs"]
mod launchpad_widgets;
pub use launchpad_widgets::{Column, DragSrc, Form, WDrag, WPos, Zone};
