//! Drawing: Direct2D/DirectWrite rendering into Composition drawing surfaces.
//!
//! Composition does the animating and blending on the GPU; Direct2D is only used
//! once per bitmap (icons, the rim, the logo, a label) to paint pixels into a surface.

use windows::core::{Interface, Result, HSTRING};
use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::UI::Composition::{CompositionDrawingSurface, CompositionGraphicsDevice, Compositor};
use windows::Win32::Foundation::{HMODULE, POINT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::WinRT::Composition::{ICompositionDrawingSurfaceInterop, ICompositorInterop};
use windows_numerics::{Matrix3x2, Vector2};

pub struct Gfx {
    pub factory: ID2D1Factory1,
    pub dwrite: IDWriteFactory,
    pub wic: IWICImagingFactory,
    pub graphics: CompositionGraphicsDevice,
}

pub fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}
fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

impl Gfx {
    pub fn new(compositor: &Compositor) -> Result<Self> {
        unsafe {
            let mut device: Option<ID3D11Device> = None;
            D3D11CreateDevice(
                None,
                driver_type(),
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
            let dxgi: IDXGIDevice = device.unwrap().cast()?;
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device = factory.CreateDevice(&dxgi)?;
            let graphics = compositor.cast::<ICompositorInterop>()?.CreateGraphicsDevice(&d2d_device)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            Ok(Self { factory, dwrite, wic, graphics })
        }
    }

    pub fn surface(&self, w: f32, h: f32) -> Result<CompositionDrawingSurface> {
        self.graphics.CreateDrawingSurface(
            Size { Width: w.ceil(), Height: h.ceil() },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )
    }

    /// Paint into a surface with Direct2D. The closure draws in surface-local pixels.
    pub fn draw(
        &self,
        surface: &CompositionDrawingSurface,
        f: impl FnOnce(&ID2D1DeviceContext) -> Result<()>,
    ) -> Result<()> {
        unsafe {
            let interop: ICompositionDrawingSurfaceInterop = surface.cast()?;
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = interop.BeginDraw(None, &mut offset)?;
            dc.SetTransform(&Matrix3x2::translation(offset.x as f32, offset.y as f32));
            dc.Clear(Some(&color(0.0, 0.0, 0.0, 0.0)));
            let r = f(&dc);
            interop.EndDraw()?;
            r
        }
    }

    /// An icon (premultiplied BGRA from the icon cache) scaled into a square surface with high-quality filtering.
    pub fn icon_surface(&self, src: &IWICBitmapSource, px: f32) -> Result<CompositionDrawingSurface> {
        let surface = self.surface(px, px)?;
        unsafe {
            self.draw(&surface, |dc| {
                let bmp = dc.CreateBitmapFromWicBitmap(src, None)?;
                dc.DrawBitmap(
                    &bmp,
                    Some(&rect(0.0, 0.0, px.ceil(), px.ceil())),
                    1.0,
                    D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                    None,
                    None,
                );
                Ok(())
            })?;
        }
        Ok(surface)
    }

    /// Superellipse ("squircle") outline, the same tile shape as the prototype.
    fn squircle(&self, x: f32, y: f32, size: f32) -> Result<ID2D1PathGeometry1> {
        unsafe {
            let geo = self.factory.CreatePathGeometry()?;
            let sink = geo.Open()?;
            let r = size / 2.0;
            let n = 5.0f32;
            let steps = 160;
            for i in 0..steps {
                let t = i as f32 / steps as f32 * std::f32::consts::TAU;
                let (s, c) = t.sin_cos();
                let px = x + r + r * c.signum() * c.abs().powf(2.0 / n);
                let py = y + r + r * s.signum() * s.abs().powf(2.0 / n);
                if i == 0 {
                    sink.BeginFigure(pt(px, py), D2D1_FIGURE_BEGIN_FILLED);
                } else {
                    sink.AddLine(pt(px, py));
                }
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            Ok(geo)
        }
    }

    fn stops(&self, dc: &ID2D1DeviceContext, stops: &[(f32, D2D1_COLOR_F)]) -> Result<ID2D1GradientStopCollection> {
        let s: Vec<D2D1_GRADIENT_STOP> = stops.iter().map(|(p, c)| D2D1_GRADIENT_STOP { position: *p, color: *c }).collect();
        let rt: &ID2D1RenderTarget = dc;
        unsafe { rt.CreateGradientStopCollection(&s, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) }
    }

    /// The Laminar mark: a gradient squircle with three flowing lines.
    pub fn logo_surface(&self, px: f32) -> Result<CompositionDrawingSurface> {
        let surface = self.surface(px, px)?;
        let tile = self.squircle(0.0, 0.0, px)?;
        self.draw(&surface, |dc| unsafe {
            let grad = self.stops(dc, &[
                (0.0, color(0.50, 0.66, 1.0, 1.0)),
                (0.5, color(0.42, 0.30, 0.96, 1.0)),
                (1.0, color(0.88, 0.35, 0.81, 1.0)),
            ])?;
            let fill = dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(0.0, 0.0), endPoint: pt(px, px) },
                None,
                &grad,
            )?;
            dc.FillGeometry(&tile, &fill, None);
            // gloss
            let gloss = self.stops(dc, &[(0.0, color(1.0, 1.0, 1.0, 0.28)), (0.5, color(1.0, 1.0, 1.0, 0.0))])?;
            let gb = dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(0.0, 0.0), endPoint: pt(0.0, px) },
                None,
                &gloss,
            )?;
            dc.FillGeometry(&tile, &gb, None);
            // three flow lines (same curves as the prototype, in a 100-unit box)
            let k = px / 100.0;
            let base: &ID2D1Factory = &self.factory;
            let cap = base.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    ..Default::default()
                },
                None,
            )?;
            for (y, a) in [(40.0, 0.98), (54.0, 0.78), (68.0, 0.55)] {
                let g = self.factory.CreatePathGeometry()?;
                let s = g.Open()?;
                s.BeginFigure(pt(20.0 * k, y * k), D2D1_FIGURE_BEGIN_HOLLOW);
                s.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(32.0 * k, (y - 10.0) * k), point2: pt(42.0 * k, (y + 8.0) * k), point3: pt(54.0 * k, (y - 2.0) * k) });
                s.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(66.0 * k, (y - 12.0) * k), point2: pt(74.0 * k, (y - 10.0) * k), point3: pt(80.0 * k, (y - 6.0) * k) });
                s.EndFigure(D2D1_FIGURE_END_OPEN);
                s.Close()?;
                let w = dc.CreateSolidColorBrush(&color(1.0, 1.0, 1.0, a), None)?;
                dc.DrawGeometry(&g, &w, 5.2 * k, &cap);
            }
            let rim = dc.CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 0.24), None)?;
            dc.DrawGeometry(&tile, &rim, 1.2 * k, None);
            Ok(())
        })?;
        Ok(surface)
    }

    /// The dock's "update ready" icon: a green-to-blue tile with a white download arrow.
    pub fn update_surface(&self, px: f32) -> Result<CompositionDrawingSurface> {
        let surface = self.surface(px, px)?;
        let tile = self.squircle(0.0, 0.0, px)?;
        self.draw(&surface, |dc| unsafe {
            let grad = self.stops(dc, &[(0.0, color(0.20, 0.80, 0.52, 1.0)), (1.0, color(0.12, 0.50, 0.95, 1.0))])?;
            let fill = dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(0.0, 0.0), endPoint: pt(px, px) },
                None,
                &grad,
            )?;
            dc.FillGeometry(&tile, &fill, None);
            let k = px / 100.0;
            let base: &ID2D1Factory = &self.factory;
            let cap = base.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    ..Default::default()
                },
                None,
            )?;
            let white = dc.CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 1.0), None)?;
            let g = self.factory.CreatePathGeometry()?;
            let s = g.Open()?;
            // Shaft, arrow head, and the tray it lands in.
            s.BeginFigure(pt(50.0 * k, 24.0 * k), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddLine(pt(50.0 * k, 62.0 * k));
            s.EndFigure(D2D1_FIGURE_END_OPEN);
            s.BeginFigure(pt(34.0 * k, 47.0 * k), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddLine(pt(50.0 * k, 63.0 * k));
            s.AddLine(pt(66.0 * k, 47.0 * k));
            s.EndFigure(D2D1_FIGURE_END_OPEN);
            s.BeginFigure(pt(29.0 * k, 66.0 * k), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddLine(pt(29.0 * k, 76.0 * k));
            s.AddLine(pt(71.0 * k, 76.0 * k));
            s.AddLine(pt(71.0 * k, 66.0 * k));
            s.EndFigure(D2D1_FIGURE_END_OPEN);
            s.Close()?;
            dc.DrawGeometry(&g, &white, 7.0 * k, &cap);
            let rim = dc.CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 0.24), None)?;
            dc.DrawGeometry(&tile, &rim, 1.2 * k, None);
            Ok(())
        })?;
        Ok(surface)
    }

    /// Specular rim for the glass shelf. Used as a nine-grid so the corners never stretch.
    pub fn rim_surface(&self, w: f32, h: f32, radius: f32, edge: f32) -> Result<CompositionDrawingSurface> {
        let surface = self.surface(w, h)?;
        self.draw(&surface, |dc| unsafe {
            let grad = self.stops(dc, &[
                (0.0, color(1.0, 1.0, 1.0, 0.95 * edge)),
                (0.35, color(1.0, 1.0, 1.0, 0.18 * edge)),
                (1.0, color(1.0, 1.0, 1.0, 0.40 * edge)),
            ])?;
            let b = dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(0.0, 0.0), endPoint: pt(0.0, h) },
                None,
                &grad,
            )?;
            let rr = D2D1_ROUNDED_RECT { rect: rect(0.5, 0.5, w - 0.5, h - 0.5), radiusX: radius - 0.5, radiusY: radius - 0.5 };
            dc.DrawRoundedRectangle(&rr, &b, 1.0, None);
            Ok(())
        })?;
        Ok(surface)
    }

    /// Text for a hover label. Returns the surface and its pixel size.
    pub fn text_surface(&self, text: &str, font_px: f32) -> Result<(CompositionDrawingSurface, f32, f32)> {
        self.text(text, &TextStyle { px: font_px, ..Default::default() })
    }

    /// Render one line (or a wrapped block) of text. Returns the surface and its pixel size.
    pub fn text(&self, text: &str, st: &TextStyle) -> Result<(CompositionDrawingSurface, f32, f32)> {
        unsafe {
            let family = if st.icons {
                "Segoe Fluent Icons"
            } else if st.display {
                "Segoe UI Variable Display"
            } else {
                "Segoe UI Variable Text"
            };
            let format = self.dwrite.CreateTextFormat(
                &HSTRING::from(family),
                None,
                DWRITE_FONT_WEIGHT(st.weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                st.px,
                &HSTRING::from("en-us"),
            )?;
            let max_w = st.max_w.unwrap_or(4000.0);
            if !st.wrap {
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                if st.max_w.is_some() {
                    // Trim with an ellipsis rather than overflowing.
                    let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
                    let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
                    format.SetTrimming(&trim, &sign)?;
                }
            }
            if st.center {
                format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            }
            let wide: Vec<u16> = text.encode_utf16().collect();
            let layout = self.dwrite.CreateTextLayout(&wide, &format, max_w, 2000.0)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            let w = if st.center || st.max_w.is_some() && m.widthIncludingTrailingWhitespace > max_w {
                max_w.min(m.widthIncludingTrailingWhitespace.max(if st.center { max_w } else { 0.0 }))
            } else {
                m.widthIncludingTrailingWhitespace
            };
            let (w, h) = (w.ceil().max(1.0) + 2.0, m.height.ceil() + 2.0);
            let surface = self.surface(w, h)?;
            let (r, g, b, a) = st.color;
            self.draw(&surface, |dc| {
                let brush = dc.CreateSolidColorBrush(&color(r, g, b, a), None)?;
                dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                if st.center {
                    // Centre within the surface width.
                    layout.SetMaxWidth(w - 2.0)?;
                }
                dc.DrawTextLayout(pt(1.0, 1.0), &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                Ok(())
            })?;
            Ok((surface, w, h))
        }
    }

    /// A soft drop shadow for a rounded panel, drawn as stacked translucent rounded
    /// rectangles (cheap, smooth). Used as a nine-grid: insets = radius + blur.
    pub fn shadow_surface(&self, radius: f32, blur: f32, alpha: f32) -> Result<CompositionDrawingSurface> {
        let size = ((radius + blur) * 2.0 + 4.0).ceil();
        let surface = self.surface(size, size)?;
        self.draw(&surface, |dc| unsafe {
            let steps = (blur.ceil() as i32).max(1);
            for i in 0..steps {
                // Outer layers are faint, inner layers stronger: approximates a Gaussian falloff.
                let k = i as f32 / steps as f32;
                let inset = blur * k;
                let a = alpha / steps as f32 * (0.4 + 1.6 * k);
                let b = dc.CreateSolidColorBrush(&color(0.0, 0.0, 0.0, a.min(1.0)), None)?;
                let r = radius + blur - inset;
                let rr = D2D1_ROUNDED_RECT { rect: rect(inset, inset, size - inset, size - inset), radiusX: r, radiusY: r };
                dc.FillRoundedRectangle(&rr, &b);
            }
            Ok(())
        })?;
        Ok(surface)
    }
}

/// How to render text.
#[derive(Clone, Debug)]
pub struct TextStyle {
    pub px: f32,
    /// 400 regular, 500 medium, 600 semibold, 700 bold, 300 light.
    pub weight: u32,
    pub color: (f32, f32, f32, f32),
    /// Trim (or wrap) at this width in pixels.
    pub max_w: Option<f32>,
    pub wrap: bool,
    pub center: bool,
    /// Use the Display optical size (large titles).
    pub display: bool,
    /// Render glyphs from the Segoe Fluent Icons font (Windows 11 system icons).
    pub icons: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { px: 13.0, weight: 400, color: crate::ui::theme::text(), max_w: None, wrap: false, center: false, display: false, icons: false }
    }
}

/// Which Direct3D device paints our bitmaps. The dock only paints a few bitmaps at
/// startup (the GPU compositor does all animation), so the software rasterizer
/// (WARP) can do it without loading the graphics driver into our process.
/// Set LAMINAR_GPU=hardware to compare.
fn driver_type() -> D3D_DRIVER_TYPE {
    match std::env::var("LAMINAR_GPU").as_deref() {
        Ok("hardware") => D3D_DRIVER_TYPE_HARDWARE,
        _ => D3D_DRIVER_TYPE_WARP,
    }
}
