//! The dock window and its Composition visual tree.
//!
//! Magnification is done with *expression animations*: Rust only writes the
//! pointer position into a property set, and the compositor computes every
//! icon's scale and position on the GPU each frame. The UI thread sleeps
//! unless something happens (pointer, click, a window event, a settings save).

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::Instant;

use windows::core::{w, Interface, Result, HSTRING};
use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::*;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmFlush, DwmSetWindowAttribute, DWMWA_EXCLUDED_FROM_PEEK, DWMWA_USE_HOSTBACKDROPBRUSH};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, GetMonitorInfoW, MonitorFromPoint, SetWindowRgn, MONITORINFO, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::Storage::FileSystem::{FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_LAST_WRITE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

use crate::apps;
use crate::config::{self, Config, Pin, SEPARATOR};
use crate::gfx::Gfx;
use crate::iconcache::IconCache;
use crate::menu::{Menu, MenuItem};
use crate::preview::{Hit as PreviewHit, Preview};
use crate::stack::{StackApp, StackView};
use crate::launchpad::{self as lp, Act, Launchpad};
use crate::services::{self, ControlState, Reply, Req, Services, WM_APP_SVC};
use laminar_sys::apps::AppEntry;
use laminar_sys::files::FileHit;
use laminar_sys::stats::Stats;
use laminar_sys::weather::Weather;
use crate::taskbar;
use crate::tracker::{self, Tracker, Win};
use crate::{metrics, Options};

/// Defined in Win32::UI::Controls; declared here to avoid pulling in that whole module.
const WM_MOUSELEAVE: u32 = 0x02A3;
pub const WM_APP_RESCAN: u32 = WM_APP + 1;
pub const WM_APP_APPBAR: u32 = WM_APP + 2;
const WM_APP_CONFIG: u32 = WM_APP + 3;

const T_SHRINK: usize = 1;
const T_MEASURE: usize = 2;
const T_EXIT: usize = 3;
const T_SWEEP: usize = 4;
const T_RESCAN: usize = 5;
const T_REVEAL: usize = 6;
const T_HIDE: usize = 7;
const T_CONFIG: usize = 8;
const T_TUCKED: usize = 9;
const T_GHOST: usize = 10;
const T_PREVIEW: usize = 11;
const T_PREVIEW_HIDE: usize = 12;
const T_MERGE: usize = 13;
const T_FILES: usize = 14;
const T_LP_TICK: usize = 15;
const T_WEATHER: usize = 16;
/// Keeps the tray stand-in window ahead of Explorer's.
const T_TRAY_RAISE: usize = 17;
/// Redraws Launchpad's tray icons shortly after they change (icons can animate quickly).
const T_TRAY_RENDER: usize = 18;
/// A drag rested on a running app: show its window tiles.
const T_DRAG_PREVIEW: usize = 19;
/// A drag rested on a window tile: bring that window forward.
const T_DRAG_SWITCH: usize = 20;
pub const WM_APP_WINKEY: u32 = WM_APP + 4;
/// Re-check whether the pointer is over the dock (after a drag that bypassed hover tracking).
const WM_APP_RESYNC_HOVER: u32 = WM_APP + 5;
/// The tray icons changed (see tray.rs).
const WM_APP_TRAY: u32 = WM_APP + 6;

// Menu command ids
const CMD_NEW: usize = 10;
const CMD_PIN: usize = 11;
const CMD_CLOSE: usize = 12;
const CMD_AUTOHIDE: usize = 20;
const CMD_TASKBAR: usize = 21;
const CMD_SETTINGS: usize = 22;
const CMD_QUIT: usize = 23;
const CMD_MAGNIFY: usize = 24;
const CMD_ADD_SEP: usize = 25;
const CMD_TRASH_OPEN: usize = 26;
const CMD_TASKMGR: usize = 27;
const CMD_WINSETTINGS: usize = 28;
const CMD_EXPLORER: usize = 29;
const CMD_TERMINAL: usize = 30;
const CMD_WINDOW_BASE: usize = 100;
const CMD_REMOVE_SEP_BASE: usize = 1000;
const CMD_STACK_OPEN: usize = 31;
const CMD_STACK_UNGROUP: usize = 32;
const CMD_STACK_REMOVE: usize = 33;
const CMD_NEW_STACK: usize = 34;
const CMD_STACK_RENAME: usize = 35;
const CMD_ADD_TO_STACK_BASE: usize = 2000;
const CMD_POWER_LOCK: usize = 40;
const CMD_POWER_SIGNOUT: usize = 41;
const CMD_POWER_SLEEP: usize = 42;
const CMD_POWER_RESTART: usize = 43;
const CMD_POWER_SHUTDOWN: usize = 44;
const CMD_LP_OPEN: usize = 50;
const CMD_LP_ADMIN: usize = 51;
const CMD_LP_DOCK: usize = 52;
const CMD_LP_PIN: usize = 53;
const CMD_LP_REVEAL: usize = 54;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Launch,
    App,
    Separator,
    Stack,
    Trash,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Item {
    pub kind: Kind,
    /// App id (as pinned or as running), or "::launch" / "::trash" / "::sep…".
    pub key: String,
    pub pinned: bool,
}

impl Item {
    fn is_icon(&self) -> bool {
        self.kind != Kind::Separator
    }
}

/// Layout in physical pixels for the current DPI and settings.
struct Geo {
    b: f32,
    m: f32,
    g: f32,
    p: f32,
    r: f32,
    margin: f32,
    range: f32,
    sep_w: f32,
    win_w: f32,
    win_h: f32,
    x0: f32,
    y0: f32,
    shelf_w: f32,
    shelf_h: f32,
    left: Vec<f32>,
    width: Vec<f32>,
    icon: Vec<bool>,
    hover_lr: (i32, i32),
}

impl Geo {
    fn new(items: &[Item], cfg: &Config, scale: f32, win_w: f32) -> Self {
        let b = cfg.icon_size * scale;
        let m = if cfg.magnify { cfg.magnified_size * scale } else { b };
        let (g, p) = (cfg.icon_spacing * scale, cfg.padding * scale);
        let sep_w = 13.0 * scale + g;
        let shelf_h = b + 2.0 * p;
        let r = (cfg.corner_radius * scale).min(shelf_h / 2.0);
        let margin = 8.0 * scale;
        let label_room = 40.0 * scale;
        let win_h = ((m - b) + shelf_h + margin + label_room).ceil();
        let (mut left, mut width, mut x) = (Vec::new(), Vec::new(), 0.0);
        for it in items {
            let w = if it.is_icon() { b + g } else { sep_w };
            left.push(x);
            width.push(w);
            x += w;
        }
        let shelf_w = x + 2.0 * p;
        let x0 = ((win_w - shelf_w) / 2.0).round();
        let y0 = win_h - margin - shelf_h;
        let mut geo = Geo {
            b, m, g, p, r, margin,
            range: cfg.magnify_range * (b + g),
            sep_w, win_w, win_h, x0, y0, shelf_w, shelf_h, left, width,
            icon: items.iter().map(|i| i.is_icon()).collect(),
            hover_lr: (0, 0),
        };
        // The hover region must contain the dock at every pointer position.
        let (mut lo, mut hi) = (x0, x0 + shelf_w);
        let mut px = x0 - geo.range;
        while px < x0 + shelf_w + geo.range {
            let (dx, scales) = geo.layout(px, 1.0);
            let grow: f32 = scales.iter().map(|s| (s - 1.0) * b).sum();
            lo = lo.min(dx);
            hi = hi.max(dx + shelf_w + grow);
            px += 2.0;
        }
        geo.hover_lr = ((lo - 4.0) as i32, (hi + 4.0) as i32);
        geo
    }

    fn center_abs(&self, i: usize) -> f32 {
        self.x0 + self.p + self.left[i] + self.width[i] / 2.0
    }

    /// Rust mirror of the compositor expressions: (shelf x, per-item scale).
    fn layout(&self, px: f32, amount: f32) -> (f32, Vec<f32>) {
        let k = self.m / self.b - 1.0;
        let scales: Vec<f32> = (0..self.left.len())
            .map(|i| {
                if !self.icon[i] {
                    return 1.0;
                }
                let d = ((px - self.center_abs(i)).abs() / self.range).min(1.0);
                1.0 + k * amount * ((d * PI).cos() + 1.0) / 2.0
            })
            .collect();
        let mut shift = 0.0;
        for i in 0..self.left.len() {
            let h = ((px - (self.x0 + self.p + self.left[i])) / self.width[i]).clamp(0.0, 1.0);
            shift += (scales[i] - 1.0) * self.b * h;
        }
        (self.x0 - shift, scales)
    }

    /// Centre x (window px) of item i when the pointer is at px, and its scale.
    fn slot_center(&self, px: f32, amount: f32, i: usize) -> (f32, f32) {
        let (dx, scales) = self.layout(px, amount);
        let acc: f32 = (0..i).map(|j| (scales[j] - 1.0) * self.b).sum();
        let l = dx + self.p + self.left[i] + acc + if self.icon[i] { self.g / 2.0 } else { 0.0 };
        (l + self.b * scales[i] / 2.0, scales[i])
    }

    /// Which icon is under (x, y)?
    fn hit(&self, x: f32, y: f32, amount: f32) -> Option<usize> {
        let (dx, scales) = self.layout(x, amount);
        let mut acc = 0.0;
        for i in 0..self.left.len() {
            let w = if self.icon[i] { self.b * scales[i] } else { 0.0 };
            let l = dx + self.p + self.left[i] + acc;
            let slot = self.width[i] + (scales[i] - 1.0) * self.b;
            if self.icon[i] && x >= l && x < l + slot && y >= self.y0 + self.p + self.b - w - self.g {
                return Some(i);
            }
            acc += (scales[i] - 1.0) * self.b;
        }
        None
    }
}

/// Files being dragged over the dock from another app (OLE drag and drop).
struct FileDrag {
    files: Vec<String>,
    /// Every file is a program, shortcut or folder: always pin, never "open with".
    pinnable: bool,
    /// Alt is held: pin documents too.
    alt: bool,
    /// Slot before which a gap is open for pinning.
    insert: Option<usize>,
}

/// An icon being dragged.
struct Drag {
    /// Item index in `items` of the dragged icon.
    index: usize,
    /// Slot before which the gap is open (None: no gap, e.g. pulled off the dock).
    insert: Option<usize>,
    removing: bool,
    width: f32,
    /// Item the dragged app is held over (to make or extend a stack), and whether the hold completed.
    merge: Option<usize>,
    merge_ready: bool,
    /// Set when the icon came from Launchpad (not from the dock); `index` is then unused.
    ext: Option<String>,
}

/// The floating copy of an icon that follows the pointer while dragging.
struct Ghost {
    popup: crate::ui::Popup,
    body: ContainerVisual,
    size: i32,
}

#[derive(Clone, Copy, PartialEq, Default)]
struct IndState {
    running: bool,
    focused: bool,
    /// One of the app's windows flashed for attention and hasn't been looked at yet.
    attention: bool,
}

/// The green "needs attention" dot beside an app's running dot: a solid core with a
/// soft glow behind it that breathes while the app is waiting for you.
#[derive(Clone)]
struct Att {
    vis: ContainerVisual,
    glow: SpriteVisual,
    core: SpriteVisual,
    /// "Shift": how far the running dot moves left so the pair stays centred.
    props: CompositionPropertySet,
}

/// Everything the window procedure needs.
struct Dock {
    hwnd: HWND,
    opts: Options,
    started: Instant,
    cfg: Config,
    monitor: RECT,
    scale: f32,
    compositor: Compositor,
    _target: DesktopWindowTarget,
    gfx: Gfx,
    icons: IconCache,
    brushes: HashMap<String, CompositionSurfaceBrush>,
    /// Tray icon pictures by icon key: (picture generation, brush).
    tray_brushes: HashMap<u64, (u64, CompositionSurfaceBrush)>,
    names: HashMap<String, String>,
    /// What glass shows: Windows' blurred desktop, plus any extra blur (see `update_glass`).
    backdrop: CompositionBrush,
    /// Windows' blurred desktop, unmodified.
    host_backdrop: CompositionBrush,
    ptr: CompositionPropertySet,
    root: ContainerVisual,
    dock: ContainerVisual,
    slots: Vec<ContainerVisual>,
    /// Running totals for the shelf shift (one property set per slot, chained).
    accs: Vec<CompositionPropertySet>,
    tiles: Vec<Option<Visual>>,
    inds: Vec<Option<SpriteVisual>>,
    ind_state: Vec<IndState>,
    /// Attention dots, one per item (None for non-apps).
    atts: Vec<Option<Att>>,
    /// Windows (HWND values) that asked for attention (FlashWindow) and haven't been focused.
    attention: std::collections::HashSet<isize>,
    label: ContainerVisual,
    label_geom: CompositionRoundedRectangleGeometry,
    label_text: SpriteVisual,
    items: Vec<Item>,
    geo: Geo,
    // windows
    tracker: Tracker,
    windows: Vec<Win>,
    running_order: Vec<String>,
    fg: HWND,
    resolved: HashMap<String, Option<String>>,
    // interaction
    hovering: bool,
    hover: Option<usize>,
    pressed: Option<usize>,
    tucked: bool,
    menu_open: bool,
    menu: Menu,
    /// Drag & drop in the dock (reordering, removing, pinning).
    drag: Option<Drag>,
    press: Option<(usize, f32, f32)>,
    ghost: Option<Ghost>,
    /// Live window previews above a hovered running app.
    preview: Preview,
    preview_pending: Option<usize>,
    /// Stack pop-up (grid of a stack's apps).
    stack_view: StackView,
    /// Stack (pin index) the open menu is about.
    menu_stack: Option<usize>,
    /// Launchpad and the data it shows (fetched by the services worker).
    launchpad: Launchpad,
    services: Services,
    lp_apps: Vec<AppEntry>,
    control: ControlState,
    stats: Option<Stats>,
    weather: Option<Weather>,
    files: Vec<FileHit>,
    files_gen: u64,
    icons_requested: std::collections::HashSet<String>,
    user_name: String,
    lp_drag: Option<Act>,
    /// Dock icon highlighted while files are dragged over it.
    drop_target: Option<usize>,
    file_drag: Option<FileDrag>,
    /// Visible Wi-Fi networks for Launchpad (None until asked), or the Windows error code.
    wifi: Option<std::result::Result<Vec<laminar_sys::radios::WifiNetwork>, u32>>,
    /// Windows Update status and when it was last asked for.
    update: Option<laminar_sys::update::UpdateStatus>,
    update_asked: Option<Instant>,
    /// Keeps the OLE drop target alive.
    _drop: Option<windows::Win32::System::Ole::IDropTarget>,
    _preview_drop: Option<windows::Win32::System::Ole::IDropTarget>,
    /// During a drag: the running app under the pointer (its tiles open after a pause).
    drag_under: Option<usize>,
    /// During a drag: the window tile under the pointer (it comes forward after a pause).
    drag_switch: Option<HWND>,
    /// The open previews were opened by a drag (they stay up while it moves to them).
    drag_preview: bool,
    /// App a Launchpad right-click menu is about.
    lp_menu_app: Option<String>,
    /// An app tile pressed in Launchpad (id, x, y) — becomes a drag into the dock if moved.
    lp_press: Option<(String, f32, f32)>,
    /// What the open menu is about: the app item (if any) and its windows.
    menu_ctx: Option<(Option<Item>, Vec<Win>)>,
    // system integration (off in --measure / --demo-hover)
    integrate: bool,
    /// This is a helper dock for an extra display (no taskbar takeover / Windows key).
    helper: bool,
    helpers: crate::displays::Helpers,
    appbar: bool,
    tray_original: Option<u32>,
    taskbar_hidden: bool,
    // measurement
    measure_step: u32,
    m_cpu0: f64,
    m_t0: Instant,
    report: Vec<(String, String)>,
    sweep_t0: Instant,
    sweep_moves: u32,
    last_scan_ms: f64,
    last_refresh_ms: f64,
}

thread_local! {
    static DOCK: RefCell<Option<Dock>> = const { RefCell::new(None) };
}

fn with_dock(f: impl FnOnce(&mut Dock)) {
    DOCK.with(|d| {
        if let Ok(mut g) = d.try_borrow_mut() {
            if let Some(dock) = g.as_mut() {
                f(dock);
            }
        }
    });
}

fn hs(s: &str) -> HSTRING {
    HSTRING::from(s)
}
fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
fn v3(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3 { X: x, Y: y, Z: z }
}
fn ms(n: i64) -> TimeSpan {
    TimeSpan { Duration: n * 10_000 }
}

/// Debug log to stderr when LAMINAR_TRACE is set, with milliseconds since start.
macro_rules! trace {
    ($($arg:tt)*) => {
        if crate::dock::tracing() {
            eprintln!("[{:>7.1} ms] {}", crate::dock::trace_clock(), format!($($arg)*));
        }
    };
}
pub(crate) fn tracing() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("LAMINAR_TRACE").is_ok())
}
pub(crate) fn trace_clock() -> f64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// Durations of the auto-hide slide out and back in.
const TUCK_MS: u32 = 300;
const SHOW_MS: u32 = 460;

/// Smooth start and finish (cubic), for motion that leaves the screen.
fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

/// Testing aid: LAMINAR_SLOWMO=20 makes dock motion 20x slower so it can be inspected.
fn slowmo() -> f32 {
    static V: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("LAMINAR_SLOWMO").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0f32).max(0.01))
}

/// Windows' "Animation effects" setting (Settings > Accessibility > Visual effects).
fn animations_enabled() -> bool {
    let mut on = windows::core::BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut on as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    on.as_bool()
}

/// False while a full-screen game, video or presentation is in front (don't peek over it).
fn quiet_ok() -> bool {
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS};
    unsafe { SHQueryUserNotificationState().map(|s| s == QUNS_ACCEPTS_NOTIFICATIONS).unwrap_or(true) }
}

/// Damped-spring progress at time t (0..1), the same curve the prototype uses.
fn spring(t: f32, bounce: f32) -> f32 {
    let zeta = (1.0 - bounce).max(0.36);
    let w = 6.2 / zeta;
    if zeta < 1.0 {
        let wd = w * (1.0 - zeta * zeta).sqrt();
        1.0 - (-zeta * w * t).exp() * ((wd * t).cos() + (zeta * w / wd) * (wd * t).sin())
    } else {
        1.0 - (-w * t).exp() * (1.0 + w * t)
    }
}

impl Dock {
    // ---------------------------------------------------------------- helpers

    fn expr(&self, text: &str, refs: &[(&str, &CompositionObject)]) -> Result<ExpressionAnimation> {
        let e = self.compositor.CreateExpressionAnimationWithExpression(&hs(text))?;
        e.SetReferenceParameter(&hs("ptr"), &self.ptr)?;
        for (name, obj) in refs {
            e.SetReferenceParameter(&hs(name), *obj)?;
        }
        Ok(e)
    }

    /// Glass = live blurred desktop (HostBackdropBrush) + tint, clipped to a rounded rectangle.
    fn glass(&self, tint: f32, radius: f32) -> Result<(ContainerVisual, CompositionRoundedRectangleGeometry)> {
        let c = &self.compositor;
        let cont = c.CreateContainerVisual()?;
        let blur = c.CreateSpriteVisual()?;
        blur.SetBrush(&self.backdrop)?;
        blur.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
        let t = c.CreateSpriteVisual()?;
        // Light theme: pale glass, a little denser so dark icons and dots read on any wallpaper.
        let tint_color = if crate::ui::theme::light() {
            Color { A: ((tint + 0.12).min(0.92) * 255.0) as u8, R: 244, G: 244, B: 248 }
        } else {
            Color { A: (tint * 255.0) as u8, R: 30, G: 30, B: 38 }
        };
        t.SetBrush(&c.CreateColorBrushWithColor(tint_color)?)?;
        t.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
        cont.Children()?.InsertAtTop(&blur)?;
        cont.Children()?.InsertAtTop(&t)?;
        let geom = c.CreateRoundedRectangleGeometry()?;
        geom.SetCornerRadius(v2(radius, radius))?;
        cont.SetClip(&c.CreateGeometricClipWithGeometry(&geom)?)?;
        Ok((cont, geom))
    }

    fn pin_path(&mut self, pin: &str) -> Option<String> {
        if !self.resolved.contains_key(pin) {
            let r = self.icons.target(pin); // cached on disk; asks the shell only for new pins
            self.resolved.insert(pin.to_string(), r);
        }
        self.resolved[pin].clone()
    }

    fn pin_matches(&mut self, pin: &str, w: &Win) -> bool {
        let p = pin.to_lowercase();
        p == w.app || p == w.exe || self.pin_path(pin).map(|r| r == w.exe).unwrap_or(false)
    }

    /// Windows belonging to a dock item, front to back.
    fn item_windows(&mut self, it: &Item) -> Vec<Win> {
        if it.kind != Kind::App {
            return Vec::new();
        }
        let wins = self.windows.clone();
        if it.pinned {
            wins.into_iter().filter(|w| self.pin_matches(&it.key, w)).collect()
        } else {
            wins.into_iter().filter(|w| w.app == it.key).collect()
        }
    }

    // ---------------------------------------------------------------- model

    fn compute_items(&mut self) -> Vec<Item> {
        let mut v = vec![Item { kind: Kind::Launch, key: "::launch".into(), pinned: true }];
        let pins = self.cfg.pins.clone();
        for (n, p) in pins.iter().enumerate() {
            match p {
                Pin::Id(id) if id == SEPARATOR => v.push(Item { kind: Kind::Separator, key: format!("::sep{n}"), pinned: true }),
                Pin::Id(id) => v.push(Item { kind: Kind::App, key: id.clone(), pinned: true }),
                Pin::Stack { .. } => v.push(Item { kind: Kind::Stack, key: format!("::stack{n}"), pinned: true }),
            }
        }
        // Running apps that aren't pinned, in the order they first appeared.
        let order = self.running_order.clone();
        let mut extra = Vec::new();
        for app in order {
            let wins: Vec<Win> = self.windows.iter().filter(|w| w.app == app).cloned().collect();
            let pinned = wins.iter().any(|w| pins.iter().filter_map(|p| p.app()).any(|p| self.pin_matches(p, w)));
            if !pinned {
                extra.push(Item { kind: Kind::App, key: app, pinned: false });
            }
        }
        if !extra.is_empty() {
            v.push(Item { kind: Kind::Separator, key: "::sep-run".into(), pinned: false });
            v.extend(extra);
        }
        v.push(Item { kind: Kind::Separator, key: "::sep-end".into(), pinned: false });
        v.push(Item { kind: Kind::Trash, key: "::trash".into(), pinned: true });
        v
    }

    /// (pin index, name, app ids) of a stack item.
    fn stack_of(&self, key: &str) -> Option<(usize, String, Vec<String>)> {
        let n: usize = key.strip_prefix("::stack")?.parse().ok()?;
        match self.cfg.pins.get(n)? {
            Pin::Stack { stack, apps } => Some((n, stack.clone(), apps.clone())),
            _ => None,
        }
    }

    /// Icon brush for an item, loading it (from the cache, or the shell once) if needed.
    fn ensure_brush(&mut self, it: &Item) -> bool {
        if it.kind == Kind::Stack {
            if let Some((_, _, apps)) = self.stack_of(&it.key) {
                for a in apps {
                    self.ensure_brush(&Item { kind: Kind::App, key: a, pinned: true });
                }
            }
            return true;
        }
        if it.kind == Kind::Separator || self.brushes.contains_key(&it.key) {
            return true;
        }
        let px = (self.cfg.magnified_size.max(self.cfg.icon_size) * self.scale).ceil();
        let surface = match it.kind {
            Kind::Launch => self.gfx.logo_surface(px).ok(),
            Kind::Trash => self.icons.get(&self.gfx.wic, apps::RECYCLE_BIN).and_then(|(n, src)| {
                self.names.insert(it.key.clone(), n);
                self.gfx.icon_surface(&src, px).ok()
            }),
            _ => {
                let exe = self.windows.iter().find(|w| w.app == it.key).map(|w| w.exe.clone());
                let got = self.icons.get(&self.gfx.wic, &it.key).or_else(|| exe.and_then(|e| self.icons.get(&self.gfx.wic, &e)));
                got.and_then(|(n, src)| {
                    self.names.insert(it.key.clone(), n);
                    self.gfx.icon_surface(&src, px).ok()
                })
            }
        };
        let Some(surface) = surface else { return false };
        let Ok(brush) = self.compositor.CreateSurfaceBrushWithSurface(&surface) else { return false };
        let _ = brush.SetStretch(CompositionStretch::Fill);
        let _ = brush.SetBitmapInterpolationMode(CompositionBitmapInterpolationMode::Linear);
        self.brushes.insert(it.key.clone(), brush);
        true
    }

    /// Recompute items; rebuild the visual tree only if they changed.
    fn refresh(&mut self, force: bool) {
        let candidates = self.compute_items();
        let ok: Vec<bool> = candidates.iter().map(|it| self.ensure_brush(it)).collect();
        // Drop apps whose icon couldn't load (e.g. uninstalled); no leading/trailing/doubled separators.
        let mut clean: Vec<Item> = Vec::new();
        for (it, ok) in candidates.into_iter().zip(ok) {
            if !ok {
                continue;
            }
            if it.kind == Kind::Separator && clean.last().map(|l| l.kind == Kind::Separator || l.kind == Kind::Launch).unwrap_or(true) {
                continue;
            }
            if it.kind == Kind::Trash && clean.last().map(|l| l.kind == Kind::Separator).unwrap_or(false) {
                // keep the separator before the trash
            }
            clean.push(it);
        }
        self.icons.flush();
        if force || clean != self.items {
            self.items = clean;
            if let Err(e) = self.rebuild() {
                eprintln!("rebuild failed: {e:?}");
            }
        }
        self.update_indicators();
    }

    // ---------------------------------------------------------------- visual tree

    fn rebuild(&mut self) -> Result<()> {
        trace!("rebuild: {} items on monitor ({}, {})-({}, {}) scale {}", self.items.len(), self.monitor.left, self.monitor.top, self.monitor.right, self.monitor.bottom, self.scale);
        self.geo = Geo::new(&self.items, &self.cfg, self.scale, (self.monitor.right - self.monitor.left) as f32);
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                self.monitor.left,
                self.monitor.bottom - self.geo.win_h as i32,
                self.geo.win_w as i32,
                self.geo.win_h as i32,
                SWP_NOACTIVATE,
            )?;
        }
        let c = self.compositor.clone();
        let geo_k = self.geo.m / self.geo.b - 1.0;

        self.dock.Children()?.RemoveAll()?;
        self.slots.clear();
        self.tiles.clear();
        self.inds.clear();
        self.ind_state.clear();
        self.atts.clear();
        self.hover = None;
        let _ = self.show_label(None);

        self.root.SetSize(v2(self.geo.win_w, self.geo.win_h))?;
        if !self.hovering {
            self.ptr.InsertScalar(&hs("PointerX"), self.geo.x0 + self.geo.shelf_w / 2.0)?;
        }

        let geo = &self.geo;
        // Slots (one container per item), created first so expressions can reference them.
        for it in &self.items {
            let slot = c.CreateContainerVisual()?;
            if it.kind == Kind::Stack {
                slot.SetSize(v2(geo.b, geo.b))?;
                slot.SetCenterPoint(v3(0.0, geo.b, 0.0))?;
                // A frosted tile with up to four mini icons.
                let tile = c.CreateContainerVisual()?;
                tile.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
                let (bg, _, bg_shape) = crate::ui::rounded(&c, geo.b, geo.b, geo.b * 0.24, crate::ui::theme::ink(0.16))?;
                bg_shape.SetStrokeBrush(&c.CreateColorBrushWithColor(crate::ui::theme::ink(0.28))?)?;
                bg_shape.SetStrokeThickness(1.0)?;
                tile.Children()?.InsertAtTop(&bg)?;
                let apps = self.stack_of(&it.key).map(|v| v.2).unwrap_or_default();
                let pad = geo.b * 0.15;
                let gap = geo.b * 0.07;
                let mini = (geo.b - 2.0 * pad - gap) / 2.0;
                for (k, a) in apps.iter().take(4).enumerate() {
                    if let Some(b) = self.brushes.get(a) {
                        let sp = c.CreateSpriteVisual()?;
                        sp.SetBrush(b)?;
                        sp.SetSize(v2(mini, mini))?;
                        sp.SetOffset(v3(pad + (k % 2) as f32 * (mini + gap), pad + (k / 2) as f32 * (mini + gap), 0.0))?;
                        tile.Children()?.InsertAtTop(&sp)?;
                    }
                }
                slot.Children()?.InsertAtTop(&tile)?;
                self.tiles.push(Some(tile.cast()?));
            } else if it.is_icon() {
                slot.SetSize(v2(geo.b, geo.b))?;
                slot.SetCenterPoint(v3(0.0, geo.b, 0.0))?; // grow up and to the right
                let tile = c.CreateSpriteVisual()?;
                tile.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
                tile.SetBrush(&self.brushes[&it.key])?;
                slot.Children()?.InsertAtTop(&tile)?;
                self.tiles.push(Some(tile.cast()?));
            } else {
                let line = c.CreateSpriteVisual()?;
                line.SetBrush(&c.CreateColorBrushWithColor(crate::ui::theme::ink(0.34))?)?;
                let lw = self.scale.round().max(1.0);
                line.SetSize(v2(lw, geo.b * 0.72))?;
                line.SetOffset(v3(((geo.sep_w - lw) / 2.0).round(), geo.b * 0.14, 0.0))?;
                slot.Children()?.InsertAtTop(&line)?;
                self.tiles.push(None);
            }
            self.slots.push(slot);
        }

        // Expressions are chained (each refers only to its left neighbour) so they stay short
        // however many icons there are: Windows rejects expression strings that are too long.
        let n = self.slots.len();
        let base_x = |i: usize| geo.p + geo.left[i] + if geo.icon[i] { geo.g / 2.0 } else { 0.0 };
        let grow_of = |name: &str, i: usize| {
            if geo.icon[i] { format!("(({name}.Scale.X - 1) * {b})", b = geo.b) } else { "0".to_string() }
        };

        // Each icon's scale: cosine falloff from the pointer, blended by Amount.
        for (i, slot) in self.slots.iter().enumerate() {
            if !geo.icon[i] {
                continue;
            }
            let s = format!(
                "1 + {geo_k} * ptr.Amount * ((Cos(Min(Abs(ptr.PointerX - {c}) / {r}, 1) * 3.14159265) + 1) / 2)",
                c = geo.center_abs(i),
                r = geo.range
            );
            slot.StartAnimation(&hs("Scale"), &self.expr(&format!("Vector3({s}, {s}, 1)"), &[])?)?;
        }
        // Per-slot animatable values (one property set per slot):
        //   Acc      running total of magnification growth left of the pointer (shelf shift)
        //   Gap      space opened before this slot while dragging (where an icon will land)
        //   Collapse space closed before this slot (the dragged icon's old place)
        self.accs.clear();
        for _ in 0..n {
            let acc = c.CreatePropertySet()?;
            acc.InsertScalar(&hs("Acc"), 0.0)?;
            acc.InsertScalar(&hs("Gap"), 0.0)?;
            acc.InsertScalar(&hs("Collapse"), 0.0)?;
            self.accs.push(acc);
        }
        // Each slot's position: left neighbour's position + the rest gap + the neighbour's growth
        // (+ drag gap − collapse).
        for i in 0..n {
            if i == 0 {
                self.slots[0].SetOffset(v3(base_x(0), geo.p, 0.0))?;
                continue;
            }
            let prev: CompositionObject = self.slots[i - 1].cast()?;
            let me: CompositionObject = self.accs[i].cast()?;
            let e = self.expr(
                &format!(
                    "Vector3(q.Offset.X + {d} + {g} + a.Gap - a.Collapse, {p}, 0)",
                    d = base_x(i) - base_x(i - 1),
                    g = grow_of("q", i - 1),
                    p = geo.p
                ),
                &[("q", &prev), ("a", &me)],
            )?;
            self.slots[i].StartAnimation(&hs("Offset"), &e)?;
        }
        // Shelf shift, so the point under the pointer stays put: a running total of the
        // growth of every icon (or the part of it) that lies left of the pointer.
        for i in 0..n {
            let term = if geo.icon[i] {
                format!(
                    "{g} * Clamp((ptr.PointerX - {l}) / {w}, 0, 1)",
                    g = grow_of("s", i),
                    l = geo.x0 + geo.p + geo.left[i],
                    w = geo.width[i]
                )
            } else {
                "0".to_string()
            };
            let slot_obj: CompositionObject = self.slots[i].cast()?;
            let e = if i == 0 {
                self.expr(&term, &[("s", &slot_obj)])?
            } else {
                let prev: CompositionObject = self.accs[i - 1].cast()?;
                self.expr(&format!("a.Acc + {term}"), &[("s", &slot_obj), ("a", &prev)])?
            };
            self.accs[i].StartAnimation(&hs("Acc"), &e)?;
        }
        let _ = self.ptr.StopAnimation(&hs("Delta"));
        self.ptr.InsertScalar(&hs("Delta"), 0.0)?;
        let total: CompositionObject = self.accs[n - 1].cast()?;
        self.dock.StartAnimation(
            &hs("Offset"),
            // ptr.Delta: net width change while dragging; recentre by half of it.
            &self.expr(&format!("Vector3({x0} - z.Acc - ptr.Delta / 2, {y0}, 0)", x0 = geo.x0, y0 = geo.y0), &[("z", &total)])?,
        )?;
        // Shelf width: rest width + how far the last slot moved + its own growth.
        let last: CompositionObject = self.slots[n - 1].cast()?;
        let size_expr = format!(
            "Vector2({w} + (last.Offset.X - {bl}) + {g}, {h})",
            w = geo.shelf_w,
            bl = base_x(n - 1),
            g = grow_of("last", n - 1),
            h = geo.shelf_h
        );
        let size_refs = [("last", &last)];

        let (glass, geom) = self.glass(self.cfg.glass_tint, geo.r)?;
        glass.StartAnimation(&hs("Size"), &self.expr(&size_expr, &size_refs)?)?;
        geom.StartAnimation(&hs("Size"), &self.expr(&size_expr, &size_refs)?)?;
        self.dock.Children()?.InsertAtBottom(&glass)?;
        // Rim highlight as a nine-grid so the rounded ends never stretch.
        let rim_w = (geo.r * 2.0 + 4.0).ceil();
        let rim_surface = self.gfx.rim_surface(rim_w, geo.shelf_h, geo.r, self.cfg.edge_highlight)?;
        let rim_brush = c.CreateSurfaceBrushWithSurface(&rim_surface)?;
        // Without an explicit Fill the nine-grid draws the source once, unstretched, in the centre.
        rim_brush.SetStretch(CompositionStretch::Fill)?;
        let nine = c.CreateNineGridBrush()?;
        nine.SetSource(&rim_brush)?;
        nine.SetLeftInset(geo.r + 1.0)?;
        nine.SetRightInset(geo.r + 1.0)?;
        nine.SetTopInset(0.0)?;
        nine.SetBottomInset(0.0)?;
        let rim = c.CreateSpriteVisual()?;
        rim.SetBrush(&nine)?;
        rim.StartAnimation(&hs("Size"), &self.expr(&size_expr, &size_refs)?)?;
        self.dock.Children()?.InsertAtTop(&rim)?;

        for slot in &self.slots {
            self.dock.Children()?.InsertAtTop(slot)?;
        }

        // Running indicators: a small light pill under each running app, centred on the
        // (magnified) icon but not scaled with it.
        let ind_brush = c.CreateColorBrushWithColor(if crate::ui::theme::light() { Color { A: 230, R: 28, G: 28, B: 34 } } else { Color { A: 255, R: 245, G: 245, B: 247 } })?;
        let d = (4.0 * self.scale).round();
        for (i, it) in self.items.iter().enumerate() {
            if it.kind != Kind::App {
                self.inds.push(None);
                self.ind_state.push(IndState::default());
                self.atts.push(None);
                continue;
            }
            let ind = c.CreateSpriteVisual()?;
            ind.SetBrush(&ind_brush)?;
            ind.SetSize(v2(d, d))?;
            ind.SetOpacity(0.0)?;
            let g2 = c.CreateRoundedRectangleGeometry()?;
            g2.SetCornerRadius(v2(d / 2.0, d / 2.0))?;
            let ind_obj: CompositionObject = ind.cast()?;
            g2.StartAnimation(&hs("Size"), &self.expr("me.Size", &[("me", &ind_obj)])?)?;
            ind.SetClip(&c.CreateGeometricClipWithGeometry(&g2)?)?;
            let slot_obj: CompositionObject = self.slots[i].cast()?;
            let props = c.CreatePropertySet()?;
            props.InsertScalar(&hs("Shift"), 0.0)?;
            let props_obj: CompositionObject = props.cast()?;
            let dot_bottom = geo.shelf_h - (2.5 * self.scale).round();
            ind.StartAnimation(
                &hs("Offset"),
                &self.expr(
                    &format!(
                        "Vector3(s.Offset.X + {b} * s.Scale.X / 2 - me.Size.X / 2 - p.Shift, {y} - me.Size.Y, 0)",
                        b = geo.b,
                        y = dot_bottom
                    ),
                    &[("s", &slot_obj), ("me", &ind_obj), ("p", &props_obj)],
                )?,
            )?;
            let att = self.make_att(&slot_obj, props, dot_bottom)?;
            self.dock.Children()?.InsertAtTop(&att.vis)?;
            self.dock.Children()?.InsertAtTop(&ind)?;
            self.inds.push(Some(ind));
            self.ind_state.push(IndState::default());
            self.atts.push(Some(att));
        }

        self.apply_region();
        if tracing() {
            let top = self.monitor.bottom - self.geo.win_h as i32;
            for (i, it) in self.items.iter().enumerate() {
                trace!("item {i} at ({}, {}) {:?} {}", self.monitor.left + self.geo.center_abs(i) as i32, top + (self.geo.y0 + self.geo.shelf_h / 2.0) as i32, it.kind, it.key);
            }
        }
        Ok(())
    }

    /// Size of the attention dot's parts: (dot, gap to the running dot, glow box).
    fn att_metrics(&self) -> (f32, f32, f32) {
        ((4.0 * self.scale).round(), (3.0 * self.scale).round(), (22.0 * self.scale).round())
    }

    /// Build an (invisible) attention dot for one app slot. It sits just right of the
    /// running dot; when shown, the running dot slides left by `Shift` so the pair is
    /// centred under the icon.
    fn make_att(&self, slot_obj: &CompositionObject, props: CompositionPropertySet, dot_bottom: f32) -> Result<Att> {
        let c = &self.compositor;
        let (d, gap, big) = self.att_metrics();
        let green = |a: u8| Color { A: a, R: 52, G: 211, B: 94 };
        let vis = c.CreateContainerVisual()?;
        vis.SetSize(v2(big, big))?;
        vis.SetOpacity(0.0)?;

        let glow = c.CreateSpriteVisual()?;
        glow.SetSize(v2(big, big))?;
        glow.SetCenterPoint(Vector3 { X: big / 2.0, Y: big / 2.0, Z: 0.0 })?;
        let rg = c.CreateRadialGradientBrush()?;
        let stops = rg.ColorStops()?;
        stops.Append(&c.CreateColorGradientStopWithOffsetAndColor(0.0, green(215))?)?;
        stops.Append(&c.CreateColorGradientStopWithOffsetAndColor(0.28, green(120))?)?;
        stops.Append(&c.CreateColorGradientStopWithOffsetAndColor(0.6, green(34))?)?;
        stops.Append(&c.CreateColorGradientStopWithOffsetAndColor(1.0, green(0))?)?;
        glow.SetBrush(&rg)?;

        let core = c.CreateSpriteVisual()?;
        core.SetSize(v2(d, d))?;
        core.SetOffset(Vector3 { X: (big - d) / 2.0, Y: (big - d) / 2.0, Z: 0.0 })?;
        core.SetBrush(&c.CreateColorBrushWithColor(green(255))?)?;
        let g = c.CreateRoundedRectangleGeometry()?;
        g.SetSize(v2(d, d))?;
        g.SetCornerRadius(v2(d / 2.0, d / 2.0))?;
        core.SetClip(&c.CreateGeometricClipWithGeometry(&g)?)?;

        vis.Children()?.InsertAtTop(&glow)?;
        vis.Children()?.InsertAtTop(&core)?;
        // Centre of the green dot: half a dot plus half the gap right of the icon's centre
        // (the running dot's centre moves the same distance left, see `show_att`).
        vis.StartAnimation(
            &hs("Offset"),
            &self.expr(
                &format!(
                    "Vector3(s.Offset.X + {b} * s.Scale.X / 2 + {dx} - {h}, {cy} - {h}, 0)",
                    b = self.geo.b,
                    dx = (d + gap) / 2.0,
                    h = big / 2.0,
                    cy = dot_bottom - d / 2.0
                ),
                &[("s", slot_obj)],
            )?,
        )?;
        Ok(Att { vis, glow, core, props })
    }

    /// Fade an attention dot in (and start it breathing) or out.
    fn show_att(&self, att: &Att, on: bool) {
        let c = &self.compositor;
        let (d, gap, _) = self.att_metrics();
        if let Ok(p) = att.props.cast::<CompositionObject>() {
            let _ = crate::ui::animate_scalar(c, &p, "Shift", if on { (gap + d) / 2.0 } else { 0.0 }, 380);
        }
        if let Ok(v) = att.vis.cast::<CompositionObject>() {
            let _ = crate::ui::animate_scalar(c, &v, "Opacity", if on { 1.0 } else { 0.0 }, if on { 480 } else { 260 });
        }
        if on {
            let _ = self.breathe(att);
        } else {
            // The container is fading out; the glow can stop where it is.
            let _ = att.glow.StopAnimation(&hs("Opacity"));
            let _ = att.glow.StopAnimation(&hs("Scale"));
            let _ = att.core.StopAnimation(&hs("Opacity"));
        }
    }

    /// A slow, eased pulse: the glow swells and brightens, then settles, forever.
    /// With Windows' "Animation effects" turned off the glow just stays lit.
    fn breathe(&self, att: &Att) -> Result<()> {
        let c = &self.compositor;
        if !animations_enabled() {
            att.glow.SetOpacity(0.85)?;
            att.glow.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 })?;
            return Ok(());
        }
        let ease = c.CreateCubicBezierEasingFunction(v2(0.45, 0.0), v2(0.55, 1.0))?;
        let period = ms(2400);
        let op = c.CreateScalarKeyFrameAnimation()?;
        op.InsertKeyFrameWithEasingFunction(0.0, 0.3, &ease)?;
        op.InsertKeyFrameWithEasingFunction(0.5, 1.0, &ease)?;
        op.InsertKeyFrameWithEasingFunction(1.0, 0.3, &ease)?;
        op.SetDuration(period)?;
        op.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        att.glow.StartAnimation(&hs("Opacity"), &op)?;
        let sc = c.CreateVector3KeyFrameAnimation()?;
        let s3 = |k: f32| Vector3 { X: k, Y: k, Z: 1.0 };
        sc.InsertKeyFrameWithEasingFunction(0.0, s3(0.7), &ease)?;
        sc.InsertKeyFrameWithEasingFunction(0.5, s3(1.15), &ease)?;
        sc.InsertKeyFrameWithEasingFunction(1.0, s3(0.7), &ease)?;
        sc.SetDuration(period)?;
        sc.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        att.glow.StartAnimation(&hs("Scale"), &sc)?;
        let co = c.CreateScalarKeyFrameAnimation()?;
        co.InsertKeyFrameWithEasingFunction(0.0, 0.75, &ease)?;
        co.InsertKeyFrameWithEasingFunction(0.5, 1.0, &ease)?;
        co.InsertKeyFrameWithEasingFunction(1.0, 0.75, &ease)?;
        co.SetDuration(period)?;
        co.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        att.core.StartAnimation(&hs("Opacity"), &co)
    }

    /// A window flashed its taskbar button (FlashWindow / FlashWindowEx), the way chat
    /// apps signal a new message. Mark it until it's focused or closed, and if the dock
    /// is auto-hidden, peek it up for a moment so the dot can be seen.
    fn on_flash(&mut self, h: HWND) {
        let root = unsafe { GetAncestor(h, GA_ROOTOWNER) };
        let target = self.windows.iter().find(|w| w.hwnd == h || w.hwnd == root).map(|w| w.hwnd).unwrap_or(root);
        // Hidden windows can't be looked at, so they never need attention. (WSL's hidden
        // "RemoteApp" window, msrdc.exe, "flashes" every few seconds; it used to keep
        // popping the auto-hidden dock up.)
        if target.is_invalid() || target == self.fg || unsafe { !IsWindowVisible(target).as_bool() } {
            return;
        }
        if !self.attention.insert(target.0 as isize) {
            return; // already marked (flashing windows send this repeatedly)
        }
        let tracked = self.windows.iter().any(|w| w.hwnd == target);
        trace!("attention: window {:?} flashed ({})", target.0, self.windows.iter().find(|w| w.hwnd == target).map(|w| w.app.as_str()).unwrap_or("not tracked yet"));
        self.update_indicators();
        // Only peek for a window the dock shows a dot for. Untracked windows (hidden helper
        // windows, tool windows) can flash over and over; they must not keep the dock up.
        if tracked && self.cfg.auto_hide && self.tucked && !self.launchpad.open && quiet_ok() {
            self.set_tucked(false);
            unsafe { SetTimer(Some(self.hwnd), T_HIDE, self.cfg.hide_delay_ms + 2600, None) };
        }
    }

    /// Animate indicator dots to match which apps are running / focused.
    fn update_indicators(&mut self) {
        let fg = self.fg;
        let items = self.items.clone();
        for (i, it) in items.iter().enumerate() {
            if i >= self.inds.len() {
                break;
            }
            let Some(ind) = self.inds[i].clone() else { continue };
            let wins = self.item_windows(it);
            let focused = wins.iter().any(|w| w.hwnd == fg);
            let attention = !focused && wins.iter().any(|w| self.attention.contains(&(w.hwnd.0 as isize)));
            let st = IndState { running: !wins.is_empty(), focused, attention };
            let prev = self.ind_state[i];
            if st == prev {
                continue;
            }
            self.ind_state[i] = st;
            if st.attention != prev.attention {
                trace!("attention dot {} for item {i} {}", if st.attention { "on" } else { "off" }, it.key);
                if let Some(Some(att)) = self.atts.get(i).cloned() {
                    self.show_att(&att, st.attention);
                }
            }
            let d = (4.0 * self.scale).round();
            let (w, op) = if st.focused { ((14.0 * self.scale).round(), 1.0) } else if st.running { (d, 0.55) } else { (d, 0.0) };
            let c = &self.compositor;
            if let (Ok(sa), Ok(oa), Ok(ease)) = (
                c.CreateVector2KeyFrameAnimation(),
                c.CreateScalarKeyFrameAnimation(),
                c.CreateCubicBezierEasingFunction(v2(0.2, 0.9), v2(0.25, 1.0)),
            ) {
                let _ = sa.InsertKeyFrameWithEasingFunction(1.0, v2(w, d), &ease);
                let _ = sa.SetDuration(ms(320));
                let _ = oa.InsertKeyFrame(1.0, op);
                let _ = oa.SetDuration(ms(220));
                let _ = ind.StartAnimation(&hs("Size"), &sa);
                let _ = ind.StartAnimation(&hs("Opacity"), &oa);
            }
        }
    }

    // ---------------------------------------------------------------- windows

    fn rescan(&mut self) {
        let t = Instant::now();
        self.windows = self.tracker.scan();
        if self.cfg.per_display_windows {
            let mine = self.monitor;
            self.windows.retain(|w| unsafe {
                let m = windows::Win32::Graphics::Gdi::MonitorFromWindow(w.hwnd, windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(m, &mut mi);
                mi.rcMonitor == mine
            });
        }
        self.last_scan_ms = t.elapsed().as_secs_f64() * 1000.0;
        self.fg = tracker::foreground();
        let fg = self.fg.0 as isize;
        let live: std::collections::HashSet<isize> = self.windows.iter().map(|w| w.hwnd.0 as isize).collect();
        self.attention.retain(|h| *h != fg && live.contains(h));
        let present: Vec<String> = self.windows.iter().map(|w| w.app.clone()).collect();
        self.running_order.retain(|a| present.contains(a));
        for a in present {
            if !self.running_order.contains(&a) {
                self.running_order.push(a);
            }
        }
        let t = Instant::now();
        self.refresh(false);
        self.last_refresh_ms = t.elapsed().as_secs_f64() * 1000.0;
    }

    // ---------------------------------------------------------------- hover, region, auto-hide

    fn region_rect(&self, hover: bool) -> RECT {
        let g = &self.geo;
        if self.tucked {
            return RECT {
                left: 0,
                top: (g.win_h - self.cfg.reveal_zone * self.scale).round() as i32,
                right: g.win_w as i32,
                bottom: g.win_h as i32,
            };
        }
        // When auto-hiding, include the margin below the shelf so the pointer resting
        // on the screen edge still counts as "on the dock".
        let bottom = if self.cfg.auto_hide { g.win_h } else { g.y0 + g.shelf_h } as i32;
        if hover {
            RECT { left: g.hover_lr.0, top: 0, right: g.hover_lr.1, bottom }
        } else {
            RECT { left: g.x0 as i32, top: g.y0 as i32, right: (g.x0 + g.shelf_w) as i32, bottom }
        }
    }

    fn apply_region(&self) {
        let r = self.region_rect(self.hovering);
        trace!("region {}x{} at ({}, {}) tucked={} hovering={}", r.right - r.left, r.bottom - r.top, r.left, r.top, self.tucked, self.hovering);
        unsafe {
            let rgn = CreateRectRgn(r.left, r.top, r.right, r.bottom);
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
    }

    fn animate_amount(&self, to: f32, duration_ms: i64) -> Result<()> {
        let c = &self.compositor;
        let a = c.CreateScalarKeyFrameAnimation()?;
        let ease = c.CreateCubicBezierEasingFunction(v2(0.25, 0.8), v2(0.25, 1.0))?;
        a.InsertKeyFrameWithEasingFunction(1.0, to, &ease)?;
        a.SetDuration(ms(duration_ms))?;
        self.ptr.StartAnimation(&hs("Amount"), &a)
    }

    /// Slide the whole dock off the bottom of the screen (or back) on a spring.
    fn set_tucked(&mut self, tucked: bool) {
        trace!("set_tucked({tucked}) was {}", self.tucked);
        if self.tucked == tucked {
            return;
        }
        self.tucked = tucked;
        let c = &self.compositor;
        // Slide only as far as needed to clear the shelf (the window also holds empty
        // headroom for magnified icons; sliding that too made the visible part vanish
        // in the first ~100 ms). Each keyframe is relative to the starting value, so an
        // interrupted hide/show continues smoothly from wherever the dock is.
        let distance = (self.geo.shelf_h + self.geo.margin + 10.0 * self.scale).ceil();
        let to = if tucked { distance } else { 0.0 };
        if let (Ok(a), Ok(lin)) = (c.CreateVector3KeyFrameAnimation(), c.CreateLinearEasingFunction()) {
            let n = 48;
            for k in 1..=n {
                let t = k as f32 / n as f32;
                // Hide: smooth ease-in-out. Show: a spring with a little overshoot.
                let p = if tucked { ease_in_out(t) } else { spring(t, 0.16) };
                let e = format!("this.StartingValue + (Vector3(0, {to}, 0) - this.StartingValue) * {p}");
                let _ = a.InsertExpressionKeyFrameWithEasingFunction(t, &hs(&e), &lin);
            }
            let dur = if tucked { TUCK_MS } else { SHOW_MS };
            let _ = a.SetDuration(ms((dur as f32 * slowmo()) as i64));
            let _ = self.root.StartAnimation(&hs("Offset"), &a);
        }
        if tucked {
            self.hovering = false;
            self.hover = None;
            let _ = self.show_label(None);
            let _ = self.animate_amount(0.0, 200);
            // The window region also clips drawing: shrinking it now would cut the slide-out
            // off after a frame or two. Shrink it once the dock is fully off-screen.
            unsafe { SetTimer(Some(self.hwnd), T_TUCKED, (TUCK_MS as f32 * slowmo()) as u32 + 30, None) };
        } else {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_TUCKED);
            }
            self.apply_region();
        }
    }

    fn on_move(&mut self, x: f32, y: f32) {
        if !self.hovering {
            trace!("pointer enters at ({x}, {y}) tucked={}", self.tucked);
        }
        if self.tucked {
            // Reveal after the pointer rests in the bottom band for reveal_delay.
            unsafe { SetTimer(Some(self.hwnd), T_REVEAL, self.cfg.reveal_delay_ms.max(1), None) };
            return;
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_HIDE);
        }
        if !self.hovering {
            self.hovering = true;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_SHRINK);
            }
            // Simulated pointers (--measure, --demo-hover) must not register for real leave events.
            if !self.synthetic() {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: self.hwnd,
                    dwHoverTime: 0,
                };
                unsafe {
                    let _ = TrackMouseEvent(&mut tme);
                }
            }
            self.apply_region();
            let _ = self.animate_amount(1.0, 160);
        }
        let _ = self.ptr.InsertScalar(&hs("PointerX"), x);
        let hit = self.geo.hit(x, y, 1.0);
        if hit != self.hover {
            self.hover = hit;
            self.preview_hover(hit);
            let _ = self.show_label(if self.preview.open { None } else { hit });
        }
    }

    fn on_leave(&mut self) {
        trace!("pointer leaves");
        if self.menu_open || self.drag.is_some() {
            return;
        }
        self.hovering = false;
        self.hover = None;
        self.pressed = None;
        let _ = self.show_label(None);
        let _ = self.animate_amount(0.0, 260);
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_PREVIEW);
            if self.preview.open {
                SetTimer(Some(self.hwnd), T_PREVIEW_HIDE, 300, None);
            }
        }
        unsafe {
            SetTimer(Some(self.hwnd), T_SHRINK, 320, None);
            if self.cfg.auto_hide {
                SetTimer(Some(self.hwnd), T_HIDE, self.cfg.hide_delay_ms.max(1), None);
            }
        }
    }

    /// Start hover tracking afresh from where the pointer really is: over the dock, it
    /// hovers (and Windows will report when it leaves); elsewhere, it leaves (auto-hide).
    fn resync_hover(&mut self) {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let win_top = self.monitor.bottom - self.geo.win_h as i32;
        let (x, y) = (p.x - self.monitor.left, p.y - win_top);
        let r = self.region_rect(true);
        let inside = !self.tucked
            && x >= r.left
            && x < r.right
            && y >= r.top
            && y < r.bottom
            && unsafe { WindowFromPoint(p) } == self.hwnd;
        trace!("resync hover: pointer at ({x}, {y}) inside={inside}");
        self.hovering = false;
        if inside {
            self.on_move(x as f32, y as f32);
        } else {
            self.on_leave();
        }
    }

    fn on_reveal_timer(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_REVEAL);
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let edge = self.monitor.bottom - (self.cfg.reveal_zone * self.scale).round() as i32 - 1;
            if p.y >= edge && p.x >= self.monitor.left && p.x < self.monitor.right {
                trace!("reveal: pointer rested at the bottom edge ({}, {})", p.x, p.y);
                self.set_tucked(false);
                // If the pointer never reaches the shelf, hide again.
                SetTimer(Some(self.hwnd), T_HIDE, self.cfg.hide_delay_ms + 900, None);
            }
        }
    }

    fn show_label(&mut self, idx: Option<usize>) -> Result<()> {
        let c = &self.compositor;
        let fade = c.CreateScalarKeyFrameAnimation()?;
        fade.SetDuration(ms(120))?;
        let Some(i) = idx else {
            fade.InsertKeyFrame(1.0, 0.0)?;
            return self.label.StartAnimation(&hs("Opacity"), &fade);
        };
        let s = self.scale;
        let it = &self.items[i];
        let name = match it.kind {
            Kind::Launch => "Launchpad".to_string(),
            Kind::Stack => self.stack_of(&it.key).map(|v| v.1).unwrap_or_else(|| "Stack".into()),
            _ => self.names.get(&it.key).cloned().unwrap_or_else(|| it.key.clone()),
        };
        let (surface, tw, th) = self.gfx.text_surface(&name, 13.0 * s)?;
        self.label_text.SetBrush(&c.CreateSurfaceBrushWithSurface(&surface)?)?;
        let (padx, pady) = (11.0 * s, 5.0 * s);
        let (lw, lh) = ((tw + 2.0 * padx).round(), (th + 2.0 * pady).round());
        self.label_text.SetSize(v2(tw, th))?;
        self.label_text.SetOffset(v3(padx, pady, 0.0))?;
        self.label.SetSize(v2(lw, lh))?;
        self.label_geom.SetSize(v2(lw, lh))?;
        let b = self.geo.b;
        let e = self.expr(
            &format!(
                "Vector3(d.Offset.X + t.Offset.X + {b} * t.Scale.X / 2 - {hw}, d.Offset.Y + t.Offset.Y + {b} - {b} * t.Scale.Y - {lh} - {gap}, 0)",
                hw = lw / 2.0,
                gap = 10.0 * s
            ),
            &[("d", &self.dock.cast()?), ("t", &self.slots[i].cast()?)],
        )?;
        self.label.StartAnimation(&hs("Offset"), &e)?;
        fade.InsertKeyFrame(1.0, 1.0)?;
        self.label.StartAnimation(&hs("Opacity"), &fade)
    }

    fn bounce(&self, i: usize) -> Result<()> {
        let Some(tile) = &self.tiles[i] else { return Ok(()) };
        let c = &self.compositor;
        let a = c.CreateScalarKeyFrameAnimation()?;
        let b = self.geo.b;
        let out = c.CreateCubicBezierEasingFunction(v2(0.2, 0.7), v2(0.4, 1.0))?;
        let inn = c.CreateCubicBezierEasingFunction(v2(0.6, 0.0), v2(0.8, 0.3))?;
        for (t, v, e) in [(0.22, -0.55, &out), (0.44, 0.0, &inn), (0.62, -0.28, &out), (0.8, 0.0, &inn), (1.0, 0.0, &out)] {
            a.InsertKeyFrameWithEasingFunction(t, v * b, e)?;
        }
        a.SetDuration(ms(1000))?;
        tile.StartAnimation(&hs("Offset.Y"), &a)
    }

    // ---------------------------------------------------------------- clicks

    /// Windows taskbar behaviour: launch if closed; bring to front if behind;
    /// minimize (or cycle windows) if already in front.
    fn on_click(&mut self, i: usize) {
        self.hide_preview();
        let it = self.items[i].clone();
        match it.kind {
            Kind::Launch => self.toggle_launchpad(),
            Kind::Trash => apps::launch(apps::RECYCLE_BIN),
            Kind::Separator => {}
            Kind::Stack => {
                if self.stack_view.open {
                    self.stack_view.hide();
                } else {
                    self.open_stack(i);
                }
            }
            Kind::App => {
                let wins = self.item_windows(&it);
                let fg = tracker::foreground();
                if wins.is_empty() {
                    let _ = self.bounce(i);
                    apps::launch(&it.key);
                } else if wins.iter().any(|w| w.hwnd == fg) {
                    if wins.len() == 1 && self.cfg.click_to_minimize {
                        tracker::minimize(wins[0].hwnd);
                    } else {
                        // Bring the back-most window forward: repeated clicks cycle through them.
                        tracker::activate(wins[wins.len() - 1].hwnd);
                    }
                } else {
                    tracker::activate(wins[0].hwnd);
                }
            }
        }
    }

    /// Right-click: build the menu for whatever is under the pointer and open it above the dock.
    fn context_menu(&mut self, x: f32, y: f32, anchor: Option<POINT>) {
        let hit = self.geo.hit(x, y, if self.hovering { 1.0 } else { 0.0 });
        let item = hit.map(|i| self.items[i].clone());
        let app = item.clone().filter(|it| it.kind == Kind::App);
        let wins = app.as_ref().map(|it| self.item_windows(it)).unwrap_or_default();
        let mut m: Vec<MenuItem> = Vec::new();
        if let Some(it) = &app {
            let name = self.names.get(&it.key).cloned().unwrap_or_else(|| it.key.clone());
            m.push(MenuItem::header(name));
            if !wins.is_empty() {
                for (i, w) in wins.iter().enumerate().take(8) {
                    let title = if w.title.chars().count() > 48 {
                        format!("{}…", w.title.chars().take(47).collect::<String>())
                    } else {
                        w.title.clone()
                    };
                    m.push(MenuItem::item(title, CMD_WINDOW_BASE + i).checked(w.hwnd == self.fg));
                }
                m.push(MenuItem::sep());
            }
            m.push(MenuItem::item("Open New Window", CMD_NEW).shortcut("Shift+Click"));
            m.push(MenuItem::sep());
            m.push(MenuItem::item("Keep in Dock", CMD_PIN).checked(it.pinned));
            if it.pinned {
                m.push(MenuItem::item("Add Separator After", CMD_ADD_SEP));
                for (n, p) in self.cfg.pins.iter().enumerate() {
                    if let Pin::Stack { stack, .. } = p {
                        m.push(MenuItem::item(format!("Move to \u{201c}{stack}\u{201d}"), CMD_ADD_TO_STACK_BASE + n));
                    }
                }
                m.push(MenuItem::item("New Stack", CMD_NEW_STACK));
            }
            if !wins.is_empty() {
                m.push(MenuItem::sep());
                let label = if wins.len() == 1 { "Close Window".to_string() } else { format!("Close All {} Windows", wins.len()) };
                m.push(MenuItem::item(label, CMD_CLOSE).danger());
            }
            m.push(MenuItem::sep());
        } else if let Some((n, name, _)) = item.as_ref().filter(|it| it.kind == Kind::Stack).and_then(|it| self.stack_of(&it.key)) {
            m.push(MenuItem::header(name));
            m.push(MenuItem::item("Open", CMD_STACK_OPEN));
            m.push(MenuItem::item("Rename Stack…", CMD_STACK_RENAME));
            m.push(MenuItem::item("Ungroup Stack", CMD_STACK_UNGROUP));
            m.push(MenuItem::item("Remove from Dock", CMD_STACK_REMOVE).danger());
            m.push(MenuItem::sep());
            self.menu_stack = Some(n);
        } else if item.as_ref().map(|it| it.kind == Kind::Trash).unwrap_or(false) {
            m.push(MenuItem::item("Open Recycle Bin", CMD_TRASH_OPEN));
            m.push(MenuItem::sep());
        } else if item.as_ref().map(|it| it.kind == Kind::Launch).unwrap_or(false) {
            m.push(MenuItem::item("Task Manager", CMD_TASKMGR).shortcut("Ctrl+Shift+Esc"));
            m.push(MenuItem::item("Settings", CMD_WINSETTINGS));
            m.push(MenuItem::item("File Explorer", CMD_EXPLORER));
            m.push(MenuItem::item("Terminal", CMD_TERMINAL));
            m.push(MenuItem::sep());
        }
        if let Some(sep) = self.separator_at(x) {
            m.push(MenuItem::item("Remove Separator", CMD_REMOVE_SEP_BASE + sep));
            m.push(MenuItem::sep());
        }
        m.push(MenuItem::item("Automatically Hide the Dock", CMD_AUTOHIDE).checked(self.cfg.auto_hide));
        m.push(MenuItem::item("Magnification", CMD_MAGNIFY).checked(self.cfg.magnify));
        m.push(MenuItem::item("Hide the Windows Taskbar", CMD_TASKBAR).checked(self.cfg.hide_windows_taskbar));
        m.push(MenuItem::sep());
        m.push(MenuItem::item("Dock Settings…", CMD_SETTINGS));
        m.push(MenuItem::item("Quit Laminar", CMD_QUIT));

        self.hide_preview();
        self.stack_view.hide();
        if !item.as_ref().map(|it| it.kind == Kind::Stack).unwrap_or(false) {
            self.menu_stack = None;
        }
        self.menu_ctx = Some((app, wins));
        self.menu_open = true;
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let p = anchor.unwrap_or(p);
        if let Err(e) = self.menu.show(&self.compositor, &self.gfx, &self.backdrop, m, p, true, self.scale, self.monitor) {
            trace!("menu failed: {e:?}");
            self.menu_open = false;
        }
    }

    /// Index into cfg.pins of a separator the pointer is on, if any.
    fn separator_at(&self, x: f32) -> Option<usize> {
        let (dx, scales) = self.geo.layout(x, if self.hovering { 1.0 } else { 0.0 });
        let mut acc = 0.0;
        for (i, it) in self.items.iter().enumerate() {
            let l = dx + self.geo.p + self.geo.left[i] + acc;
            if it.kind == Kind::Separator && it.pinned && x >= l - 4.0 && x < l + self.geo.sep_w + 4.0 {
                return it.key.strip_prefix("::sep").and_then(|n| n.parse().ok());
            }
            acc += (scales[i] - 1.0) * self.geo.b;
        }
        None
    }

    /// Test modes drive a simulated pointer; real pointer-leave events must be ignored.
    fn synthetic(&self) -> bool {
        self.opts.measure || self.opts.demo_hover || self.opts.demo.is_some()
    }

    /// --demo NAME: open a surface automatically so it can be captured and checked.
    fn run_demo(&mut self, name: &str) {
        let g = &self.geo;
        let i = 4.min(g.left.len().saturating_sub(1));
        let (x, y) = (g.center_abs(i), g.y0 + g.shelf_h / 2.0);
        let anchor = POINT { x: self.monitor.left + x as i32, y: self.monitor.bottom - g.win_h as i32 + g.y0 as i32 };
        match name {
            "menu" => {
                self.on_move(x, y);
                self.context_menu(x, y, Some(anchor));
            }
            "launchpad" | "lp-all" | "lp-search" | "lp-settings" | "lp-wifi" | "lp-wifi-sample" => {
                self.open_launchpad();
                match name {
                    "lp-all" => self.lp_act(Act::ToggleAll),
                    "lp-search" => {
                        for ch in "code".chars() {
                            self.launchpad.key_char(ch);
                        }
                        self.lp_query_changed();
                    }
                    "lp-settings" => self.lp_act(Act::OpenSettings),
                    "lp-wifi" => self.lp_act(Act::WifiMenu),
                    "lp-wifi-sample" => {
                        // Layout check with made-up networks (real replies are ignored in this mode).
                        use laminar_sys::radios::WifiNetwork;
                        let net = |ssid: &str, signal_percent, secured, connected, known| WifiNetwork { ssid: ssid.into(), signal_percent, secured, connected, known };
                        self.launchpad.wifi_open = true;
                        self.wifi = Some(Ok(vec![
                            net("Home Network", 92, true, true, true),
                            net("Home Network 5G", 80, true, false, true),
                            net("Coffee Shop Guest", 61, false, false, false),
                            net("Neighbour-2.4", 38, true, false, false),
                            net("Printer-Direct-7F", 12, true, false, false),
                        ]));
                        self.control.wifi = Some(true);
                        self.lp_render("all");
                    }
                    _ => {}
                }
            }
            "stack" => {
                if let Some(j) = self.items.iter().position(|it| it.kind == Kind::Stack) {
                    let (px, py) = (self.geo.center_abs(j), self.geo.y0 + self.geo.shelf_h / 2.0);
                    self.on_move(px, py);
                    self.open_stack(j);
                }
            }
            "preview" => {
                // The first running app with the most windows.
                let items = self.items.clone();
                let best = (0..items.len()).filter(|&j| items[j].kind == Kind::App).max_by_key(|&j| self.item_windows(&items[j]).len());
                if let Some(j) = best {
                    let (px, py) = (self.geo.center_abs(j), self.geo.y0 + self.geo.shelf_h / 2.0);
                    self.on_move(px, py);
                    self.show_preview(j);
                }
            }
            _ => {}
        }
    }

    fn menu_closed(&mut self) {
        self.menu_open = false;
        // Windows sent its one "pointer left" notice while the pointer was on the menu (and
        // it was ignored then), so start hover tracking afresh from where the pointer is.
        // Just checking "is it over the dock?" wasn't enough: a menu item sitting over the
        // dock's window left the dock thinking it was hovered, with no leave notice to come.
        if self.drag.is_none() {
            self.resync_hover();
        }
    }

    fn save_and_apply(&mut self, f: impl FnOnce(&mut Config)) {
        let mut cfg = self.cfg.clone();
        f(&mut cfg);
        cfg.save();
        self.apply_config(cfg);
    }

    fn on_menu_command(&mut self, cmd: usize) {
        let (item, wins) = self.menu_ctx.take().unwrap_or((None, Vec::new()));
        match cmd {
            CMD_NEW => {
                if let Some(it) = &item {
                    apps::launch(&it.key);
                }
            }
            CMD_PIN => {
                if let Some(it) = item {
                    self.save_and_apply(|c| {
                        if it.pinned {
                            c.pins.retain(|p| p.app() != Some(it.key.as_str()));
                        } else {
                            c.pins.push(Pin::Id(it.key.clone()));
                        }
                    });
                }
            }
            CMD_ADD_SEP => {
                if let Some(it) = item {
                    self.save_and_apply(|c| {
                        if let Some(i) = c.pins.iter().position(|p| p.app() == Some(it.key.as_str())) {
                            c.pins.insert(i + 1, Pin::sep());
                        }
                    });
                }
            }
            CMD_CLOSE => wins.iter().for_each(|w| tracker::close(w.hwnd)),
            CMD_TRASH_OPEN => apps::launch(apps::RECYCLE_BIN),
            CMD_TASKMGR => apps::launch("C:\\Windows\\System32\\Taskmgr.exe"),
            CMD_WINSETTINGS => apps::launch("ms-settings:"),
            CMD_EXPLORER => apps::launch("Microsoft.Windows.Explorer"),
            CMD_TERMINAL => apps::launch("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
            CMD_AUTOHIDE => self.save_and_apply(|c| c.auto_hide = !c.auto_hide),
            CMD_MAGNIFY => self.save_and_apply(|c| c.magnify = !c.magnify),
            CMD_TASKBAR => self.save_and_apply(|c| c.hide_windows_taskbar = !c.hide_windows_taskbar),
            CMD_SETTINGS => {
                // The Settings view lives in Launchpad's centre panel.
                if !self.launchpad.open {
                    self.open_launchpad();
                }
                self.lp_act(Act::OpenSettings);
            }
            CMD_QUIT => unsafe {
                // Clean up here: WM_DESTROY can't reach the dock while this handler holds it.
                self.shutdown();
                let _ = DestroyWindow(self.hwnd);
            },
            CMD_LP_OPEN | CMD_LP_ADMIN | CMD_LP_DOCK | CMD_LP_PIN | CMD_LP_REVEAL => {
                if let Some(id) = self.lp_menu_app.take() {
                    match cmd {
                        CMD_LP_OPEN => {
                            self.close_launchpad();
                            self.open_app(&id);
                        }
                        CMD_LP_ADMIN => {
                            self.close_launchpad();
                            apps::launch_as_admin(&id);
                        }
                        CMD_LP_DOCK => {
                            let pinned = self.cfg.pins.iter().any(|p| p.app() == Some(id.as_str()));
                            self.save_and_apply(|c| {
                                if pinned {
                                    c.pins.retain(|p| p.app() != Some(id.as_str()));
                                } else {
                                    c.pins.push(Pin::Id(id.clone()));
                                }
                            });
                        }
                        CMD_LP_PIN => {
                            let current = self.lp_pinned();
                            self.save_and_apply(|c| {
                                let mut list = current;
                                if let Some(i) = list.iter().position(|a| a == &id) {
                                    list.remove(i);
                                } else {
                                    list.push(id.clone());
                                }
                                c.launchpad_pins = list;
                            });
                            self.lp_render("all");
                        }
                        _ => {
                            if let Some(path) = self.pin_path(&id) {
                                self.close_launchpad();
                                apps::reveal(&path);
                            }
                        }
                    }
                }
            }
            CMD_POWER_LOCK => {
                self.close_launchpad();
                self.services.send(Req::Power(services::Power::Lock));
            }
            CMD_POWER_SIGNOUT => self.services.send(Req::Power(services::Power::SignOut)),
            CMD_POWER_SLEEP => {
                self.close_launchpad();
                self.services.send(Req::Power(services::Power::Sleep));
            }
            CMD_POWER_RESTART => self.services.send(Req::Power(services::Power::Restart)),
            CMD_POWER_SHUTDOWN => self.services.send(Req::Power(services::Power::ShutDown)),
            CMD_STACK_OPEN => {
                if let Some(n) = self.menu_stack.take() {
                    if let Some(i) = self.items.iter().position(|it| it.key == format!("::stack{n}")) {
                        self.open_stack(i);
                    }
                }
            }
            CMD_STACK_RENAME => {
                if let Some(n) = self.menu_stack.take() {
                    if let Some(i) = self.items.iter().position(|it| it.key == format!("::stack{n}")) {
                        self.open_stack(i);
                        if self.stack_view.open {
                            let c = self.compositor.clone();
                            self.stack_view.begin_rename(&c, &self.gfx);
                        }
                    }
                }
            }
            CMD_STACK_UNGROUP => {
                if let Some(n) = self.menu_stack.take() {
                    self.save_and_apply(|c| {
                        if let Some(Pin::Stack { apps, .. }) = c.pins.get(n).cloned() {
                            let existing: Vec<String> = c.pins.iter().filter_map(|p| p.app().map(String::from)).collect();
                            let add: Vec<Pin> = apps.into_iter().filter(|a| !existing.contains(a)).map(Pin::Id).collect();
                            c.pins.splice(n..n + 1, add);
                        }
                    });
                }
            }
            CMD_STACK_REMOVE => {
                if let Some(n) = self.menu_stack.take() {
                    self.save_and_apply(|c| {
                        if matches!(c.pins.get(n), Some(Pin::Stack { .. })) {
                            c.pins.remove(n);
                        }
                    });
                }
            }
            CMD_NEW_STACK => {
                if let Some(it) = item {
                    self.save_and_apply(|c| {
                        if let Some(i) = c.pins.iter().position(|p| p.app() == Some(it.key.as_str())) {
                            c.pins[i] = Pin::Stack { stack: "New Stack".into(), apps: vec![it.key.clone()] };
                        }
                    });
                }
            }
            c if c >= CMD_ADD_TO_STACK_BASE => {
                let n = c - CMD_ADD_TO_STACK_BASE;
                if let Some(it) = item {
                    self.save_and_apply(|cfg| {
                        if let Some(Pin::Stack { apps, .. }) = cfg.pins.get_mut(n) {
                            if !apps.contains(&it.key) {
                                apps.push(it.key.clone());
                            }
                        }
                        cfg.pins.retain(|p| p.app() != Some(it.key.as_str()));
                    });
                }
            }
            c if (CMD_WINDOW_BASE..CMD_WINDOW_BASE + 8).contains(&c) => {
                if let Some(w) = wins.get(c - CMD_WINDOW_BASE) {
                    tracker::activate(w.hwnd);
                }
            }
            c if (CMD_REMOVE_SEP_BASE..CMD_ADD_TO_STACK_BASE).contains(&c) => {
                let idx = c - CMD_REMOVE_SEP_BASE;
                self.save_and_apply(|cfg| {
                    if cfg.pins.get(idx).map(|p| p.is_sep()).unwrap_or(false) {
                        cfg.pins.remove(idx);
                    }
                });
            }
            _ => {}
        }
    }

    fn menu_choose(&mut self, cmd: Option<usize>) {
        self.menu.hide(true);
        self.menu_closed();
        if let Some(c) = cmd {
            self.on_menu_command(c);
        }
    }

    /// Messages for pop-up windows (menus, previews, stacks, Launchpad, Settings).
    fn popup_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if hwnd == self.launchpad.hwnd() {
            let (x, y) = lparam_xy(lp);
            match msg {
                WM_MOUSEMOVE => {
                    if let Some(a) = self.lp_drag.clone() {
                        self.lp_slider(&a, x, false);
                    } else if let Some((id, px, py)) = self.lp_press.clone() {
                        if ((x - px).powi(2) + (y - py).powi(2)).sqrt() > 8.0 * self.scale {
                            // Dragging an app out of Launchpad: hand it to the dock.
                            self.lp_press = None;
                            self.close_launchpad();
                            self.start_external_drag(&id);
                        }
                    } else {
                        self.launchpad.on_move(&self.compositor, x, y);
                    }
                }
                WM_LBUTTONDOWN => {
                    if let Some(Act::App(id)) = self.launchpad.act_at(x, y) {
                        self.lp_press = Some((id, x, y));
                    }
                    if let Some(a) = self.launchpad.act_at(x, y) {
                        if matches!(a, Act::Volume | Act::Slider(_)) {
                            self.lp_drag = Some(a.clone());
                            unsafe {
                                SetCapture(hwnd);
                            }
                            self.lp_slider(&a, x, false);
                        }
                    }
                }
                WM_LBUTTONUP => {
                    self.lp_press = None;
                    if let Some(a) = self.lp_drag.take() {
                        unsafe {
                            let _ = ReleaseCapture();
                        }
                        self.lp_slider(&a, x, true);
                    } else if let Some(a) = self.launchpad.act_at(x, y) {
                        self.lp_act(a);
                    }
                }
                WM_RBUTTONUP => {
                    if let Some(Act::Tray(key)) = self.launchpad.act_at(x, y) {
                        self.tray_click(key, crate::tray::Click::Right);
                    } else if let Some(Act::App(id)) = self.launchpad.act_at(x, y) {
                        let in_dock = self.cfg.pins.iter().any(|p| p.app() == Some(id.as_str()));
                        let in_lp = self.lp_pinned().contains(&id);
                        let has_path = self.pin_path(&id).is_some();
                        let name = self.names.get(&id).cloned().unwrap_or_else(|| id.clone());
                        let mut items = vec![
                            MenuItem::header(name),
                            MenuItem::item("Open", CMD_LP_OPEN),
                            MenuItem::item("Run as Administrator", CMD_LP_ADMIN),
                            MenuItem::sep(),
                            MenuItem::item("Keep in Dock", CMD_LP_DOCK).checked(in_dock),
                            MenuItem::item(if in_lp { "Unpin from Launchpad" } else { "Pin to Launchpad" }, CMD_LP_PIN),
                        ];
                        if has_path {
                            items.push(MenuItem::item("Open File Location", CMD_LP_REVEAL));
                        }
                        self.lp_menu_app = Some(id);
                        self.menu_ctx = None;
                        self.menu_open = true;
                        let mut p = POINT::default();
                        unsafe {
                            let _ = GetCursorPos(&mut p);
                        }
                        let _ = self.menu.show(&self.compositor, &self.gfx, &self.backdrop, items, p, false, self.scale, self.monitor);
                    }
                }
                WM_MOUSEWHEEL => {
                    let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as f32 / 120.0;
                    self.launchpad.on_wheel(delta * 72.0 * self.scale);
                }
                WM_CHAR => {
                    let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
                    if self.launchpad.key_char(ch) {
                        self.lp_query_changed();
                    }
                }
                WM_KEYDOWN => {
                    let vk = wp.0 as u16;
                    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP};
                    if vk == VK_ESCAPE.0 {
                        if self.launchpad.settings.is_some() {
                            self.launchpad.settings = None;
                            self.lp_render("all");
                        } else if !self.launchpad.query.is_empty() {
                            self.launchpad.query.clear();
                            self.lp_query_changed();
                        } else {
                            self.close_launchpad();
                        }
                    } else if vk == VK_RETURN.0 {
                        if let Some(a) = self.launchpad.selected_act() {
                            self.lp_act(a);
                        }
                    } else if self.launchpad.key_down(vk) {
                        self.lp_query_changed();
                    } else if vk == VK_DOWN.0 || vk == VK_UP.0 {
                        self.lp_render("all");
                    }
                }
                WM_ACTIVATE => {
                    // Clicking another window closes Launchpad (but not our own power menu).
                    if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE && !self.menu.open && !self.synthetic() {
                        self.close_launchpad();
                    }
                }
                _ => return None,
            }
            return Some(LRESULT(0));
        }
        if hwnd == self.stack_view.hwnd() {
            let (x, y) = lparam_xy(lp);
            let c = self.compositor.clone();
            match msg {
                WM_MOUSEMOVE => self.stack_view.on_move(&c, x, y),
                WM_LBUTTONDOWN | WM_RBUTTONDOWN if self.stack_view.outside(x, y) => {
                    // Clicked the invisible shadow margin: that's clicking away. Save a
                    // rename in progress and close, as for a click anywhere else.
                    let ev = self.stack_view.commit_rename(&c, &self.gfx);
                    self.stack_view.hide();
                    self.stack_event(ev);
                }
                WM_LBUTTONDOWN => {
                    let ev = self.stack_view.on_down(&c, &self.gfx, x, y);
                    unsafe {
                        SetCapture(hwnd); // keep getting the pointer while dragging an app
                    }
                    self.stack_event(ev);
                }
                WM_LBUTTONUP => {
                    let ev = self.stack_view.on_up(&c, x, y);
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    self.stack_event(ev);
                }
                WM_CAPTURECHANGED => self.stack_view.cancel_drag(&c),
                WM_CHAR => {
                    let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
                    self.stack_view.key_char(&c, &self.gfx, ch);
                }
                WM_KEYDOWN => {
                    let vk = wp.0 as u16;
                    let (handled, ev) = self.stack_view.key_down(&c, &self.gfx, vk);
                    if !handled && vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0 {
                        self.stack_view.hide();
                    }
                    self.stack_event(ev);
                }
                WM_ACTIVATE => {
                    if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE {
                        // Clicking away saves a rename in progress, like Finder.
                        let ev = self.stack_view.commit_rename(&c, &self.gfx);
                        self.stack_view.hide();
                        self.stack_event(ev);
                    }
                }
                _ => return None,
            }
            return Some(LRESULT(0));
        }
        if hwnd == self.preview.hwnd() {
            let (x, y) = lparam_xy(lp);
            match msg {
                WM_MOUSEACTIVATE => return Some(LRESULT(MA_NOACTIVATE as isize)),
                WM_MOUSEMOVE => {
                    unsafe {
                        let _ = KillTimer(Some(self.hwnd), T_PREVIEW_HIDE);
                        let _ = KillTimer(Some(self.hwnd), T_HIDE);
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                    self.preview.on_move(&self.compositor, x, y);
                }
                WM_MOUSELEAVE => unsafe {
                    SetTimer(Some(self.hwnd), T_PREVIEW_HIDE, 300, None);
                },
                WM_LBUTTONUP => match self.preview.on_click(x, y) {
                    Some(PreviewHit::Card(h)) => {
                        self.hide_preview();
                        tracker::activate(h);
                    }
                    Some(PreviewHit::Close(h)) => {
                        self.hide_preview();
                        tracker::close(h);
                    }
                    None => {}
                },
                _ => return None,
            }
            return Some(LRESULT(0));
        }
        if hwnd == self.menu.hwnd() {
            let (x, y) = lparam_xy(lp);
            match msg {
                WM_MOUSEMOVE => self.menu.on_move(&self.compositor, x, y),
                WM_LBUTTONUP | WM_RBUTTONUP => {
                    if let Some(cmd) = self.menu.on_click(x, y) {
                        self.menu_choose(Some(cmd));
                    }
                }
                WM_KEYDOWN => match self.menu.on_key(&self.compositor, wp.0 as u16) {
                    Some(cmd) => self.menu_choose(cmd),
                    None => {}
                },
                WM_ACTIVATE => {
                    if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE && self.menu.open {
                        // Clicked elsewhere: close without stealing focus back.
                        self.menu.hide(false);
                        self.menu_closed();
                    }
                }
                _ => return None,
            }
            return Some(LRESULT(0));
        }
        None
    }



    // ---------------------------------------------------------------- stacks

    fn open_stack(&mut self, i: usize) {
        let it = self.items[i].clone();
        let Some((pin, name, ids)) = self.stack_of(&it.key) else { return };
        self.hide_preview();
        let _ = self.show_label(None);
        let names: Vec<String> = ids.iter().map(|a| self.names.get(a).cloned().unwrap_or_else(|| a.clone())).collect();
        let brushes: Vec<Option<CompositionSurfaceBrush>> = ids.iter().map(|a| self.brushes.get(a).cloned()).collect();
        let list: Vec<StackApp> = ids
            .iter()
            .zip(names.iter())
            .zip(brushes.iter())
            .map(|((id, name), b)| StackApp { id, name, brush: b.as_ref() })
            .collect();
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let win_top = self.monitor.bottom - self.geo.win_h as i32;
        let (cx, sc) = self.geo.slot_center((p.x - self.monitor.left) as f32, if self.hovering { 1.0 } else { 0.0 }, i);
        let icon_top = self.geo.y0 + self.geo.p + self.geo.b - self.geo.b * sc;
        let bottom = win_top + (icon_top - 8.0 * self.scale) as i32;
        let (c, gfx, bd) = (self.compositor.clone(), &self.gfx, self.backdrop.clone());
        if let Err(e) = self.stack_view.show(&c, gfx, &bd, pin, &name, &list, self.monitor.left + cx as i32, bottom, self.scale, self.monitor) {
            trace!("stack failed: {e:?}");
        }
    }

    /// Act on what happened in the stack pop-up: open an app, save a new order or name.
    fn stack_event(&mut self, ev: Option<crate::stack::StackEvent>) {
        use crate::stack::StackEvent;
        let pin = self.stack_view.pin;
        match ev {
            None => {}
            Some(StackEvent::Open(id)) => {
                self.stack_view.hide();
                self.open_app(&id);
            }
            Some(StackEvent::Reorder(order)) => {
                trace!("stack {pin} reordered: {order:?}");
                self.save_and_apply(|c| {
                    if let Some(Pin::Stack { apps, .. }) = c.pins.get_mut(pin) {
                        // Same apps, new order (never drop or add any here).
                        if apps.len() == order.len() && order.iter().all(|a| apps.contains(a)) {
                            *apps = order;
                        }
                    }
                });
            }
            Some(StackEvent::Rename(name)) => {
                trace!("stack {pin} renamed: {name}");
                self.save_and_apply(|c| {
                    if let Some(Pin::Stack { stack, .. }) = c.pins.get_mut(pin) {
                        *stack = name;
                    }
                });
            }
        }
    }

    /// Open an app from a stack (or anywhere): bring it forward if running, else launch.
    fn open_app(&mut self, id: &str) {
        let wins = self.item_windows(&Item { kind: Kind::App, key: id.to_string(), pinned: true });
        match wins.first() {
            Some(w) => tracker::activate(w.hwnd),
            None => apps::launch(id),
        }
    }


    // ---------------------------------------------------------------- Launchpad

    /// Apps listed under "Pinned" in Launchpad.
    fn lp_pinned(&self) -> Vec<String> {
        if !self.cfg.launchpad_pins.is_empty() {
            return self.cfg.launchpad_pins.clone();
        }
        let mut v: Vec<String> = Vec::new();
        for p in &self.cfg.pins {
            match p {
                Pin::Id(id) if id != SEPARATOR => v.push(id.clone()),
                Pin::Stack { apps, .. } => v.extend(apps.iter().cloned()),
                _ => {}
            }
        }
        v.dedup();
        v.truncate(24);
        v
    }

    /// Make sure icon brushes exist for these app ids, using only the on-disk cache;
    /// anything not cached yet is fetched by the worker (tiles show placeholders meanwhile).
    fn lp_brushes(&mut self, ids: &[String]) {
        let px = (self.cfg.magnified_size.max(self.cfg.icon_size) * self.scale).ceil();
        let mut missing = Vec::new();
        for id in ids {
            if self.brushes.contains_key(id) {
                continue;
            }
            match self.icons.get_cached(&self.gfx.wic, id) {
                Some(src) => {
                    if let Ok(surface) = self.gfx.icon_surface(&src, px) {
                        if let Ok(b) = self.compositor.CreateSurfaceBrushWithSurface(&surface) {
                            let _ = b.SetStretch(CompositionStretch::Fill);
                            self.brushes.insert(id.clone(), b);
                        }
                    }
                }
                None => {
                    if self.icons_requested.insert(id.clone()) {
                        missing.push(id.clone());
                    }
                }
            }
        }
        if !missing.is_empty() {
            self.services.send(Req::Icons(self.icons.dir(), missing));
        }
    }

    fn lp_render(&mut self, which: &str) {
        if !self.launchpad.open {
            return;
        }
        let pinned = self.lp_pinned();
        let mut need = pinned.clone();
        if self.launchpad.all_open && self.launchpad.query.is_empty() {
            need.extend(self.lp_apps.iter().map(|a| a.id.clone()));
        }
        self.launchpad_results_ids(&mut need);
        self.lp_brushes(&need);
        for a in &self.lp_apps {
            self.names.entry(a.id.clone()).or_insert_with(|| a.name.clone());
        }
        let tray = self.tray_views();
        let data = lp::Data {
            apps: &self.lp_apps,
            brushes: &self.brushes,
            pinned: &pinned,
            names: &self.names,
            control: &self.control,
            stats: self.stats.as_ref(),
            weather: self.weather.as_ref(),
            files: &self.files,
            cfg: &self.cfg,
            user: &self.user_name,
            wifi: self.wifi.as_ref(),
            tray: &tray,
            update: self.update.as_ref(),
        };
        self.launchpad.update_results(&data);
        let (c, gfx) = (self.compositor.clone(), &self.gfx);
        let r = match which {
            "all" => self.launchpad.render(&c, gfx, &data),
            "right" => self.launchpad.render_right_panel(&c, gfx, &data),
            _ => Ok(()),
        };
        if let Err(e) = r {
            trace!("launchpad render failed: {e:?}");
        }
    }

    /// Click a tray icon: Launchpad closes first so the app's window or menu can come up
    /// in front, at the pointer.
    fn tray_click(&mut self, key: u64, kind: crate::tray::Click) {
        self.close_launchpad();
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        crate::tray::click(key, kind, p);
    }

    /// An app's tray icon as a brush. WIC gives icons straight (not premultiplied) alpha,
    /// which Direct2D can't draw directly, so convert first.
    fn tray_brush(&self, hicon: HICON, px: f32) -> Result<CompositionSurfaceBrush> {
        use windows::Win32::Graphics::Imaging::*;
        let wic = &self.gfx.wic;
        let bmp = unsafe { wic.CreateBitmapFromHICON(hicon)? };
        let conv = unsafe { wic.CreateFormatConverter()? };
        unsafe { conv.Initialize(&bmp, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)? };
        let src: IWICBitmapSource = conv.cast()?;
        let surface = self.gfx.icon_surface(&src, px)?;
        let b = self.compositor.CreateSurfaceBrushWithSurface(&surface)?;
        b.SetStretch(CompositionStretch::Fill)?;
        Ok(b)
    }

    /// Visible tray icons with their pictures, redrawn only when an icon changed.
    fn tray_views(&mut self) -> Vec<lp::TrayView> {
        let icons: Vec<(u64, u64, HICON, String)> = crate::tray::with_icons(|icons| {
            icons.iter().filter(|i| !i.hidden).filter_map(|i| i.hicon.map(|h| (i.key, i.gen, h, i.tip.clone()))).collect()
        });
        self.tray_brushes.retain(|k, _| icons.iter().any(|i| i.0 == *k));
        let px = (24.0 * self.scale).ceil();
        let mut out = Vec::new();
        for (key, gen, hicon, tip) in icons {
            let fresh = self.tray_brushes.get(&key).map(|(g, _)| *g == gen).unwrap_or(false);
            if !fresh {
                match self.tray_brush(hicon, px) {
                    Ok(b) => {
                        self.tray_brushes.insert(key, (gen, b));
                    }
                    Err(e) => {
                        trace!("tray icon {key} ('{tip}') couldn't be drawn: {e:?}");
                        // Don't retry this picture on every redraw; wait for the app to change it.
                        if let Ok(b) = self.compositor.CreateSurfaceBrush() {
                            self.tray_brushes.insert(key, (gen, b));
                        }
                    }
                }
            }
            out.push(lp::TrayView { key, brush: self.tray_brushes.get(&key).map(|(_, b)| b.clone()), tip });
        }
        out
    }

    fn launchpad_results_ids(&self, need: &mut Vec<String>) {
        if self.launchpad.query.is_empty() {
            return;
        }
        for (a, _) in laminar_sys::search::rank(&self.launchpad.query, &self.lp_apps, 8) {
            need.push(a.id.clone());
        }
    }

    fn toggle_launchpad(&mut self) {
        if self.launchpad.open {
            self.close_launchpad();
        } else {
            self.open_launchpad();
        }
    }

    fn open_launchpad(&mut self) {
        self.hide_preview();
        self.stack_view.hide();
        let _ = self.show_label(None);
        if self.tucked {
            self.set_tucked(false);
        }
        self.services.send(Req::Control);
        self.services.send(Req::Stats);
        // Windows Update status: at most every 20 minutes (each check takes seconds).
        if self.update_asked.map(|t| t.elapsed().as_secs() > 20 * 60).unwrap_or(true) {
            self.update_asked = Some(Instant::now());
            self.services.send(Req::Update);
        }
        if self.lp_apps.is_empty() {
            self.services.send(Req::Apps);
        }
        let pinned = self.lp_pinned();
        self.lp_brushes(&pinned);
        for a in &self.lp_apps {
            self.names.entry(a.id.clone()).or_insert_with(|| a.name.clone());
        }
        let reserve = self.geo.shelf_h + 2.0 * self.geo.margin;
        let tray = self.tray_views();
        let data = lp::Data {
            apps: &self.lp_apps,
            brushes: &self.brushes,
            pinned: &pinned,
            names: &self.names,
            control: &self.control,
            stats: self.stats.as_ref(),
            weather: self.weather.as_ref(),
            files: &self.files,
            cfg: &self.cfg,
            user: &self.user_name,
            wifi: self.wifi.as_ref(),
            tray: &tray,
            update: self.update.as_ref(),
        };
        let (c, gfx, bd) = (self.compositor.clone(), &self.gfx, self.backdrop.clone());
        if let Err(e) = self.launchpad.show(&c, gfx, &bd, self.monitor, reserve, self.scale, &data) {
            trace!("launchpad failed: {e:?}");
            return;
        }
        trace!(
            "launchpad dpi={} dock dpi={} system dpi={} monitor={}x{}",
            unsafe { GetDpiForWindow(self.launchpad.hwnd()) },
            unsafe { GetDpiForWindow(self.hwnd) },
            unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() },
            self.monitor.right - self.monitor.left,
            self.monitor.bottom - self.monitor.top
        );
        // Keep the dock above the Launchpad.
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetTimer(Some(self.hwnd), T_LP_TICK, 1000, None);
        }
        self.ind_state.iter_mut().for_each(|s| *s = IndState::default());
        self.update_indicators();
    }

    fn close_launchpad(&mut self) {
        if !self.launchpad.open {
            return;
        }
        self.launchpad.hide();
        self.launchpad.wifi_open = false;
        self.lp_drag = None;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_LP_TICK);
            let _ = KillTimer(Some(self.hwnd), T_FILES);
            if self.cfg.auto_hide && !self.hovering {
                SetTimer(Some(self.hwnd), T_HIDE, self.cfg.hide_delay_ms.max(1), None);
            }
            // Re-check hover once Launchpad is gone, in case a leave notice was lost while it
            // was open (menus, settings, drags); otherwise the dock can stay up for good.
            let _ = PostMessageW(Some(self.hwnd), WM_APP_RESYNC_HOVER, WPARAM(0), LPARAM(0));
        }
    }

    fn lp_query_changed(&mut self) {
        self.files.clear();
        self.lp_render("all");
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_FILES);
            if !self.launchpad.query.trim().is_empty() {
                SetTimer(Some(self.hwnd), T_FILES, 180, None);
            }
        }
    }

    fn lp_slider(&mut self, act: &Act, x: f32, commit: bool) {
        let Some(frac) = self.launchpad.slider_value(act, x) else { return };
        match act {
            Act::Volume => {
                self.control.volume = Some(frac);
                self.services.send(Req::SetVolume(frac));
                self.lp_render("all");
            }
            Act::Slider(key) => {
                let Some((min, max, step)) = Launchpad::slider_range(key) else { return };
                let v = ((min + frac * (max - min)) / step).round() * step;
                let mut cfg = self.cfg.clone();
                lp::cfg_set(&mut cfg, key, v);
                if commit {
                    cfg.save();
                }
                if cfg != self.cfg {
                    self.apply_config(cfg);
                }
                self.lp_render("all");
            }
            _ => {}
        }
    }

    fn lp_act(&mut self, act: Act) {
        trace!("launchpad action {act:?}");
        match act {
            Act::Close => self.close_launchpad(),
            Act::Tray(key) => self.tray_click(key, crate::tray::Click::Left),
            Act::App(id) => {
                self.close_launchpad();
                self.open_app(&id);
            }
            Act::File(path) => {
                self.close_launchpad();
                apps::launch(&path);
            }
            Act::Uri(u) => {
                self.close_launchpad();
                apps::launch(&u);
            }
            Act::Web(q) => {
                self.close_launchpad();
                let enc: String = q
                    .bytes()
                    .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
                    .collect();
                apps::launch(&format!("https://www.bing.com/search?q={enc}"));
            }
            Act::ToggleAll => {
                self.launchpad.all_open = !self.launchpad.all_open;
                self.lp_render("all");
            }
            Act::OpenSettings => {
                self.launchpad.settings = Some(0);
                self.lp_render("all");
            }
            Act::CloseSettings => {
                self.launchpad.settings = None;
                self.lp_render("all");
            }
            Act::Section(k) => {
                self.launchpad.query.clear();
                self.launchpad.settings = Some(k);
                self.lp_render("all");
            }
            Act::Toggle(key) => {
                let v = lp::cfg_get(&self.cfg, key);
                self.save_and_apply(|c| lp::cfg_set(c, key, if v > 0.5 { 0.0 } else { 1.0 }));
                self.lp_render("all");
            }
            Act::Power => {
                let items = vec![
                    MenuItem::item("Lock", CMD_POWER_LOCK),
                    MenuItem::item("Sign out", CMD_POWER_SIGNOUT),
                    MenuItem::sep(),
                    MenuItem::item("Sleep", CMD_POWER_SLEEP),
                    MenuItem::item("Restart", CMD_POWER_RESTART),
                    MenuItem::item("Shut down", CMD_POWER_SHUTDOWN),
                ];
                let mut p = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut p);
                }
                self.menu_ctx = None;
                self.menu_open = true;
                let _ = self.menu.show(&self.compositor, &self.gfx, &self.backdrop, items, p, true, self.scale, self.monitor);
            }
            Act::Wifi => {
                let on = !self.control.wifi.unwrap_or(false);
                self.control.wifi = Some(on);
                self.services.send(Req::SetWifi(on));
                if on && self.launchpad.wifi_open {
                    self.wifi = None;
                    self.services.send(Req::WifiList);
                }
                self.lp_render("all");
            }
            Act::WifiMenu => {
                self.launchpad.wifi_open = true;
                self.services.send(Req::WifiList);
                self.lp_render("all");
            }
            Act::WifiBack => {
                self.launchpad.wifi_open = false;
                self.lp_render("all");
            }
            Act::WifiJoin(ssid) => {
                trace!("wifi join {ssid}");
                self.services.send(Req::WifiConnect(ssid));
            }
            Act::Bluetooth => {
                let on = !self.control.bluetooth.unwrap_or(false);
                self.control.bluetooth = Some(on);
                self.services.send(Req::SetBluetooth(on));
                self.lp_render("all");
            }
            Act::Dark => {
                let on = !self.control.dark;
                self.control.dark = on;
                self.services.send(Req::SetDark(on));
                self.lp_render("all");
            }
            Act::Mute => {
                let m = !self.control.muted.unwrap_or(false);
                self.control.muted = Some(m);
                self.services.send(Req::SetMuted(m));
                self.lp_render("all");
            }
            Act::MediaPrev => self.services.send(Req::Media(services::Media::Previous)),
            Act::MediaPlay => self.services.send(Req::Media(services::Media::PlayPause)),
            Act::MediaNext => self.services.send(Req::Media(services::Media::Next)),
            Act::OpenConfig => {
                self.close_launchpad();
                apps::edit_file(&config::path());
            }
            Act::WinSettings => {
                self.close_launchpad();
                apps::launch("ms-settings:");
            }
            Act::Snip => {
                self.close_launchpad();
                apps::launch("ms-screenclip:");
            }
            Act::Lock => {
                self.close_launchpad();
                self.services.send(Req::Power(services::Power::Lock));
            }
            Act::Volume | Act::Slider(_) | Act::Search => {}
        }
    }

    fn on_service_replies(&mut self) {
        let mut render = false;
        for r in self.services.take() {
            match r {
                Reply::Apps(list) => {
                    trace!("apps: {}", list.len());
                    self.lp_apps = list;
                    render = true;
                }
                Reply::Files(gen, hits) => {
                    if gen == self.files_gen {
                        self.files = hits;
                        render = true;
                    }
                }
                Reply::Control(c) => {
                    let sample = self.opts.demo.as_deref() == Some("lp-wifi-sample");
                    self.control = c;
                    if sample {
                        self.control.wifi = Some(true);
                    }
                    render = true;
                }
                Reply::Wifi(_) if self.opts.demo.as_deref() == Some("lp-wifi-sample") => {}
                Reply::Wifi(w) => {
                    trace!("wifi: {:?}", w.as_ref().map(|v| v.len()));
                    self.wifi = Some(w);
                    render = true;
                }
                Reply::Update(u) => {
                    trace!("windows update: {u:?}");
                    self.update = Some(u);
                    render = true;
                }
                Reply::Stats(st) => {
                    self.stats = Some(st);
                    render = true;
                }
                Reply::Weather(w) => {
                    if w.is_some() {
                        self.weather = w;
                    }
                    render = true;
                }
                Reply::Icons(list) => {
                    for (id, name) in list {
                        self.icons.set_name(&id, &name);
                        self.names.entry(id).or_insert(name);
                    }
                    self.icons.flush();
                    render = true;
                }
            }
        }
        if render && self.launchpad.open && self.lp_drag.is_none() {
            self.lp_render("all");
        }
    }


    // ---------------------------------------------------------------- files dropped on the dock

    fn set_drop_highlight(&mut self, t: Option<usize>) {
        if t == self.drop_target {
            return;
        }
        let b = self.geo.b;
        for (idx, scale) in [(self.drop_target, 1.0f32), (t, 1.15f32)] {
            if let Some(Some(tile)) = idx.and_then(|i| self.tiles.get(i)) {
                let _ = tile.SetCenterPoint(v3(b / 2.0, b / 2.0, 0.0));
                if let Ok(o) = tile.cast::<CompositionObject>() {
                    let _ = crate::ui::animate_v3(&self.compositor, &o, "Scale", v3(scale, scale, 1.0), 150);
                }
            }
        }
        self.drop_target = t;
    }

    fn on_drop_event(&mut self, ev: crate::filedrop::Event) -> crate::filedrop::Target {
        use crate::filedrop::{Event, Target};
        match ev {
            Event::Enter(sx, sy, files, alt) => {
                trace!("files dragged over the dock at ({sx}, {sy}) alt={alt}: {files:?}");
                let pinnable = !files.is_empty() && files.iter().all(|f| crate::filedrop::is_pinnable(f));
                self.file_drag = Some(FileDrag { files, pinnable, alt, insert: None });
                self.file_drag_over(sx, sy)
            }
            Event::Over(sx, sy, alt) => {
                if let Some(fd) = self.file_drag.as_mut() {
                    fd.alt = alt;
                }
                self.file_drag_over(sx, sy)
            }
            Event::Leave => {
                self.file_drag_end();
                self.on_leave();
                Target::None
            }
            Event::Drop(sx, sy, files) => {
                let t = self.file_drag_over(sx, sy);
                let insert = self.file_drag.as_ref().and_then(|f| f.insert);
                let on_item = self.drop_target;
                self.file_drag_end();
                self.hide_preview();
                trace!("files dropped at ({sx}, {sy}): {:?} insert={insert:?} item={on_item:?} {} files", t as u8, files.len());
                let result = match (t, on_item) {
                    (Target::Pin, _) if insert.is_some() => {
                        self.pin_files_at(insert.unwrap(), &files);
                        Target::Pin
                    }
                    (Target::App, Some(i)) => {
                        let it = self.items[i].clone();
                        // Packaged apps (ids with '!') are activated for the files; classic apps get paths.
                        let exe = if apps::is_path(&it.key) {
                            Some(it.key.clone())
                        } else if it.key.contains('!') {
                            None
                        } else {
                            self.pin_path(&it.key).or_else(|| self.windows.iter().find(|w| w.app == it.key).map(|w| w.exe.clone()))
                        };
                        let _ = self.bounce(i);
                        crate::filedrop::open_with(&it.key, exe, &files);
                        Target::App
                    }
                    (Target::Trash, _) => {
                        crate::filedrop::recycle(&files);
                        Target::Trash
                    }
                    _ => Target::None,
                };
                // The pointer is still over the dock: resume normal hover tracking.
                let win_top = self.monitor.bottom - self.geo.win_h as i32;
                self.hovering = false;
                self.on_move((sx - self.monitor.left) as f32, (sy - win_top) as f32);
                result
            }
        }
    }

    /// Files dragged over the dock: decide pin / open with / recycle, opening a gap to pin into.
    /// Programs, shortcuts and folders pin (icons part to make room). Documents open with the
    /// app they're dropped on, unless Alt is held, which pins them instead. Keeping the two
    /// modes apart means icons never shift under a document that's heading for an app.
    fn file_drag_over(&mut self, sx: i32, sy: i32) -> crate::filedrop::Target {
        use crate::filedrop::Target;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_HIDE);
            let _ = KillTimer(Some(self.hwnd), T_SHRINK);
        }
        if self.tucked {
            self.set_tucked(false);
        }
        if !self.hovering {
            // OLE runs its own mouse loop, so set up hover state here (no magnification while
            // dropping, so the icons hold still).
            self.hovering = true;
            // Tiles a drag opened stay up when it comes back to the dock.
            if !(self.preview.open && self.drag_preview) {
                self.hide_preview();
            }
            let _ = self.show_label(None);
            self.apply_region();
        }
        let _ = self.animate_amount(0.0, 120);
        let win_top = self.monitor.bottom - self.geo.win_h as i32;
        let (x, y) = ((sx - self.monitor.left) as f32, (sy - win_top) as f32);
        let Some(fd) = &self.file_drag else { return Target::None };
        let old = fd.insert;
        let w = self.geo.b + self.geo.g;
        let (target, insert, on_item) = {
            // Item centres as currently drawn (an open gap shifts them by half its width each way).
            let vc = |j: usize| {
                self.geo.center_abs(j)
                    + match old {
                        Some(k) if j >= k => w / 2.0,
                        Some(_) => -w / 2.0,
                        None => 0.0,
                    }
            };
            let n = self.items.len();
            let in_band = y > self.geo.y0 - 30.0 * self.scale;
            let mut out = (Target::None, None, None);
            let pin_mode = fd.pinnable || fd.alt;
            if in_band && !fd.files.is_empty() && n > 0 {
                let item = (0..n).find(|&j| (x - vc(j)).abs() < self.geo.width[j] / 2.0);
                match item {
                    Some(j) if self.items[j].kind == Kind::Trash => out = (Target::Trash, None, Some(j)),
                    Some(j) if !pin_mode && self.items[j].kind == Kind::App => out = (Target::App, None, Some(j)),
                    _ if pin_mode && x > vc(0) - w && x < vc(n - 1) + w => {
                        let boundary = self.pinned_boundary();
                        let k = (1..boundary).find(|&j| vc(j) > x).unwrap_or(boundary);
                        out = (Target::Pin, Some(k), None);
                    }
                    _ => {}
                }
            }
            out
        };
        if insert != old {
            if let Some(o) = old {
                self.set_slot_value(o, "Gap", 0.0);
            }
            if let Some(k) = insert {
                self.set_slot_value(k, "Gap", w);
            }
            if let Ok(o) = self.ptr.cast::<CompositionObject>() {
                let _ = crate::ui::animate_scalar(&self.compositor, &o, "Delta", if insert.is_some() { w } else { 0.0 }, (220.0 * slowmo()) as i64);
            }
            if let Some(fd) = self.file_drag.as_mut() {
                fd.insert = insert;
            }
            trace!("file drag: insert={insert:?}");
        }
        self.set_drop_highlight(on_item);
        let under = {
            let shift = |j: usize| match insert {
                Some(k) if j >= k => w / 2.0,
                Some(_) => -w / 2.0,
                None => 0.0,
            };
            let in_band = y > self.geo.y0 - 30.0 * self.scale;
            (0..self.items.len()).find(|&j| in_band && (x - self.geo.center_abs(j) - shift(j)).abs() < self.geo.width[j] / 2.0)
        };
        self.drag_hover(under);
        target
    }

    /// A drag moved over item `idx` (or off the icons). Resting on a running app opens its
    /// window tiles, like hovering does, so the drag can go on to a window.
    fn drag_hover(&mut self, idx: Option<usize>) {
        let idx = idx.filter(|&i| {
            let it = self.items[i].clone();
            it.kind == Kind::App && !self.item_windows(&it).is_empty()
        });
        if idx == self.drag_under {
            return;
        }
        self.drag_under = idx;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_DRAG_PREVIEW);
            match idx {
                Some(i) if self.preview.open && self.drag_preview => {
                    if self.preview.key != self.items[i].key {
                        self.show_preview(i);
                    }
                }
                Some(_) => {
                    SetTimer(Some(self.hwnd), T_DRAG_PREVIEW, 600, None);
                }
                None => {}
            }
        }
    }

    /// A drag over the window tiles: highlight the tile under it and, after a pause, bring
    /// that window forward (and close the tiles) so the drag can be dropped into it.
    fn on_preview_drag(&mut self, ev: crate::filedrop::Event) {
        use crate::filedrop::Event;
        let local = |hwnd: HWND, sx: i32, sy: i32| {
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(hwnd, &mut r);
            }
            ((sx - r.left) as f32, (sy - r.top) as f32)
        };
        let (sx, sy, drop) = match ev {
            Event::Enter(x, y, _, _) | Event::Over(x, y, _) => (x, y, false),
            Event::Drop(x, y, _) => (x, y, true),
            Event::Leave => {
                self.drag_switch = None;
                self.preview.on_move(&self.compositor, -1.0, -1.0);
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), T_DRAG_SWITCH);
                    SetTimer(Some(self.hwnd), T_PREVIEW_HIDE, 300, None);
                }
                return;
            }
        };
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_PREVIEW_HIDE);
            let _ = KillTimer(Some(self.hwnd), T_HIDE);
        }
        let (x, y) = local(self.preview.hwnd(), sx, sy);
        self.preview.on_move(&self.compositor, x, y);
        let card = self.preview.window_at(x, y);
        if drop {
            // Nothing can be dropped on a tile; take it as "show me this window".
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_DRAG_SWITCH);
            }
            self.drag_switch = None;
            if let Some(h) = card {
                self.hide_preview();
                tracker::activate_during_drag(h);
                if self.cfg.auto_hide && !self.hovering {
                    unsafe {
                        SetTimer(Some(self.hwnd), T_HIDE, self.cfg.hide_delay_ms.max(1), None);
                    }
                }
            }
            return;
        }
        if card != self.drag_switch {
            self.drag_switch = card;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_DRAG_SWITCH);
                if card.is_some() {
                    SetTimer(Some(self.hwnd), T_DRAG_SWITCH, 500, None);
                }
            }
        }
    }

    /// Close any gap opened for a file drag and forget the drag.
    fn file_drag_end(&mut self) {
        self.drag_under = None;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_DRAG_PREVIEW);
        }
        if let Some(fd) = self.file_drag.take() {
            if let Some(k) = fd.insert {
                self.set_slot_value(k, "Gap", 0.0);
                if let Ok(o) = self.ptr.cast::<CompositionObject>() {
                    let _ = crate::ui::animate_scalar(&self.compositor, &o, "Delta", 0.0, (220.0 * slowmo()) as i64);
                }
            }
        }
        self.set_drop_highlight(None);
    }

    /// Pin dropped files (programs, shortcuts, folders, documents) before slot `k`.
    fn pin_files_at(&mut self, k: usize, files: &[String]) {
        let now = self.pinned_order();
        let pos = now.iter().filter(|(j, _)| *j < k).count();
        let mut order: Vec<Pin> = now.into_iter().map(|(_, p)| p).collect();
        let mut added = 0;
        for f in files {
            let f = &crate::apps::stable_path(f);
            let already = self.cfg.pins.iter().any(|p| match p {
                Pin::Id(a) => a.eq_ignore_ascii_case(f),
                Pin::Stack { apps, .. } => apps.iter().any(|a| a.eq_ignore_ascii_case(f)),
            });
            if already {
                continue;
            }
            order.insert((pos + added).min(order.len()), Pin::Id(f.clone()));
            added += 1;
        }
        trace!("pinned {added} dropped files at slot {k}");
        if added > 0 {
            self.apply_pin_order(order);
        }
    }

    // ---------------------------------------------------------------- previews

    /// The pointer moved onto (or off) a dock item: schedule, switch or close previews.
    fn preview_hover(&mut self, idx: Option<usize>) {
        if self.drag.is_some() || self.menu_open {
            return;
        }
        let target = idx.filter(|&i| {
            let it = self.items[i].clone();
            it.kind == Kind::App && !self.item_windows(&it).is_empty()
        });
        unsafe {
            match target {
                Some(i) => {
                    let _ = KillTimer(Some(self.hwnd), T_PREVIEW_HIDE);
                    if self.preview.open {
                        if self.preview.key != self.items[i].key {
                            self.show_preview(i);
                        }
                    } else {
                        self.preview_pending = Some(i);
                        SetTimer(Some(self.hwnd), T_PREVIEW, 450, None);
                    }
                }
                None => {
                    let _ = KillTimer(Some(self.hwnd), T_PREVIEW);
                    self.preview_pending = None;
                    if self.preview.open {
                        SetTimer(Some(self.hwnd), T_PREVIEW_HIDE, 300, None);
                    }
                }
            }
        }
    }

    fn show_preview(&mut self, i: usize) {
        let it = self.items[i].clone();
        let wins = self.item_windows(&it);
        if wins.is_empty() {
            return;
        }
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let win_top = self.monitor.bottom - self.geo.win_h as i32;
        let local_x = (p.x - self.monitor.left) as f32;
        // Drags don't magnify the icons.
        let amount = if self.hovering && self.file_drag.is_none() { 1.0 } else { 0.0 };
        let (cx, sc) = self.geo.slot_center(local_x, amount, i);
        self.drag_preview = self.file_drag.is_some();
        let icon_top = self.geo.y0 + self.geo.p + self.geo.b - self.geo.b * sc;
        let bottom = win_top + (icon_top - 8.0 * self.scale) as i32;
        let brush = self.brushes.get(&it.key).cloned();
        let _ = self.show_label(None);
        if let Err(e) = self.preview.show(
            &self.compositor,
            &self.gfx,
            &self.backdrop,
            &it.key,
            &wins,
            brush.as_ref(),
            self.monitor.left + cx as i32,
            bottom,
            self.scale,
            self.monitor,
        ) {
            trace!("preview failed: {e:?}");
        }
    }

    fn hide_preview(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_PREVIEW);
            let _ = KillTimer(Some(self.hwnd), T_PREVIEW_HIDE);
        }
        self.preview_pending = None;
        self.preview.hide();
    }

    // ---------------------------------------------------------------- drag & drop

    /// Items 1..boundary are the pinned ones (after the launch button).
    fn pinned_boundary(&self) -> usize {
        let mut b = 1;
        while b < self.items.len() && self.items[b].pinned && matches!(self.items[b].kind, Kind::App | Kind::Separator | Kind::Stack) {
            b += 1;
        }
        b
    }

    fn draggable(&self, i: usize) -> bool {
        let it = &self.items[i];
        it.kind == Kind::App || it.kind == Kind::Stack || (it.kind == Kind::Separator && it.pinned)
    }

    fn set_slot_value(&self, slot: usize, prop: &str, v: f32) {
        if let Some(acc) = self.accs.get(slot) {
            if let Ok(o) = acc.cast::<CompositionObject>() {
                let _ = crate::ui::animate_scalar(&self.compositor, &o, prop, v, (220.0 * slowmo()) as i64);
            }
        }
    }

    fn set_delta(&self) {
        let Some(d) = &self.drag else { return };
        let gap = if d.insert.is_some() { d.width } else { 0.0 };
        let collapse = if d.ext.is_none() && (d.insert.is_some() || d.removing) { d.width } else { 0.0 };
        if let Ok(o) = self.ptr.cast::<CompositionObject>() {
            let _ = crate::ui::animate_scalar(&self.compositor, &o, "Delta", gap - collapse, (220.0 * slowmo()) as i64);
        }
    }

    fn start_drag(&mut self, i: usize) {
        self.hide_preview();
        self.stack_view.hide();
        trace!("drag start: item {i} {}", self.items[i].key);
        let width = self.geo.width[i];
        self.drag = Some(Drag { index: i, insert: None, removing: false, width, merge: None, merge_ready: false, ext: None });
        let _ = self.animate_amount(0.0, 120);
        let _ = self.show_label(None);
        let _ = self.slots[i].SetOpacity(0.0);
        if let Some(Some(ind)) = self.inds.get(i) {
            let _ = ind.SetOpacity(0.0);
        }
        if let Some(Some(att)) = self.atts.get(i) {
            let _ = att.vis.SetOpacity(0.0);
        }
        // Show the ghost (a floating copy of the icon) under the pointer.
        if let Err(e) = self.show_ghost(i) {
            trace!("ghost failed: {e:?}");
        }
    }

    /// Begin dragging an app that came from Launchpad (pinning it where it's dropped).
    fn start_external_drag(&mut self, id: &str) {
        trace!("external drag start: {id}");
        self.lp_brushes(&[id.to_string()]);
        let width = self.geo.b + self.geo.g;
        // Show the ghost using a temporary item description.
        self.items.push(Item { kind: Kind::App, key: id.to_string(), pinned: false });
        let tmp = self.items.len() - 1;
        let r = self.show_ghost(tmp);
        self.items.pop();
        if let Err(e) = r {
            trace!("ghost failed: {e:?}");
        }
        self.drag = Some(Drag { index: usize::MAX, insert: None, removing: false, width, merge: None, merge_ready: false, ext: Some(id.to_string()) });
        let _ = self.animate_amount(0.0, 120);
        if self.tucked {
            self.set_tucked(false);
        }
        self.hovering = true;
        self.apply_region();
        unsafe {
            SetCapture(self.hwnd);
        }
    }

    fn show_ghost(&mut self, i: usize) -> Result<()> {
        let c = self.compositor.clone();
        if self.ghost.is_none() {
            let popup = crate::ui::Popup::new(&c, false)?;
            let body = c.CreateContainerVisual()?;
            popup.root.Children()?.InsertAtTop(&body)?;
            self.ghost = Some(Ghost { popup, body, size: 0 });
        }
        let it = self.items[i].clone();
        let s = self.scale;
        let icon = (self.geo.b * 1.12).round();
        let blur = (14.0 * s).round();
        let size = (icon + 2.0 * blur + 8.0 * s).ceil();
        let g = self.ghost.as_mut().unwrap();
        g.body.Children()?.RemoveAll()?;
        g.size = size as i32;
        g.body.SetSize(crate::ui::v2(size, size))?;
        g.body.SetOpacity(1.0)?;
        g.body.SetScale(crate::ui::v3(1.0, 1.0, 1.0))?;
        g.body.SetCenterPoint(crate::ui::v3(size / 2.0, size / 2.0, 0.0))?;
        if it.kind == Kind::Separator {
            let (line, _, _) = crate::ui::rounded(&c, (3.0 * s).round(), icon * 0.7, 1.5 * s, crate::ui::theme::ink(0.8))?;
            line.SetOffset(crate::ui::v3((size - 3.0 * s) / 2.0, (size - icon * 0.7) / 2.0, 0.0))?;
            g.body.Children()?.InsertAtTop(&line)?;
        } else if let Some(brush) = self.brushes.get(&it.key) {
            let sh = self.gfx.shadow_surface(icon * 0.22, blur, 0.45)?;
            let sb = c.CreateSurfaceBrushWithSurface(&sh)?;
            sb.SetStretch(CompositionStretch::Fill)?;
            let nine = c.CreateNineGridBrush()?;
            nine.SetSource(&sb)?;
            nine.SetInsets(icon * 0.22 + blur + 2.0)?;
            let shadow = c.CreateSpriteVisual()?;
            shadow.SetBrush(&nine)?;
            shadow.SetSize(crate::ui::v2(icon + 2.0 * blur, icon + 2.0 * blur))?;
            shadow.SetOffset(crate::ui::v3((size - icon) / 2.0 - blur, (size - icon) / 2.0 - blur + 6.0 * s, 0.0))?;
            g.body.Children()?.InsertAtTop(&shadow)?;
            let tile = c.CreateSpriteVisual()?;
            tile.SetBrush(brush)?;
            tile.SetSize(crate::ui::v2(icon, icon))?;
            tile.SetOffset(crate::ui::v3((size - icon) / 2.0, (size - icon) / 2.0, 0.0))?;
            g.body.Children()?.InsertAtTop(&tile)?;
        }
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let half = (size / 2.0) as i32;
        g.popup.show(RECT { left: p.x - half, top: p.y - half, right: p.x + half, bottom: p.y + half }, false);
        Ok(())
    }

    fn move_ghost(&self, sx: i32, sy: i32) {
        if let Some(g) = &self.ghost {
            let half = g.size / 2;
            unsafe {
                let _ = SetWindowPos(g.popup.hwnd, Some(HWND_TOPMOST), sx - half, sy - half, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    fn hide_ghost(&mut self, poof: bool) {
        let Some(g) = &self.ghost else { return };
        if poof {
            // Shrink and fade, then hide.
            if let Ok(o) = g.body.cast::<CompositionObject>() {
                let _ = crate::ui::animate_scalar(&self.compositor, &o, "Opacity", 0.0, 200);
                let _ = crate::ui::animate_v3(&self.compositor, &o, "Scale", crate::ui::v3(0.3, 0.3, 1.0), 220);
            }
            unsafe { SetTimer(Some(self.hwnd), T_GHOST, 240, None) };
        } else {
            g.popup.hide();
        }
    }

    /// Pointer moved while dragging (client coordinates of the dock window).
    fn drag_move(&mut self, x: f32, y: f32) {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        self.move_ghost(p.x, p.y);
        let Some(d) = &self.drag else { return };
        let i = d.index;
        let external = d.ext.is_some();
        let pinned = !external && self.items[i].pinned;
        let removing = pinned && y < self.geo.y0 - 90.0 * self.scale;
        let is_app = external || self.items[i].kind == Kind::App;
        // Holding an app over the middle of another pinned app or a stack makes/extends a stack.
        if is_app && !removing && y > self.geo.y0 - 20.0 * self.scale {
            let boundary = self.pinned_boundary();
            let over = (1..boundary).find(|&j| {
                j != i
                    && matches!(self.items[j].kind, Kind::App | Kind::Stack)
                    && (x - self.geo.center_abs(j)).abs() < self.geo.b * 0.28
            });
            let current = d.merge;
            if over != current {
                if let Some(t) = current {
                    if let Some(Some(tile)) = self.tiles.get(t) {
                        if let Ok(o) = tile.cast::<CompositionObject>() {
                            let _ = crate::ui::animate_v3(&self.compositor, &o, "Scale", crate::ui::v3(1.0, 1.0, 1.0), 150);
                        }
                    }
                }
                if let Some(dm) = self.drag.as_mut() {
                    dm.merge = over;
                    dm.merge_ready = false;
                }
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), T_MERGE);
                    if let Some(t) = over {
                        let wait = if self.items[t].kind == Kind::Stack { 150 } else { 500 };
                        SetTimer(Some(self.hwnd), T_MERGE, wait, None);
                    }
                }
            }
            if over.is_some() {
                return; // hold position while deciding whether to stack
            }
        }
        let Some(d) = &self.drag else { return };
        // Where would it land? Before the first pinned slot whose centre is right of the pointer.
        let boundary = self.pinned_boundary();
        let mut k = boundary;
        if !removing {
            for j in 1..boundary {
                if j != i && self.geo.center_abs(j) > x {
                    k = j;
                    break;
                }
            }
        }
        // Landing back where it started means "no change"; an external icon only lands
        // when the pointer is down at the dock.
        let over_dock = y > self.geo.y0 - 70.0 * self.scale && y < self.geo.win_h + 40.0 * self.scale;
        let insert = if removing || (pinned && (k == i || k == i + 1)) || (external && !over_dock) { None } else { Some(k) };
        let (old_insert, old_removing) = (d.insert, d.removing);
        if insert == old_insert && removing == old_removing {
            return;
        }
        if let Some(o) = old_insert {
            self.set_slot_value(o, "Gap", 0.0);
        }
        if let Some(n) = insert {
            self.set_slot_value(n, "Gap", d.width);
        }
        // Close the dragged icon's old space whenever it's going somewhere else.
        let collapse = if insert.is_some() || removing { d.width } else { 0.0 };
        if !external && i + 1 < self.accs.len() {
            self.set_slot_value(i + 1, "Collapse", collapse);
        }
        if let Some(g) = &self.ghost {
            if let Ok(o) = g.body.cast::<CompositionObject>() {
                let _ = crate::ui::animate_scalar(&self.compositor, &o, "Opacity", if removing { 0.55 } else { 1.0 }, 120);
            }
        }
        if let Some(d) = self.drag.as_mut() {
            d.insert = insert;
            d.removing = removing;
        }
        self.set_delta();
        trace!("drag: insert={insert:?} removing={removing}");
    }

    /// The pin strings (app ids and "|") of the pinned items, in dock order.
    fn pin_of(&self, j: usize) -> Pin {
        let it = &self.items[j];
        match it.kind {
            Kind::Separator => Pin::sep(),
            Kind::Stack => self.stack_of(&it.key).map(|(n, _, _)| self.cfg.pins[n].clone()).unwrap_or_else(Pin::sep),
            _ => Pin::Id(it.key.clone()),
        }
    }

    fn pinned_order(&self) -> Vec<(usize, Pin)> {
        (1..self.pinned_boundary()).map(|j| (j, self.pin_of(j))).collect()
    }

    fn end_drag(&mut self, commit: bool) {
        let Some(d) = self.drag.take() else { return };
        if let Some(id) = d.ext.clone() {
            self.end_external_drag(d, &id, commit);
            return;
        }
        let i = d.index;
        let it = self.items[i].clone();
        trace!("drag end: commit={commit} insert={:?} removing={}", d.insert, d.removing);
        if commit && d.removing && it.pinned {
            self.hide_ghost(true);
            let order: Vec<Pin> = self.pinned_order().into_iter().filter(|(j, _)| *j != i).map(|(_, s)| s).collect();
            self.apply_pin_order(order);
            return;
        }
        if let Some(t) = d.merge {
            if let Some(Some(tile)) = self.tiles.get(t) {
                if let Ok(o) = tile.cast::<CompositionObject>() {
                    let _ = crate::ui::animate_v3(&self.compositor, &o, "Scale", crate::ui::v3(1.0, 1.0, 1.0), 150);
                }
            }
        }
        if commit && d.merge_ready && it.kind == Kind::App {
            // Dropped onto an app or stack: make a stack / add to it.
            let t = d.merge.unwrap();
            self.hide_ghost(true);
            let target = self.pin_of(t);
            let mut order: Vec<Pin> = Vec::new();
            for (j, p) in self.pinned_order() {
                if j == i {
                    continue;
                }
                if j == t {
                    order.push(match target.clone() {
                        Pin::Stack { stack, mut apps } => {
                            if !apps.contains(&it.key) {
                                apps.push(it.key.clone());
                            }
                            Pin::Stack { stack, apps }
                        }
                        Pin::Id(a) => Pin::Stack { stack: "New Stack".into(), apps: vec![a, it.key.clone()] },
                    });
                } else {
                    order.push(p);
                }
            }
            self.apply_pin_order(order);
            return;
        }
        if commit {
            if let Some(k) = d.insert {
                self.hide_ghost(false);
                let dragged = self.pin_of(i);
                let without: Vec<(usize, Pin)> = self.pinned_order().into_iter().filter(|(j, _)| *j != i).collect();
                let pos = without.iter().filter(|(j, _)| *j < k).count();
                let mut order: Vec<Pin> = without.into_iter().map(|(_, s)| s).collect();
                order.insert(pos, dragged);
                self.apply_pin_order(order);
                return;
            }
        }
        // Cancel: put everything back.
        self.hide_ghost(false);
        if let Some(n) = d.insert {
            self.set_slot_value(n, "Gap", 0.0);
        }
        if i + 1 < self.accs.len() {
            self.set_slot_value(i + 1, "Collapse", 0.0);
        }
        if let Ok(o) = self.ptr.cast::<CompositionObject>() {
            let _ = crate::ui::animate_scalar(&self.compositor, &o, "Delta", 0.0, 220);
        }
        let _ = self.slots[i].SetOpacity(1.0);
        self.ind_state[i] = IndState::default();
        self.update_indicators();
    }

    fn end_external_drag(&mut self, d: Drag, id: &str, commit: bool) {
        trace!("external drag end: commit={commit} insert={:?} merge_ready={}", d.insert, d.merge_ready);
        // The drag set `hovering` without asking Windows for a leave notice, so the dock
        // would never hear the pointer go and never auto-hide. Re-check once the mouse
        // capture is released (a posted message runs after the button-up handling).
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_APP_RESYNC_HOVER, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = d.merge {
            if let Some(Some(tile)) = self.tiles.get(t) {
                if let Ok(o) = tile.cast::<CompositionObject>() {
                    let _ = crate::ui::animate_v3(&self.compositor, &o, "Scale", crate::ui::v3(1.0, 1.0, 1.0), 150);
                }
            }
        }
        let already = self.cfg.pins.iter().any(|p| p.app() == Some(id));
        if commit && d.merge_ready && d.merge.is_some() {
            self.hide_ghost(true);
            let t = d.merge.unwrap();
            let target = self.pin_of(t);
            let order: Vec<Pin> = self
                .pinned_order()
                .into_iter()
                .filter(|(_, p)| p.app() != Some(id))
                .map(|(j, p)| {
                    if j != t {
                        return p;
                    }
                    match target.clone() {
                        Pin::Stack { stack, mut apps } => {
                            if !apps.iter().any(|a| a == id) {
                                apps.push(id.to_string());
                            }
                            Pin::Stack { stack, apps }
                        }
                        Pin::Id(a) => Pin::Stack { stack: "New Stack".into(), apps: vec![a, id.to_string()] },
                    }
                })
                .collect();
            self.apply_pin_order(order);
            return;
        }
        if commit {
            if let Some(k) = d.insert {
                self.hide_ghost(false);
                let without: Vec<(usize, Pin)> = self.pinned_order().into_iter().filter(|(_, p)| p.app() != Some(id)).collect();
                let pos = without.iter().filter(|(j, _)| *j < k).count();
                let mut order: Vec<Pin> = without.into_iter().map(|(_, p)| p).collect();
                order.insert(pos.min(order.len()), Pin::Id(id.to_string()));
                self.apply_pin_order(order);
                return;
            }
        }
        let _ = already;
        self.hide_ghost(true);
        if let Some(n) = d.insert {
            self.set_slot_value(n, "Gap", 0.0);
        }
        if let Ok(o) = self.ptr.cast::<CompositionObject>() {
            let _ = crate::ui::animate_scalar(&self.compositor, &o, "Delta", 0.0, 220);
        }
    }

    /// Save a new pin order (keeping pins that aren't currently shown, e.g. icon failed).
    fn apply_pin_order(&mut self, mut order: Vec<Pin>) {
        // Keep pinned apps that aren't shown right now (e.g. their icon failed to load).
        let shown: Vec<Pin> = order.clone();
        for p in &self.cfg.pins {
            if let Some(a) = p.app() {
                if !shown.contains(p) && !self.items.iter().any(|it| it.pinned && it.key == a) {
                    order.push(p.clone());
                }
            }
        }
        self.save_and_apply(|c| c.pins = order);
    }

    // ---------------------------------------------------------------- settings & system

    fn reserve_height(&self) -> i32 {
        (self.geo.shelf_h + 2.0 * self.geo.margin).ceil() as i32
    }

    fn apply_system(&mut self) {
        if !self.integrate {
            return;
        }
        if self.helper {
            let h = self.reserve_height();
            taskbar::appbar(self.hwnd, self.monitor, h, !self.cfg.auto_hide, &mut self.appbar);
            return;
        }
        self.helpers.sync(self.cfg.show_on_all_displays);
        if self.cfg.hide_windows_taskbar && !self.taskbar_hidden {
            // (main dock only — helpers returned above)
            self.tray_original = taskbar::take_over();
            taskbar::spawn_watchdog(self.tray_original);
            self.taskbar_hidden = true;
        } else if !self.cfg.hide_windows_taskbar && self.taskbar_hidden {
            taskbar::restore(self.tray_original.take());
            self.taskbar_hidden = false;
        }
        let h = self.reserve_height();
        taskbar::appbar(self.hwnd, self.monitor, h, !self.cfg.auto_hide, &mut self.appbar);
    }

    /// The hover label: a glass pill holding the label text, above everything else.
    fn build_label(&mut self) -> Result<()> {
        let (label, label_geom) = self.glass(0.66, 9.0 * self.scale)?;
        let _ = self.root.Children()?.Remove(&self.label);
        if let Ok(parent) = self.label_text.Parent() {
            let _ = parent.Children()?.Remove(&self.label_text);
        }
        label.Children()?.InsertAtTop(&self.label_text)?;
        label.SetOpacity(0.0)?;
        self.root.Children()?.InsertAtTop(&label)?;
        self.label = label;
        self.label_geom = label_geom;
        Ok(())
    }

    /// The glass brush: Windows' blur alone, or with extra blur on top.
    fn update_glass(&mut self) {
        self.backdrop = if self.cfg.extra_blur > 0.5 {
            match crate::effects::blurred_backdrop(&self.compositor, &self.host_backdrop, self.cfg.extra_blur * self.scale / 2.0) {
                Ok(b) => b,
                Err(e) => {
                    trace!("extra blur unavailable: {e:?}");
                    self.host_backdrop.clone()
                }
            }
        } else {
            self.host_backdrop.clone()
        };
    }

    /// Light or dark glass, from the setting and Windows' app mode. Returns true if it changed.
    fn update_theme(&mut self) -> bool {
        // LAMINAR_THEME=light|dark forces a theme (for testing without changing Windows).
        let forced = std::env::var("LAMINAR_THEME").ok();
        let light = match forced.as_deref() {
            Some("light") => true,
            Some("dark") => false,
            _ => self.cfg.match_windows_theme && !laminar_sys::theme::apps_use_dark(),
        };
        if light == crate::ui::theme::light() {
            return false;
        }
        trace!("theme: {}", if light { "light" } else { "dark" });
        crate::ui::theme::set_light(light);
        true
    }

    /// Redraw everything in the new theme (Windows switched light/dark, or the setting changed).
    fn retheme(&mut self) {
        self.refresh(true);
        let _ = self.build_label();
        if self.launchpad.open {
            // Panels take their glass tint when opened: reopen, keeping the Settings view.
            let settings = self.launchpad.settings;
            self.close_launchpad();
            self.open_launchpad();
            if let Some(sec) = settings {
                self.launchpad.settings = Some(sec);
                self.lp_render("all");
            }
        }
    }

    fn apply_config(&mut self, new: Config) {
        let old = std::mem::replace(&mut self.cfg, new);
        crate::winkey::set_enabled(self.cfg.win_key_launchpad);
        if old.extra_blur != self.cfg.extra_blur {
            self.update_glass();
            let _ = self.build_label();
        }
        if self.update_theme() {
            self.retheme();
        } else if old.visuals_differ(&self.cfg) {
            if old.magnified_size != self.cfg.magnified_size || old.magnify != self.cfg.magnify || old.icon_size != self.cfg.icon_size {
                self.brushes.clear(); // re-render icons at the new size
            }
            self.refresh(true);
        }
        if old.per_display_windows != self.cfg.per_display_windows {
            self.rescan();
        }
        if old.auto_hide != self.cfg.auto_hide {
            if self.cfg.auto_hide {
                unsafe { SetTimer(Some(self.hwnd), T_HIDE, 800, None) };
            } else {
                self.set_tucked(false);
            }
        }
        self.apply_system();
    }

    fn shutdown(&mut self) {
        self.helpers.stop();
        crate::tray::stop();
        if self.integrate {
            taskbar::appbar(self.hwnd, self.monitor, 0, false, &mut self.appbar);
            if self.taskbar_hidden {
                taskbar::restore(self.tray_original.take());
                self.taskbar_hidden = false;
            }
        }
    }

    // ---------------------------------------------------------------- --measure

    fn measure_tick(&mut self) {
        let hwnd = self.hwnd;
        match self.measure_step {
            0 => {
                self.m_cpu0 = metrics::cpu_seconds();
                self.m_t0 = Instant::now();
                self.measure_step = 1;
                unsafe { SetTimer(Some(hwnd), T_MEASURE, 10_000, None) };
            }
            1 => {
                let wall = self.m_t0.elapsed().as_secs_f64();
                let cpu = metrics::cpu_seconds() - self.m_cpu0;
                let m = metrics::memory();
                self.report.push(("idle_seconds".into(), format!("{wall:.1}")));
                self.report.push(("idle_cpu_percent_of_one_core".into(), format!("{:.3}", cpu / wall * 100.0)));
                self.report.push(("working_set_mb".into(), format!("{:.1}", m.working_set)));
                self.report.push(("private_bytes_mb".into(), format!("{:.1}", m.private_bytes)));
                self.report.push(("task_manager_memory_mb".into(), format!("{:.1}", m.private_working_set)));
                self.m_cpu0 = metrics::cpu_seconds();
                self.m_t0 = Instant::now();
                self.sweep_t0 = Instant::now();
                self.measure_step = 2;
                unsafe {
                    SetTimer(Some(hwnd), T_SWEEP, 8, None);
                    SetTimer(Some(hwnd), T_MEASURE, 5_000, None);
                }
            }
            _ => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), T_SWEEP);
                    let _ = KillTimer(Some(hwnd), T_MEASURE);
                }
                let wall = self.m_t0.elapsed().as_secs_f64();
                let cpu = metrics::cpu_seconds() - self.m_cpu0;
                self.report.push(("hover_pointer_moves_per_second".into(), format!("{:.0}", self.sweep_moves as f64 / wall)));
                self.report.push(("hover_cpu_percent_of_one_core".into(), format!("{:.3}", cpu / wall * 100.0)));
                self.on_leave();
                let json = format!("{{\n{}\n}}", self.report.iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect::<Vec<_>>().join(",\n"));
                println!("{json}");
                let _ = std::fs::write("laminar-metrics.json", &json);
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
    }

    fn sweep_tick(&mut self) {
        let t = self.sweep_t0.elapsed().as_secs_f32();
        let g = &self.geo;
        let x = g.x0 + g.shelf_w * (0.5 + 0.48 * (t * std::f32::consts::TAU / 2.5).sin());
        let y = g.y0 + g.shelf_h / 2.0;
        self.sweep_moves += 1;
        self.on_move(x, y);
    }
}

fn lparam_xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32)
}

/// Registered message Explorer broadcasts when its taskbar is (re)created.
static TASKBAR_CREATED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The "SHELLHOOK" message id: Windows tells shell windows about flashing buttons with it.
static SHELLHOOK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// wParam of a SHELLHOOK message when a window asks for attention.
const HSHELL_FLASH: usize = 0x8006;

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| {
                if d.drag.is_some() {
                    d.drag_move(x, y);
                    return;
                }
                if let Some((i, px, py)) = d.press {
                    if ((x - px).powi(2) + (y - py).powi(2)).sqrt() > 6.0 * d.scale && d.draggable(i) {
                        d.press = None;
                        d.start_drag(i);
                        d.drag_move(x, y);
                        return;
                    }
                }
                d.on_move(x, y)
            });
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            with_dock(|d| d.on_leave());
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| {
                d.pressed = d.geo.hit(x, y, 1.0);
                d.press = d.pressed.map(|i| (i, x, y));
            });
            unsafe {
                SetCapture(hwnd); // keep receiving the pointer while dragging off the dock
            }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| {
                d.press = None;
                if d.drag.is_some() {
                    d.end_drag(true);
                } else {
                    let hit = d.geo.hit(x, y, 1.0);
                    if hit.is_some() && hit == d.pressed {
                        d.on_click(hit.unwrap());
                    }
                }
                d.pressed = None;
            });
            unsafe {
                let _ = ReleaseCapture();
            }
            return LRESULT(0);
        }
        WM_CAPTURECHANGED => {
            with_dock(|d| {
                d.press = None;
                if d.drag.is_some() {
                    d.end_drag(false);
                }
            });
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| d.context_menu(x, y, None));
            return LRESULT(0);
        }
        WM_APP_RESCAN => {
            // A window changed. If Explorer re-showed its taskbar, hide it again.
            let h = HWND(lp.0 as _);
            with_dock(|d| {
                if wp.0 as u32 == EVENT_OBJECT_SHOW && d.taskbar_hidden && tracker::is_explorer_taskbar(h) {
                    taskbar::set_visible(false);
                }
                if wp.0 as u32 == EVENT_SYSTEM_FOREGROUND {
                    d.fg = tracker::foreground();
                    d.attention.remove(&(d.fg.0 as isize)); // looked at: clear its dot
                    d.update_indicators();
                }
            });
            unsafe { SetTimer(Some(hwnd), T_RESCAN, 60, None) }; // debounce bursts of events
            return LRESULT(0);
        }
        WM_APP_SVC => {
            with_dock(|d| d.on_service_replies());
            return LRESULT(0);
        }
        WM_APP_WINKEY => {
            with_dock(|d| d.toggle_launchpad());
            return LRESULT(0);
        }
        WM_APP_RESYNC_HOVER => {
            with_dock(|d| {
                if d.drag.is_none() {
                    d.resync_hover();
                }
            });
            return LRESULT(0);
        }
        WM_APP_TRAY => {
            with_dock(|d| {
                if d.launchpad.open {
                    unsafe { SetTimer(Some(hwnd), T_TRAY_RENDER, 150, None) };
                } else {
                    crate::tray::with_icons(|_| ()); // clears the pending flag
                }
            });
            return LRESULT(0);
        }
        WM_APP_CONFIG => {
            unsafe { SetTimer(Some(hwnd), T_CONFIG, 150, None) }; // editors often write twice
            return LRESULT(0);
        }
        WM_TIMER => {
            let id = wp.0;
            with_dock(|d| match id {
                T_SHRINK => unsafe {
                    let _ = KillTimer(Some(hwnd), T_SHRINK);
                    if !d.hovering {
                        d.apply_region();
                    }
                },
                T_RESCAN => unsafe {
                    let _ = KillTimer(Some(hwnd), T_RESCAN);
                    d.rescan();
                },
                T_REVEAL => d.on_reveal_timer(),
                T_TRAY_RAISE => crate::tray::raise(),
                T_TRAY_RENDER => unsafe {
                    let _ = KillTimer(Some(hwnd), T_TRAY_RENDER);
                    d.lp_render("right");
                },
                T_FILES => unsafe {
                    let _ = KillTimer(Some(hwnd), T_FILES);
                    d.files_gen += 1;
                    d.services.send(Req::Files(d.files_gen, d.launchpad.query.trim().to_string()));
                },
                T_LP_TICK => {
                    // While Launchpad is open: refresh stats (and with them the clock).
                    d.services.send(Req::Stats);
                }
                T_WEATHER => {
                    d.services.send(Req::Weather(d.cfg.weather_lat, d.cfg.weather_lon));
                }
                T_PREVIEW => unsafe {
                    let _ = KillTimer(Some(hwnd), T_PREVIEW);
                    if let Some(i) = d.preview_pending.take() {
                        if d.hover == Some(i) {
                            d.show_preview(i);
                        }
                    }
                },
                T_DRAG_PREVIEW => unsafe {
                    let _ = KillTimer(Some(hwnd), T_DRAG_PREVIEW);
                    if let (Some(i), true) = (d.drag_under, d.file_drag.is_some()) {
                        trace!("drag rested on '{}': showing its windows", d.items[i].key);
                        d.show_preview(i);
                    }
                },
                T_DRAG_SWITCH => unsafe {
                    let _ = KillTimer(Some(hwnd), T_DRAG_SWITCH);
                    if let Some(h) = d.drag_switch.take() {
                        d.hide_preview();
                        let ok = tracker::activate_during_drag(h);
                        trace!("drag rested on a window tile: brought it forward = {ok}");
                        if d.cfg.auto_hide && !d.hovering {
                            SetTimer(Some(hwnd), T_HIDE, d.cfg.hide_delay_ms.max(1), None);
                        }
                    }
                },
                T_PREVIEW_HIDE => unsafe {
                    let _ = KillTimer(Some(hwnd), T_PREVIEW_HIDE);
                    let mut p = POINT::default();
                    let _ = GetCursorPos(&mut p);
                    let under = WindowFromPoint(p);
                    if !d.synthetic() && under != d.preview.hwnd() && !(under == d.hwnd && d.hover.is_some()) {
                        d.preview.hide();
                        if d.cfg.auto_hide && !d.hovering {
                            SetTimer(Some(hwnd), T_HIDE, d.cfg.hide_delay_ms.max(1), None);
                        }
                    }
                },
                T_MERGE => unsafe {
                    let _ = KillTimer(Some(hwnd), T_MERGE);
                    let target = d.drag.as_ref().and_then(|dr| dr.merge);
                    if let Some(t) = target {
                        if let Some(dr) = d.drag.as_mut() {
                            dr.merge_ready = true;
                        }
                        if let Some(Some(tile)) = d.tiles.get(t) {
                            let b = d.geo.b;
                            let _ = tile.SetCenterPoint(v3(b / 2.0, b / 2.0, 0.0));
                            if let Ok(o) = tile.cast::<CompositionObject>() {
                                let _ = crate::ui::animate_v3(&d.compositor, &o, "Scale", v3(1.18, 1.18, 1.0), 180);
                            }
                        }
                        trace!("merge ready over item {t}");
                    }
                },
                T_GHOST => unsafe {
                    let _ = KillTimer(Some(hwnd), T_GHOST);
                    if let Some(g) = &d.ghost {
                        g.popup.hide();
                    }
                },
                T_TUCKED => unsafe {
                    let _ = KillTimer(Some(hwnd), T_TUCKED);
                    if d.tucked {
                        d.apply_region();
                    }
                },
                T_HIDE => unsafe {
                    let _ = KillTimer(Some(hwnd), T_HIDE);
                    if d.cfg.auto_hide && !d.hovering && !d.menu_open && !d.preview.open && d.drag.is_none() && !d.launchpad.open {
                        if d.stack_view.open {
                            // Stay up under an open stack (like macOS); check again until it closes.
                            SetTimer(Some(hwnd), T_HIDE, 250, None);
                        } else {
                            d.set_tucked(true);
                        }
                    } else if d.cfg.auto_hide && !d.tucked {
                        trace!(
                            "hide timer: staying up (hovering={} menu={} preview={} drag={} launchpad={})",
                            d.hovering,
                            d.menu_open,
                            d.preview.open,
                            d.drag.is_some(),
                            d.launchpad.open
                        );
                    }
                },
                T_CONFIG => unsafe {
                    let _ = KillTimer(Some(hwnd), T_CONFIG);
                    if let Some(c) = Config::load() {
                        if c != d.cfg {
                            d.apply_config(c);
                        }
                    }
                },
                T_MEASURE => d.measure_tick(),
                T_SWEEP => d.sweep_tick(),
                T_EXIT => unsafe {
                    d.shutdown();
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            });
            return LRESULT(0);
        }
        WM_SETTINGCHANGE => {
            // "ImmersiveColorSet": Windows switched between light and dark mode.
            let area = if lp.0 != 0 { unsafe { windows::core::PCWSTR(lp.0 as *const u16).to_string().unwrap_or_default() } } else { String::new() };
            if area == "ImmersiveColorSet" {
                with_dock(|d| {
                    if d.update_theme() {
                        d.retheme();
                    }
                });
            }
            return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_dock(|d| {
                let (m, dpi) = this_display(&d.opts);
                d.monitor = m;
                d.scale = if msg == WM_DPICHANGED { (wp.0 & 0xFFFF) as f32 / 96.0 } else { dpi as f32 / 96.0 };
                d.brushes.clear();
                d.refresh(true);
                d.apply_system();
            });
            return LRESULT(0);
        }
        WM_CLOSE => {
            with_dock(|d| d.shutdown());
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return LRESULT(0);
        }
        WM_ENDSESSION => {
            // Windows is signing out or shutting down: put the taskbar back first.
            if wp.0 != 0 {
                with_dock(|d| d.shutdown());
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            with_dock(|d| d.shutdown());
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    let sh = SHELLHOOK.load(std::sync::atomic::Ordering::Relaxed);
    if sh != 0 && msg == sh {
        if wp.0 == HSHELL_FLASH {
            let h = HWND(lp.0 as _);
            with_dock(|d| d.on_flash(h));
        }
        return LRESULT(0);
    }
    let tc = TASKBAR_CREATED.load(std::sync::atomic::Ordering::Relaxed);
    if tc != 0 && msg == tc {
        // Explorer restarted: its new taskbar needs hiding and our AppBar re-registering.
        with_dock(|d| {
            d.taskbar_hidden = false;
            d.appbar = false;
            d.apply_system();
        });
        crate::tray::on_taskbar_created();
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn user_name() -> String {
    let n = std::env::var("USERNAME").unwrap_or_else(|_| "User".into());
    let mut c = n.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => n,
    }
}

/// OLE drop target callback: route to the dock.
fn drop_callback(ev: crate::filedrop::Event) -> crate::filedrop::Target {
    let mut t = crate::filedrop::Target::None;
    with_dock(|d| t = d.on_drop_event(ev));
    t
}

/// OLE drop target callback for the window previews.
fn preview_drop_callback(ev: crate::filedrop::Event) -> crate::filedrop::Target {
    with_dock(|d| d.on_preview_drag(ev));
    crate::filedrop::Target::None
}

/// Window procedure shared by all pop-up windows; the dock handles their messages.
extern "system" fn popup_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mut r = None;
    with_dock(|d| r = d.popup_message(hwnd, msg, wp, lp));
    r.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
}

/// The display this dock lives on (primary, or the helper's display) and its DPI.
fn this_display(opts: &Options) -> (RECT, u32) {
    let all = crate::displays::all();
    let d = opts.monitor.and_then(|i| all.get(i).copied()).or_else(|| all.first().copied());
    match d {
        Some(d) => (d.rect, d.dpi),
        None => (primary_monitor(), 96),
    }
}

fn primary_monitor() -> RECT {
    unsafe {
        let m = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        mi.rcMonitor
    }
}

pub fn run(opts: Options, started: Instant) -> Result<()> {
    let integrate = !(opts.measure || opts.demo_hover || opts.demo.is_some());

    // Settings: first run writes defaults with the apps installed on this PC.
    let cfg = match Config::load() {
        Some(c) => c,
        None => {
            let mut c = Config::default();
            c.pins = apps::default_pins();
            // Only create the file if there isn't one; never overwrite a file that
            // merely failed to parse (it may hold the user's pins).
            if !Config::exists() {
                c.save();
            }
            c
        }
    };

    unsafe {
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), std::sync::atomic::Ordering::Relaxed);
        let instance = GetModuleHandleW(None)?;
        let class = w!("LaminarDock");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class,
            w!("Laminar Dock"),
            WS_POPUP,
            0,
            0,
            100,
            100,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        // Allow the live desktop blur, and keep the dock visible during Aero Peek.
        let on: i32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, &on as *const _ as _, 4)?;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_EXCLUDED_FROM_PEEK, &on as *const _ as _, 4);
        // Start fully clipped so nothing is clickable until the dock is built.
        SetWindowRgn(hwnd, Some(CreateRectRgn(0, 0, 0, 0)), false);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let (monitor, dpi) = this_display(&opts);
        let scale = dpi as f32 / 96.0;
        let compositor = Compositor::new()?;
        let target = compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)?;
        let root = compositor.CreateContainerVisual()?;
        target.SetRoot(&root)?;
        let gfx = Gfx::new(&compositor)?;
        let ptr = compositor.CreatePropertySet()?;
        ptr.InsertScalar(&hs("PointerX"), 0.0)?;
        ptr.InsertScalar(&hs("Amount"), 0.0)?;
        ptr.InsertScalar(&hs("Delta"), 0.0)?;
        let dock_visual = compositor.CreateContainerVisual()?;
        root.Children()?.InsertAtTop(&dock_visual)?;
        crate::ui::register_popup_class(popup_wndproc)?;
        let menu = Menu::new(&compositor)?;
        let preview = Preview::new(&compositor)?;
        let stack_view = StackView::new(&compositor)?;
        let launchpad = Launchpad::new(&compositor)?;

        let mut dock = Dock {
            hwnd,
            opts: opts.clone(),
            started,
            geo: Geo::new(&[], &cfg, scale, (monitor.right - monitor.left) as f32),
            cfg,
            monitor,
            scale,
            backdrop: compositor.CreateHostBackdropBrush()?.cast()?,
            host_backdrop: compositor.CreateHostBackdropBrush()?.cast()?,
            label: compositor.CreateContainerVisual()?,
            label_geom: compositor.CreateRoundedRectangleGeometry()?,
            label_text: compositor.CreateSpriteVisual()?,
            dock: dock_visual,
            compositor,
            _target: target,
            gfx,
            icons: IconCache::open(),
            brushes: HashMap::new(),
            tray_brushes: HashMap::new(),
            names: HashMap::new(),
            ptr,
            root,
            slots: Vec::new(),
            accs: Vec::new(),
            tiles: Vec::new(),
            inds: Vec::new(),
            ind_state: Vec::new(),
            atts: Vec::new(),
            attention: Default::default(),
            items: Vec::new(),
            tracker: Tracker::default(),
            windows: Vec::new(),
            running_order: Vec::new(),
            fg: HWND::default(),
            resolved: HashMap::new(),
            hovering: false,
            hover: None,
            pressed: None,
            tucked: false,
            menu_open: false,
            menu,
            drag: None,
            press: None,
            ghost: None,
            preview,
            preview_pending: None,
            stack_view,
            menu_stack: None,
            launchpad,
            services: Services::start(hwnd),
            lp_apps: Vec::new(),
            control: ControlState::default(),
            stats: None,
            weather: None,
            files: Vec::new(),
            files_gen: 0,
            icons_requested: std::collections::HashSet::new(),
            user_name: user_name(),
            lp_drag: None,
            lp_press: None,
            lp_menu_app: None,
            drop_target: None,
            file_drag: None,
            wifi: None,
            update: None,
            update_asked: None,
            _drop: None,
            _preview_drop: None,
            drag_under: None,
            drag_switch: None,
            drag_preview: false,
            menu_ctx: None,
            integrate,
            helper: opts.monitor.is_some(),
            helpers: crate::displays::Helpers::default(),
            appbar: false,
            tray_original: None,
            taskbar_hidden: false,
            measure_step: 0,
            m_cpu0: 0.0,
            m_t0: Instant::now(),
            report: Vec::new(),
            sweep_t0: Instant::now(),
            sweep_moves: 0,
            last_scan_ms: 0.0,
            last_refresh_ms: 0.0,
        };
        dock.update_theme();
        dock.update_glass();
        dock.build_label()?;

        let before_dock_ms = dock.started.elapsed().as_secs_f64() * 1000.0;
        dock.rescan(); // finds running windows and builds the dock
        let _ = DwmFlush();
        let startup_ms = dock.started.elapsed().as_secs_f64() * 1000.0;
        let _hooks = tracker::install_hooks(hwnd);
        // Hear about windows flashing for attention, like the Windows taskbar does.
        SHELLHOOK.store(RegisterWindowMessageW(w!("SHELLHOOK")), std::sync::atomic::Ordering::Relaxed);
        let shell_hook = RegisterShellHookWindow(hwnd).as_bool();
        trace!("shell hook registered: {shell_hook}");
        dock._drop = match crate::filedrop::register(hwnd, drop_callback) {
            Ok(t) => Some(t),
            Err(e) => {
                trace!("file drop unavailable: {e:?}");
                None
            }
        };
        trace!("file drop target registered: {}", dock._drop.is_some());
        // Drags can also rest on window tiles, to bring a window forward and drop into it.
        dock._preview_drop = crate::filedrop::register(dock.preview.hwnd(), preview_drop_callback).ok();
        dock.apply_system();
        // Warm up Launchpad data in the background; refresh the weather every 30 minutes.
        dock.services.send(Req::Apps);
        dock.services.send(Req::Weather(dock.cfg.weather_lat, dock.cfg.weather_lon));
        dock.services.send(Req::Control);
        SetTimer(Some(hwnd), T_WEATHER, 30 * 60 * 1000, None);
        let main_dock = integrate && opts.monitor.is_none();
        let _kb = if main_dock { crate::winkey::install(hwnd, dock.cfg.win_key_launchpad) } else { None };
        if main_dock {
            // Host the tray icons apps put beside the clock (shown in Launchpad).
            crate::tray::start(hwnd, WM_APP_TRAY);
            if crate::tray::active() {
                SetTimer(Some(hwnd), T_TRAY_RAISE, 1500, None);
            }
        }
        if let Some(pid) = opts.parent {
            crate::displays::watch_parent(pid, hwnd);
        }

        if dock.opts.measure {
            let m = metrics::memory();
            dock.report.push(("startup_ms".into(), format!("{startup_ms:.0}")));
            dock.report.push(("startup_before_dock_ms".into(), format!("{before_dock_ms:.0}")));
            dock.report.push(("startup_window_scan_ms".into(), format!("{:.0}", dock.last_scan_ms)));
            dock.report.push(("startup_icons_and_build_ms".into(), format!("{:.0}", dock.last_refresh_ms)));
            dock.report.push(("icons".into(), format!("{}", dock.items.iter().filter(|i| i.is_icon()).count())));
            dock.report.push(("running_windows".into(), format!("{}", dock.windows.len())));
            dock.report.push(("private_bytes_at_start_mb".into(), format!("{:.1}", m.private_bytes)));
            SetTimer(Some(hwnd), T_MEASURE, 2_000, None);
        }
        if dock.opts.demo_hover {
            let g = &dock.geo;
            let (x, y) = (g.center_abs(4.min(g.left.len().saturating_sub(1))), g.y0 + g.shelf_h / 2.0);
            dock.on_move(x, y);
        }
        if let Some(name) = dock.opts.demo.clone() {
            dock.run_demo(&name);
        }
        if let Some(secs) = dock.opts.exit_after {
            SetTimer(Some(hwnd), T_EXIT, secs * 1000, None);
        }
        if dock.cfg.auto_hide && integrate {
            SetTimer(Some(hwnd), T_HIDE, 1500, None); // show briefly at start, then tuck away
        }
        DOCK.with(|d| *d.borrow_mut() = Some(dock));

        // Message loop that also wakes when the settings folder changes (no polling).
        let _ = std::fs::create_dir_all(config::dir());
        let watch: Option<HANDLE> =
            FindFirstChangeNotificationW(&HSTRING::from(config::dir().as_os_str()), false, FILE_NOTIFY_CHANGE_LAST_WRITE).ok();
        let handles: Vec<HANDLE> = watch.into_iter().collect();
        let mut msg = MSG::default();
        'outer: loop {
            let r = MsgWaitForMultipleObjects(Some(&handles), false, INFINITE, QS_ALLINPUT);
            if !handles.is_empty() && r == WAIT_OBJECT_0 {
                let _ = FindNextChangeNotification(handles[0]);
                let _ = PostMessageW(Some(hwnd), WM_APP_CONFIG, WPARAM(0), LPARAM(0));
            }
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'outer;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        DOCK.with(|d| *d.borrow_mut() = None);
    }
    Ok(())
}
