//! A small UI toolkit on Windows Composition.
//!
//! Everything is a visual the GPU compositor owns: vector rounded rectangles
//! (ShapeVisual: crisp at any size, animatable), text rendered once into a
//! surface, glass panels (blurred desktop + tint) with a soft shadow, and
//! pop-up windows that host a visual tree. Rust only builds and moves visuals;
//! the compositor animates them.

use windows::core::{w, Interface, Result, HSTRING};
use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::*;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_EXCLUDED_FROM_PEEK, DWMWA_USE_HOSTBACKDROPBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

use crate::gfx::{Gfx, TextStyle};

pub fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
pub fn v3(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3 { X: x, Y: y, Z: z }
}
pub fn ms(n: i64) -> TimeSpan {
    TimeSpan { Duration: n * 10_000 }
}
pub fn hs(s: &str) -> HSTRING {
    HSTRING::from(s)
}
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color { A: (a.clamp(0.0, 1.0) * 255.0).round() as u8, R: r, G: g, B: b }
}

/// Colours shared by Laminar surfaces. Dark glass by default; light glass when Windows'
/// apps are set to light mode (and the "Match Windows light/dark" setting is on).
/// Everything is drawn from these, so switching is a redraw.
pub mod theme {
    use super::rgba;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::UI::Color;
    static LIGHT: AtomicBool = AtomicBool::new(false);
    pub fn set_light(on: bool) {
        LIGHT.store(on, Ordering::Relaxed);
    }
    pub fn light() -> bool {
        LIGHT.load(Ordering::Relaxed)
    }
    fn ink_rgb() -> (f32, f32, f32) {
        if light() {
            (0.07, 0.07, 0.09)
        } else {
            (0.96, 0.96, 0.97)
        }
    }
    /// Primary, secondary and tertiary text.
    pub fn text() -> (f32, f32, f32, f32) {
        let (r, g, b) = ink_rgb();
        (r, g, b, if light() { 0.92 } else { 1.0 })
    }
    pub fn text_2() -> (f32, f32, f32, f32) {
        let (r, g, b) = ink_rgb();
        (r, g, b, if light() { 0.62 } else { 0.62 })
    }
    pub fn text_3() -> (f32, f32, f32, f32) {
        let (r, g, b) = ink_rgb();
        (r, g, b, if light() { 0.45 } else { 0.40 })
    }
    /// White, for text and glyphs on accent-coloured fills in either theme.
    pub const WHITE: (f32, f32, f32, f32) = (1.0, 1.0, 1.0, 1.0);
    pub const DANGER: (f32, f32, f32, f32) = (1.0, 0.27, 0.23, 1.0);
    /// A translucent fill that contrasts with the glass (cards, tracks, hover): white on
    /// dark glass, near-black on light glass. `a` is the dark-theme opacity.
    pub fn ink(a: f32) -> Color {
        if light() {
            rgba(20, 20, 30, a * 0.6)
        } else {
            rgba(255, 255, 255, a)
        }
    }
    /// Glyph colour for marks drawn on a solid `ink(0.92)` fill (the volume bar).
    pub fn on_ink() -> (f32, f32, f32, f32) {
        if light() {
            (1.0, 1.0, 1.0, 0.95)
        } else {
            (0.1, 0.1, 0.12, 0.85)
        }
    }
    /// Glass tint for panels, menus and pop-ups.
    pub fn surface(a: f32) -> Color {
        if light() {
            rgba(246, 246, 250, (a + 0.08).min(1.0))
        } else {
            rgba(32, 32, 40, a)
        }
    }
    /// The tint over the whole screen behind Launchpad.
    pub fn scrim(a: f32) -> Color {
        if light() {
            rgba(236, 236, 242, a)
        } else {
            rgba(8, 8, 18, a)
        }
    }
    pub fn accent() -> Color {
        rgba(10, 132, 255, 1.0)
    }
    pub fn fill_2() -> Color {
        ink(0.14)
    }
    pub fn hair() -> Color {
        ink(0.10)
    }
}

/// A vector rounded rectangle (ShapeVisual). Returns the visual, its geometry and shape so
/// size and colour can be changed or animated later.
pub fn rounded(
    c: &Compositor,
    w: f32,
    h: f32,
    r: f32,
    color: Color,
) -> Result<(ShapeVisual, CompositionRoundedRectangleGeometry, CompositionSpriteShape)> {
    let sv = c.CreateShapeVisual()?;
    let geom = c.CreateRoundedRectangleGeometry()?;
    geom.SetSize(v2(w, h))?;
    geom.SetCornerRadius(v2(r, r))?;
    let shape = c.CreateSpriteShapeWithGeometry(&geom)?;
    shape.SetFillBrush(&c.CreateColorBrushWithColor(color)?)?;
    sv.Shapes()?.Append(&shape)?;
    sv.SetSize(v2(w, h))?;
    Ok((sv, geom, shape))
}

/// A sprite showing a surface at its natural pixel size.
pub fn surface_sprite(c: &Compositor, surface: &CompositionDrawingSurface, w: f32, h: f32) -> Result<SpriteVisual> {
    let s = c.CreateSpriteVisual()?;
    let b = c.CreateSurfaceBrushWithSurface(surface)?;
    b.SetStretch(CompositionStretch::None)?;
    b.SetHorizontalAlignmentRatio(0.0)?;
    b.SetVerticalAlignmentRatio(0.0)?;
    s.SetBrush(&b)?;
    s.SetSize(v2(w, h))?;
    Ok(s)
}

/// A text sprite. Returns (visual, width, height) in pixels.
pub fn text(c: &Compositor, gfx: &Gfx, s: &str, style: &TextStyle) -> Result<(SpriteVisual, f32, f32)> {
    let (surface, w, h) = gfx.text(s, style)?;
    Ok((surface_sprite(c, &surface, w, h)?, w, h))
}

/// Glass = the live, blurred desktop behind the window (HostBackdropBrush) + a tint,
/// clipped to a rounded rectangle. Size it with `container.SetSize` + `geom.SetSize`.
pub fn glass(
    c: &Compositor,
    backdrop: &CompositionBrush,
    tint: Color,
    radius: f32,
) -> Result<(ContainerVisual, CompositionRoundedRectangleGeometry)> {
    let cont = c.CreateContainerVisual()?;
    let blur = c.CreateSpriteVisual()?;
    blur.SetBrush(backdrop)?;
    blur.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
    let t = c.CreateSpriteVisual()?;
    t.SetBrush(&c.CreateColorBrushWithColor(tint)?)?;
    t.SetRelativeSizeAdjustment(v2(1.0, 1.0))?;
    cont.Children()?.InsertAtTop(&blur)?;
    cont.Children()?.InsertAtTop(&t)?;
    let geom = c.CreateRoundedRectangleGeometry()?;
    geom.SetCornerRadius(v2(radius, radius))?;
    cont.SetClip(&c.CreateGeometricClipWithGeometry(&geom)?)?;
    Ok((cont, geom))
}

/// A glass panel with a soft shadow and a hairline rim, as one container of size
/// (w + 2*margin, h + 2*margin); the panel itself sits at (margin, margin).
pub struct Panel {
    pub root: ContainerVisual,
    pub body: ContainerVisual,
    pub content: ContainerVisual,
    pub margin: f32,
    pub w: f32,
    pub h: f32,
}

pub fn panel(
    c: &Compositor,
    gfx: &Gfx,
    backdrop: &CompositionBrush,
    w: f32,
    h: f32,
    radius: f32,
    tint: Color,
    scale: f32,
) -> Result<Panel> {
    let blur = 22.0 * scale;
    let margin = (blur + 6.0 * scale).ceil();
    let root = c.CreateContainerVisual()?;
    root.SetSize(v2(w + 2.0 * margin, h + 2.0 * margin))?;
    // Shadow, slightly offset downwards.
    let sh = gfx.shadow_surface(radius, blur, 0.42)?;
    let sb = c.CreateSurfaceBrushWithSurface(&sh)?;
    sb.SetStretch(CompositionStretch::Fill)?;
    let nine = c.CreateNineGridBrush()?;
    nine.SetSource(&sb)?;
    nine.SetInsets(radius + blur + 2.0)?;
    let shadow = c.CreateSpriteVisual()?;
    shadow.SetBrush(&nine)?;
    shadow.SetSize(v2(w + 2.0 * blur, h + 2.0 * blur))?;
    shadow.SetOffset(v3(margin - blur, margin - blur + 6.0 * scale, 0.0))?;
    root.Children()?.InsertAtTop(&shadow)?;
    // Glass body
    let body = c.CreateContainerVisual()?;
    body.SetSize(v2(w, h))?;
    body.SetOffset(v3(margin, margin, 0.0))?;
    let (g, geom) = glass(c, backdrop, tint, radius)?;
    g.SetSize(v2(w, h))?;
    geom.SetSize(v2(w, h))?;
    body.Children()?.InsertAtTop(&g)?;
    // Hairline rim
    let (rim, _, rim_shape) = rounded(c, w, h, radius, rgba(0, 0, 0, 0.0))?;
    rim_shape.SetStrokeBrush(&c.CreateColorBrushWithColor(theme::ink(0.16))?)?;
    rim_shape.SetStrokeThickness(1.0)?;
    body.Children()?.InsertAtTop(&rim)?;
    let content = c.CreateContainerVisual()?;
    content.SetSize(v2(w, h))?;
    body.Children()?.InsertAtTop(&content)?;
    root.Children()?.InsertAtTop(&body)?;
    Ok(Panel { root, body, content, margin, w, h })
}

/// Animate a scalar property to `to` over `dur` ms with a smooth ease-out.
pub fn animate_scalar(c: &Compositor, obj: &CompositionObject, prop: &str, to: f32, dur: i64) -> Result<()> {
    let a = c.CreateScalarKeyFrameAnimation()?;
    let e = c.CreateCubicBezierEasingFunction(v2(0.2, 0.8), v2(0.2, 1.0))?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(ms(dur.max(1)))?;
    obj.StartAnimation(&hs(prop), &a)
}

/// Animate a Vector3 property (Offset, Scale) to `to` over `dur` ms with a smooth ease-out.
pub fn animate_v3(c: &Compositor, obj: &CompositionObject, prop: &str, to: Vector3, dur: i64) -> Result<()> {
    let a = c.CreateVector3KeyFrameAnimation()?;
    let e = c.CreateCubicBezierEasingFunction(v2(0.2, 0.8), v2(0.2, 1.0))?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(ms(dur.max(1)))?;
    obj.StartAnimation(&hs(prop), &a)
}

/// Same, from an explicit start value.
pub fn animate_v3_from(c: &Compositor, obj: &CompositionObject, prop: &str, from: Vector3, to: Vector3, dur: i64) -> Result<()> {
    let a = c.CreateVector3KeyFrameAnimation()?;
    let e = c.CreateCubicBezierEasingFunction(v2(0.2, 0.9), v2(0.2, 1.0))?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(ms(dur.max(1)))?;
    obj.StartAnimation(&hs(prop), &a)
}

/// A top-level pop-up window hosting its own Composition visual tree
/// (menus, previews, stacks, Launchpad, Settings).
pub struct Popup {
    pub hwnd: HWND,
    pub root: ContainerVisual,
    _target: DesktopWindowTarget,
}

/// Register the window class every pop-up uses, with the procedure that handles
/// their messages. Call once at startup, before creating pop-ups.
pub fn register_popup_class(proc_: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            lpszClassName: w!("LaminarPopup"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
    Ok(())
}

impl Popup {
    /// `activatable`: windows that take keyboard focus (menus, Launchpad, Settings).
    pub fn new(c: &Compositor, activatable: bool) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let mut ex = WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
            if !activatable {
                ex |= WS_EX_NOACTIVATE;
            }
            let hwnd = CreateWindowExW(ex, w!("LaminarPopup"), w!("Laminar"), WS_POPUP, 0, 0, 1, 1, None, None, Some(instance.into()), None)?;
            let on: i32 = 1;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, &on as *const _ as _, 4);
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_EXCLUDED_FROM_PEEK, &on as *const _ as _, 4);
            let target = c.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)?;
            let root = c.CreateContainerVisual()?;
            target.SetRoot(&root)?;
            Ok(Self { hwnd, root, _target: target })
        }
    }

    /// Move/resize (screen pixels) and show.
    pub fn show(&self, r: RECT, activate: bool) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                if activate { SWP_SHOWWINDOW } else { SWP_SHOWWINDOW | SWP_NOACTIVATE },
            );
            if activate {
                take_foreground(self.hwnd);
            }
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

/// Bring a window to the foreground even when Windows' foreground lock would refuse
/// (e.g. after a global hotkey): briefly attach to the current foreground thread's input.
pub fn take_foreground(hwnd: HWND) {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        if GetForegroundWindow() == hwnd {
            return;
        }
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        if fg_thread != 0 && fg_thread != me {
            let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg_thread, true);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg_thread, false);
        }
    }
}

/// Clamp a rectangle so it stays on the given monitor.
pub fn clamp_rect(mut r: RECT, m: RECT) -> RECT {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if r.right > m.right {
        r.left = m.right - w;
    }
    if r.left < m.left {
        r.left = m.left;
    }
    if r.bottom > m.bottom {
        r.top = m.bottom - h;
    }
    if r.top < m.top {
        r.top = m.top;
    }
    r.right = r.left + w;
    r.bottom = r.top + h;
    r
}
