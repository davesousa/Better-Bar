//! The dock window and its Composition visual tree.
//!
//! Magnification is done with *expression animations*: Rust only writes the
//! pointer position into a property set, and the compositor computes every
//! icon's scale and position on the GPU each frame. The UI thread is idle
//! unless the pointer moves or the user clicks.

use std::cell::RefCell;
use std::f32::consts::PI;
use std::time::Instant;

use windows::core::{w, Interface, Result, HSTRING};
use windows_numerics::{Vector2, Vector3};
use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::*;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmFlush, DwmSetWindowAttribute, DWMWA_EXCLUDED_FROM_PEEK, DWMWA_USE_HOSTBACKDROPBRUSH,
};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::apps::{self, Item, Kind};
use crate::gfx::Gfx;
use crate::{metrics, Options};

/// Defined in Win32::UI::Controls; declared here to avoid pulling in that whole module.
const WM_MOUSELEAVE: u32 = 0x02A3;
const T_SHRINK: usize = 1;
const T_MEASURE: usize = 2;
const T_EXIT: usize = 3;
const T_SWEEP: usize = 4;

/// Visual design constants in device-independent pixels (match the prototype defaults).
struct Design {
    icon: f32,
    magnified: f32,
    gap: f32,
    pad: f32,
    radius: f32,
    margin: f32,
    spread: f32,
    tint: f32,
    edge: f32,
}
const DESIGN: Design = Design {
    icon: 54.0,
    magnified: 88.0,
    gap: 8.0,
    pad: 8.0,
    radius: 24.0,
    margin: 8.0,
    spread: 2.6,
    tint: 0.24,
    edge: 0.55,
};

/// Layout in physical pixels for the current DPI.
struct Geo {
    b: f32,      // icon size
    m: f32,      // magnified size
    g: f32,      // gap
    p: f32,      // shelf padding
    r: f32,      // corner radius
    range: f32,  // magnification reach in px
    sep_w: f32,  // separator slot width
    win_w: f32,
    win_h: f32,
    x0: f32,     // shelf left at rest (window coords)
    y0: f32,     // shelf top (window coords)
    shelf_w: f32,
    shelf_h: f32,
    left: Vec<f32>,  // slot left edge relative to shelf content start
    width: Vec<f32>, // slot width (icon + gap, or separator)
    icon: Vec<bool>,
    hover_rect: RECT,
}

impl Geo {
    fn new(items: &[Item], scale: f32, win_w: f32) -> Self {
        let d = &DESIGN;
        let (b, m, g, p) = (d.icon * scale, d.magnified * scale, d.gap * scale, d.pad * scale);
        let sep_w = 13.0 * scale + g;
        let shelf_h = b + 2.0 * p;
        let r = (d.radius * scale).min(shelf_h / 2.0);
        let label_room = 40.0 * scale;
        let win_h = (m - b) + shelf_h + d.margin * scale + label_room;
        let mut left = Vec::new();
        let mut width = Vec::new();
        let mut x = 0.0;
        for it in items {
            let w = if it.is_icon() { b + g } else { sep_w };
            left.push(x);
            width.push(w);
            x += w;
        }
        let shelf_w = x + 2.0 * p;
        let x0 = ((win_w - shelf_w) / 2.0).round();
        let y0 = win_h - d.margin * scale - shelf_h;
        let mut geo = Geo {
            b, m, g, p, r,
            range: d.spread * (b + g),
            sep_w, win_w, win_h, x0, y0, shelf_w, shelf_h,
            left, width,
            icon: items.iter().map(|i| i.is_icon()).collect(),
            hover_rect: RECT::default(),
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
        geo.hover_rect = RECT {
            left: (lo - 4.0) as i32,
            top: 0,
            right: (hi + 4.0) as i32,
            bottom: (y0 + shelf_h) as i32,
        };
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

/// Everything the window procedure needs.
struct Dock {
    hwnd: HWND,
    opts: Options,
    started: Instant,
    compositor: Compositor,
    _target: DesktopWindowTarget,
    gfx: Gfx,
    ptr: CompositionPropertySet,
    root: ContainerVisual,
    dock: ContainerVisual,
    slots: Vec<ContainerVisual>,
    tiles: Vec<Option<SpriteVisual>>,
    label: ContainerVisual,
    label_geom: CompositionRoundedRectangleGeometry,
    label_text: SpriteVisual,
    items: Vec<Item>,
    geo: Geo,
    scale: f32,
    hovering: bool,
    hover: Option<usize>,
    pressed: Option<usize>,
    // measurement
    measure_step: u32,
    m_cpu0: f64,
    m_t0: Instant,
    report: Vec<(String, String)>,
    sweep_t0: Instant,
    sweep_moves: u32,
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

impl Dock {
    /// Glass = live blurred desktop (HostBackdropBrush) + tint, clipped to a rounded rectangle.
    fn glass(
        compositor: &Compositor,
        backdrop: &CompositionBackdropBrush,
        tint: f32,
        radius: f32,
    ) -> Result<(ContainerVisual, CompositionRoundedRectangleGeometry)> {
        let c = compositor.CreateContainerVisual()?;
        let blur = compositor.CreateSpriteVisual()?;
        blur.SetBrush(backdrop)?;
        blur.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
        let t = compositor.CreateSpriteVisual()?;
        t.SetBrush(&compositor.CreateColorBrushWithColor(Color { A: (tint * 255.0) as u8, R: 30, G: 30, B: 38 })?)?;
        t.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
        c.Children()?.InsertAtTop(&blur)?;
        c.Children()?.InsertAtTop(&t)?;
        let geom = compositor.CreateRoundedRectangleGeometry()?;
        geom.SetCornerRadius(v2(radius, radius))?;
        c.SetClip(&compositor.CreateGeometricClipWithGeometry(&geom)?)?;
        Ok((c, geom))
    }

    fn expr(&self, text: &str, refs: &[(&str, &CompositionObject)]) -> Result<ExpressionAnimation> {
        let e = self.compositor.CreateExpressionAnimationWithExpression(&hs(text))?;
        e.SetReferenceParameter(&hs("ptr"), &self.ptr)?;
        for (name, obj) in refs {
            e.SetReferenceParameter(&hs(name), *obj)?;
        }
        Ok(e)
    }

    fn build(&mut self) -> Result<()> {
        let geo = &self.geo;
        let c = &self.compositor;
        let k = geo.m / geo.b - 1.0;
        let backdrop = c.CreateHostBackdropBrush()?;

        // Pointer state shared with every expression.
        self.ptr.InsertScalar(&hs("PointerX"), geo.x0 + geo.shelf_w / 2.0)?;
        self.ptr.InsertScalar(&hs("Amount"), 0.0)?;

        self.root.SetSize(v2(geo.win_w, geo.win_h))?;
        self.dock.SetOffset(v3(geo.x0, geo.y0, 0.0))?;
        self.root.Children()?.InsertAtTop(&self.dock)?;

        // Slots (one container per item) — created first so expressions can reference them.
        for (i, it) in self.items.iter().enumerate() {
            let slot = c.CreateContainerVisual()?;
            if it.is_icon() {
                slot.SetSize(v2(geo.b, geo.b))?;
                slot.SetCenterPoint(v3(0.0, geo.b, 0.0))?; // grow up and to the right
                let tile = c.CreateSpriteVisual()?;
                tile.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
                let px = geo.m.ceil();
                let surface = match it.kind {
                    Kind::Launch => self.gfx.logo_surface(px)?,
                    _ => self.gfx.icon_surface(it.bitmap.unwrap(), px)?,
                };
                let brush = c.CreateSurfaceBrushWithSurface(&surface)?;
                brush.SetStretch(CompositionStretch::Fill)?;
                brush.SetBitmapInterpolationMode(CompositionBitmapInterpolationMode::Linear)?;
                tile.SetBrush(&brush)?;
                slot.Children()?.InsertAtTop(&tile)?;
                self.tiles.push(Some(tile));
            } else {
                let line = c.CreateSpriteVisual()?;
                line.SetBrush(&c.CreateColorBrushWithColor(Color { A: 87, R: 245, G: 245, B: 247 })?)?;
                let lw = (1.0 * self.scale).round().max(1.0);
                line.SetSize(v2(lw, geo.b * 0.72))?;
                line.SetOffset(v3(((geo.sep_w - lw) / 2.0).round(), geo.b * 0.14, 0.0))?;
                slot.Children()?.InsertAtTop(&line)?;
                self.tiles.push(None);
            }
            let _ = i;
            self.slots.push(slot);
        }

        // Growth term for slot j: how much wider it is than at rest.
        let grow = |j: usize| format!("((s{j}.Scale.X - 1) * {b})", b = geo.b);
        let refs_all: Vec<(String, CompositionObject)> = self
            .slots
            .iter()
            .enumerate()
            .map(|(j, s)| (format!("s{j}"), s.cast::<CompositionObject>().unwrap()))
            .collect();
        let refs = |n: usize| -> Vec<(&str, &CompositionObject)> {
            refs_all[..n].iter().map(|(a, b)| (a.as_str(), b)).collect()
        };

        // Each icon's scale: cosine falloff from the pointer, blended by Amount.
        for (i, slot) in self.slots.iter().enumerate() {
            if !geo.icon[i] {
                continue;
            }
            let s = format!(
                "1 + {k} * ptr.Amount * ((Cos(Min(Abs(ptr.PointerX - {c}) / {r}, 1) * 3.14159265) + 1) / 2)",
                c = geo.center_abs(i),
                r = geo.range
            );
            slot.StartAnimation(&hs("Scale"), &self.expr(&format!("Vector3({s}, {s}, 1)"), &[])?)?;
        }
        // Each slot's position: rest position + growth of everything to its left.
        for (i, slot) in self.slots.iter().enumerate() {
            let extra = (0..i).filter(|j| geo.icon[*j]).map(&grow).collect::<Vec<_>>();
            let base_x = geo.p + geo.left[i] + if geo.icon[i] { geo.g / 2.0 } else { 0.0 };
            let x = if extra.is_empty() { format!("{base_x}") } else { format!("{base_x} + {}", extra.join(" + ")) };
            slot.StartAnimation(&hs("Offset"), &self.expr(&format!("Vector3({x}, {y}, 0)", y = geo.p), &refs(i))?)?;
        }
        // Shelf position: shift so the point under the pointer stays put.
        let shift: Vec<String> = (0..self.slots.len())
            .filter(|j| geo.icon[*j])
            .map(|j| {
                format!(
                    "{g} * Clamp((ptr.PointerX - {l}) / {w}, 0, 1)",
                    g = grow(j),
                    l = geo.x0 + geo.p + geo.left[j],
                    w = geo.width[j]
                )
            })
            .collect();
        let n = self.slots.len();
        self.dock.StartAnimation(
            &hs("Offset"),
            &self.expr(&format!("Vector3({x0} - ({s}), {y0}, 0)", x0 = geo.x0, s = shift.join(" + "), y0 = geo.y0), &refs(n))?,
        )?;
        // Shelf width: rest width + total growth.
        let all_grow: Vec<String> = (0..n).filter(|j| geo.icon[*j]).map(&grow).collect();
        let size_expr = format!("Vector2({w} + {g}, {h})", w = geo.shelf_w, g = all_grow.join(" + "), h = geo.shelf_h);

        let (glass, geom) = Self::glass(c, &backdrop, DESIGN.tint, geo.r)?;
        glass.StartAnimation(&hs("Size"), &self.expr(&size_expr, &refs(n))?)?;
        geom.StartAnimation(&hs("Size"), &self.expr(&size_expr, &refs(n))?)?;
        self.dock.Children()?.InsertAtBottom(&glass)?;

        // Rim highlight as a nine-grid so the rounded ends never stretch.
        let rim_w = (geo.r * 2.0 + 4.0).ceil();
        let rim_surface = self.gfx.rim_surface(rim_w, geo.shelf_h, geo.r, DESIGN.edge)?;
        let nine = c.CreateNineGridBrush()?;
        let rim_brush = c.CreateSurfaceBrushWithSurface(&rim_surface)?;
        // Without an explicit Fill the nine-grid draws the source once, unstretched, in the centre.
        rim_brush.SetStretch(CompositionStretch::Fill)?;
        nine.SetSource(&rim_brush)?;
        nine.SetLeftInset(geo.r + 1.0)?;
        nine.SetRightInset(geo.r + 1.0)?;
        nine.SetTopInset(0.0)?;
        nine.SetBottomInset(0.0)?;
        let rim = c.CreateSpriteVisual()?;
        rim.SetBrush(&nine)?;
        rim.StartAnimation(&hs("Size"), &self.expr(&size_expr, &refs(n))?)?;
        self.dock.Children()?.InsertAtTop(&rim)?;

        for slot in &self.slots {
            self.dock.Children()?.InsertAtTop(slot)?;
        }

        // Hover label (glass pill + DirectWrite text), positioned by expression.
        let (label, label_geom) = Self::glass(c, &backdrop, 0.66, 9.0 * self.scale)?;
        label.Children()?.InsertAtTop(&self.label_text)?;
        label.SetOpacity(0.0)?;
        self.root.Children()?.InsertAtTop(&label)?;
        self.label = label;
        self.label_geom = label_geom;
        Ok(())
    }

    fn animate_amount(&self, to: f32, duration_ms: i64) -> Result<()> {
        let c = &self.compositor;
        let a = c.CreateScalarKeyFrameAnimation()?;
        let ease = c.CreateCubicBezierEasingFunction(v2(0.25, 0.8), v2(0.25, 1.0))?;
        a.InsertKeyFrameWithEasingFunction(1.0, to, &ease)?;
        a.SetDuration(ms(duration_ms))?;
        self.ptr.StartAnimation(&hs("Amount"), &a)
    }

    fn set_region(&self, hover: bool) {
        let g = &self.geo;
        let r = if hover {
            g.hover_rect
        } else {
            RECT { left: g.x0 as i32, top: g.y0 as i32, right: (g.x0 + g.shelf_w) as i32, bottom: (g.y0 + g.shelf_h) as i32 }
        };
        unsafe {
            let rgn = CreateRectRgn(r.left, r.top, r.right, r.bottom);
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
    }

    fn on_move(&mut self, x: f32, y: f32) {
        if !self.hovering {
            self.hovering = true;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_SHRINK);
            }
            // Simulated pointers (--measure, --demo-hover) must not register for real leave events.
            let synthetic = self.opts.measure || self.opts.demo_hover;
            if !synthetic {
                unsafe {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: self.hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                }
            }
            self.set_region(true);
            let _ = self.animate_amount(1.0, 160);
        }
        let _ = self.ptr.InsertScalar(&hs("PointerX"), x);
        let hit = self.geo.hit(x, y, 1.0);
        if hit != self.hover {
            self.hover = hit;
            let _ = self.show_label(hit);
        }
    }

    fn on_leave(&mut self) {
        self.hovering = false;
        self.hover = None;
        self.pressed = None;
        let _ = self.show_label(None);
        let _ = self.animate_amount(0.0, 260);
        unsafe {
            SetTimer(Some(self.hwnd), T_SHRINK, 320, None);
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
        let (surface, tw, th) = self.gfx.text_surface(&self.items[i].name, 13.0 * s)?;
        let brush = c.CreateSurfaceBrushWithSurface(&surface)?;
        self.label_text.SetBrush(&brush)?;
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

    fn on_click(&mut self, i: usize) {
        let _ = self.bounce(i);
        match self.items[i].kind {
            Kind::App | Kind::Trash => apps::launch(&self.items[i].target),
            _ => {} // Launchpad arrives in Phase 3.
        }
    }

    /// --measure: settle, measure idle, then a simulated hover sweep.
    fn measure_tick(&mut self) {
        let hwnd = self.hwnd;
        match self.measure_step {
            0 => {
                // settled → start idle window
                self.m_cpu0 = metrics::cpu_seconds();
                self.m_t0 = Instant::now();
                self.measure_step = 1;
                unsafe { SetTimer(Some(hwnd), T_MEASURE, 10_000, None) };
            }
            1 => {
                let wall = self.m_t0.elapsed().as_secs_f64();
                let cpu = metrics::cpu_seconds() - self.m_cpu0;
                let (ws, private) = metrics::memory_mb();
                self.report.push(("idle_seconds".into(), format!("{wall:.1}")));
                self.report.push(("idle_cpu_percent_of_one_core".into(), format!("{:.3}", cpu / wall * 100.0)));
                self.report.push(("idle_cpu_percent_of_total".into(), format!("{:.4}", cpu / wall * 100.0 / metrics::cores())));
                self.report.push(("working_set_mb".into(), format!("{ws:.1}")));
                self.report.push(("private_bytes_mb".into(), format!("{private:.1}")));
                self.report.push(("task_manager_memory_mb".into(), format!("{:.1}", metrics::memory().private_working_set)));
                // hover sweep for 5 s
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
                let (ws, private) = metrics::memory_mb();
                self.report.push(("hover_seconds".into(), format!("{wall:.1}")));
                self.report.push(("hover_pointer_moves_per_second".into(), format!("{:.0}", self.sweep_moves as f64 / wall)));
                self.report.push(("hover_cpu_percent_of_one_core".into(), format!("{:.3}", cpu / wall * 100.0)));
                self.report.push(("working_set_after_hover_mb".into(), format!("{ws:.1}")));
                self.report.push(("private_bytes_after_hover_mb".into(), format!("{private:.1}")));
                self.on_leave();
                let json = format!(
                    "{{\n{}\n}}",
                    self.report.iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect::<Vec<_>>().join(",\n")
                );
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
    let x = (lp.0 & 0xFFFF) as i16 as f32;
    let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
    (x, y)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| d.on_move(x, y));
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            with_dock(|d| d.on_leave());
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| d.pressed = d.geo.hit(x, y, 1.0));
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_xy(lp);
            with_dock(|d| {
                let hit = d.geo.hit(x, y, 1.0);
                if hit.is_some() && hit == d.pressed {
                    d.on_click(hit.unwrap());
                }
                d.pressed = None;
            });
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            // Native styled menus come in Phase 1; a plain one is enough to quit the spike.
            unsafe {
                let menu = CreatePopupMenu().unwrap();
                let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, w!("Laminar Dock — Phase 0 spike"));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
                let _ = AppendMenuW(menu, MF_STRING, 1, w!("Quit Laminar"));
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                let _ = SetForegroundWindow(hwnd);
                let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON, p.x, p.y, None, hwnd, None);
                let _ = DestroyMenu(menu);
                if cmd.0 == 1 {
                    let _ = DestroyWindow(hwnd);
                }
            }
            return LRESULT(0);
        }
        WM_TIMER => {
            let id = wp.0;
            with_dock(|d| match id {
                T_SHRINK => unsafe {
                    let _ = KillTimer(Some(hwnd), T_SHRINK);
                    if !d.hovering {
                        d.set_region(false);
                    }
                },
                T_MEASURE => d.measure_tick(),
                T_SWEEP => d.sweep_tick(),
                T_EXIT => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            });
            return LRESULT(0);
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub fn run(opts: Options, started: Instant) -> Result<()> {
    // Private-bytes checkpoints so --measure can show what each stage costs.
    let mut stages: Vec<(&str, f64)> = vec![("runtime_ready", metrics::memory_mb().1)];
    let t_apps = Instant::now();
    let items = apps::pinned();
    let apps_ms = t_apps.elapsed().as_secs_f64() * 1000.0;
    stages.push(("after_shell_icons", metrics::memory_mb().1));

    unsafe {
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

        // Size the window to the bottom of the primary work area (above the Windows taskbar for now).
        let mut work = RECT::default();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0))?;
        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let win_w = (work.right - work.left) as f32;
        let geo = Geo::new(&items, scale, win_w);
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            work.left,
            work.bottom - geo.win_h as i32,
            geo.win_w as i32,
            geo.win_h as i32,
            SWP_NOACTIVATE,
        )?;
        // Show the (still empty, fully transparent) window right away, clipped to the
        // shelf so it never blocks clicks; content appears on the first commit.
        let idle = CreateRectRgn(
            geo.x0 as i32,
            geo.y0 as i32,
            (geo.x0 + geo.shelf_w) as i32,
            (geo.y0 + geo.shelf_h) as i32,
        );
        SetWindowRgn(hwnd, Some(idle), false);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let compositor = Compositor::new()?;
        let target = compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)?;
        let root = compositor.CreateContainerVisual()?;
        target.SetRoot(&root)?;
        stages.push(("after_compositor", metrics::memory_mb().1));
        let gfx = Gfx::new(&compositor)?;
        stages.push(("after_gpu_device", metrics::memory_mb().1));
        let ptr = compositor.CreatePropertySet()?;
        let mut dock = Dock {
            hwnd,
            opts: opts.clone(),
            started,
            dock: compositor.CreateContainerVisual()?,
            label: compositor.CreateContainerVisual()?,
            label_geom: compositor.CreateRoundedRectangleGeometry()?,
            label_text: compositor.CreateSpriteVisual()?,
            compositor,
            _target: target,
            gfx,
            ptr,
            root,
            slots: Vec::new(),
            tiles: Vec::new(),
            items,
            geo,
            scale,
            hovering: false,
            hover: None,
            pressed: None,
            measure_step: 0,
            m_cpu0: 0.0,
            m_t0: Instant::now(),
            report: Vec::new(),
            sweep_t0: Instant::now(),
            sweep_moves: 0,
        };
        dock.build()?;
        stages.push(("after_visual_tree", metrics::memory_mb().1));
        dock.set_region(false);
        let _ = DwmFlush(); // wait for the first composed frame
        let startup_ms = dock.started.elapsed().as_secs_f64() * 1000.0;

        if dock.opts.measure {
            let (ws, private) = metrics::memory_mb();
            dock.report.push(("startup_ms".into(), format!("{startup_ms:.0}")));
            dock.report.push(("app_and_icon_loading_ms".into(), format!("{apps_ms:.0}")));
            dock.report.push(("icons".into(), format!("{}", dock.items.iter().filter(|i| i.is_icon()).count())));
            dock.report.push(("dpi_scale".into(), format!("{scale:.2}")));
            dock.report.push(("cpu_cores".into(), format!("{}", metrics::cores())));
            dock.report.push(("working_set_at_start_mb".into(), format!("{ws:.1}")));
            dock.report.push(("private_bytes_at_start_mb".into(), format!("{private:.1}")));
            for (name, mb) in &stages {
                dock.report.push((format!("private_mb_{name}"), format!("{mb:.1}")));
            }
            SetTimer(Some(hwnd), T_MEASURE, 2_000, None);
        }
        if dock.opts.demo_hover {
            let g = &dock.geo;
            let (x, y) = (g.center_abs(4), g.y0 + g.shelf_h / 2.0);
            dock.on_move(x, y);
        }
        if let Some(secs) = dock.opts.exit_after {
            SetTimer(Some(hwnd), T_EXIT, secs * 1000, None);
        }
        DOCK.with(|d| *d.borrow_mut() = Some(dock));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        DOCK.with(|d| *d.borrow_mut() = None);
    }
    Ok(())
}
