//! A small Direct2D painter for widgets: rounded rectangles, text, rings, charts, badges.
//!
//! Every widget is one bitmap (a Composition drawing surface) painted with these helpers,
//! in logical pixels: the canvas applies the DPI scale, so layouts read like CSS.

use windows::core::{Interface, HSTRING};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Imaging::IWICBitmapSource;
use windows_numerics::{Matrix3x2, Vector2};

/// RGBA, 0..1.
pub type Col = (f32, f32, f32, f32);

pub const fn hex(rgb: u32) -> Col {
    (((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, 1.0)
}
pub fn alpha(c: Col, a: f32) -> Col {
    (c.0, c.1, c.2, c.3 * a)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Fonts: text, display (big numbers), icons (Segoe Fluent Icons).
#[derive(Clone, Copy, PartialEq)]
pub enum Font {
    Text,
    Display,
    Icons,
}

pub struct Canvas<'a> {
    pub dc: &'a ID2D1DeviceContext,
    pub factory: &'a ID2D1Factory1,
    pub dw: &'a IDWriteFactory,
    /// Logical size.
    pub w: f32,
    pub h: f32,
    /// Light theme (dark ink on pale glass).
    pub light: bool,
}

fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
fn rc(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h }
}
fn d2c(c: Col) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.0, g: c.1, b: c.2, a: c.3 }
}

impl<'a> Canvas<'a> {
    pub fn new(dc: &'a ID2D1DeviceContext, factory: &'a ID2D1Factory1, dw: &'a IDWriteFactory, w: f32, h: f32, scale: f32, light: bool) -> Self {
        let origin = unsafe {
            let mut m = Matrix3x2::default();
            dc.GetTransform(&mut m);
            m
        };
        unsafe {
            dc.SetTransform(&(Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 0.0 } * origin));
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        }
        Self { dc, factory, dw, w, h, light }
    }

    // ---------------------------------------------------------------- theme
    pub fn ink(&self, a: f32) -> Col {
        if self.light {
            (0.08, 0.08, 0.11, a * 0.85)
        } else {
            (1.0, 1.0, 1.0, a)
        }
    }
    pub fn text1(&self) -> Col {
        self.ink(if self.light { 1.0 } else { 0.96 })
    }
    pub fn text2(&self) -> Col {
        self.ink(0.62)
    }
    pub fn text3(&self) -> Col {
        self.ink(0.40)
    }
    /// The card fill behind a widget.
    pub fn card(&self) -> Col {
        if self.light {
            (1.0, 1.0, 1.0, 0.55)
        } else {
            (1.0, 1.0, 1.0, 0.065)
        }
    }

    fn brush(&self, c: Col) -> Option<ID2D1SolidColorBrush> {
        unsafe { self.dc.CreateSolidColorBrush(&d2c(c), None).ok() }
    }

    // ---------------------------------------------------------------- shapes
    pub fn rr(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Col) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(b) = self.brush(c) {
            let r = r.min(w / 2.0).min(h / 2.0);
            unsafe { self.dc.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r }, &b) };
        }
    }
    pub fn rr_stroke(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Col, t: f32) {
        if let Some(b) = self.brush(c) {
            let r = r.min(w / 2.0).min(h / 2.0);
            unsafe { self.dc.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rc(x + t / 2.0, y + t / 2.0, w - t, h - t), radiusX: r, radiusY: r }, &b, t, None) };
        }
    }
    /// Vertical gradient rounded rectangle.
    pub fn rr_grad(&self, x: f32, y: f32, w: f32, h: f32, r: f32, top: Col, bottom: Col) {
        unsafe {
            let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: d2c(top) }, D2D1_GRADIENT_STOP { position: 1.0, color: d2c(bottom) }];
            let rt: &ID2D1RenderTarget = self.dc;
            let Ok(coll) = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let Ok(b) = self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(x, y), endPoint: pt(x, y + h) }, None, &coll) else { return };
            let r = r.min(w / 2.0).min(h / 2.0);
            self.dc.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r }, &b);
        }
    }
    pub fn circle(&self, cx: f32, cy: f32, r: f32, c: Col) {
        if let Some(b) = self.brush(c) {
            unsafe { self.dc.FillEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r }, &b) };
        }
    }
    pub fn circle_stroke(&self, cx: f32, cy: f32, r: f32, c: Col, t: f32) {
        if let Some(b) = self.brush(c) {
            unsafe { self.dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r }, &b, t, None) };
        }
    }
    fn round_style(&self) -> Option<ID2D1StrokeStyle> {
        let f: &ID2D1Factory = self.factory;
        unsafe {
            f.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, lineJoin: D2D1_LINE_JOIN_ROUND, ..Default::default() },
                None,
            )
            .ok()
        }
    }
    pub fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, t: f32, c: Col) {
        if let Some(b) = self.brush(c) {
            let st = self.round_style();
            unsafe { self.dc.DrawLine(pt(x1, y1), pt(x2, y2), &b, t, st.as_ref()) };
        }
    }
    /// A progress ring: track + arc from 12 o'clock, clockwise.
    pub fn ring(&self, cx: f32, cy: f32, r: f32, t: f32, frac: f32, c: Col) {
        self.circle_stroke(cx, cy, r, self.ink(0.12), t);
        self.arc(cx, cy, r, t, -90.0, -90.0 + 360.0 * frac.clamp(0.0, 1.0), c);
    }
    /// Arc between angles (degrees, 0 = 3 o'clock, clockwise).
    pub fn arc(&self, cx: f32, cy: f32, r: f32, t: f32, a0: f32, a1: f32, c: Col) {
        if (a1 - a0).abs() < 0.5 {
            return;
        }
        if (a1 - a0).abs() >= 359.9 {
            self.circle_stroke(cx, cy, r, c, t);
            return;
        }
        unsafe {
            let Ok(g) = self.factory.CreatePathGeometry() else { return };
            let Ok(s) = g.Open() else { return };
            let p = |a: f32| pt(cx + r * a.to_radians().cos(), cy + r * a.to_radians().sin());
            s.BeginFigure(p(a0), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddArc(&D2D1_ARC_SEGMENT {
                point: p(a1),
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: if a1 > a0 { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                arcSize: if (a1 - a0).abs() > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            s.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = s.Close();
            if let Some(b) = self.brush(c) {
                let st = self.round_style();
                self.dc.DrawGeometry(&g, &b, t, st.as_ref());
            }
        }
    }
    /// Horizontal bar with a track.
    pub fn bar(&self, x: f32, y: f32, w: f32, h: f32, frac: f32, c: Col) {
        self.rr(x, y, w, h, h / 2.0, self.ink(0.12));
        if frac > 0.0 {
            self.rr(x, y, (w * frac.clamp(0.0, 1.0)).max(h), h, h / 2.0, c);
        }
    }

    /// Line chart with a soft fill under it. `vals` oldest first.
    pub fn area(&self, x: f32, y: f32, w: f32, h: f32, vals: &[f64], c: Col, fill: bool) {
        let n = vals.len();
        if n < 2 || w <= 0.0 {
            return;
        }
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for v in vals.iter().filter(|v| v.is_finite()) {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        if lo > hi {
            return;
        }
        if (hi - lo).abs() < 1e-9 {
            hi = lo + 1.0;
            lo -= 1.0;
        }
        let px = |i: usize| x + w * i as f32 / (n - 1) as f32;
        let py = |v: f64| y + h - ((v - lo) / (hi - lo)) as f32 * h;
        unsafe {
            if fill {
                if let Ok(g) = self.factory.CreatePathGeometry() {
                    if let Ok(s) = g.Open() {
                        s.BeginFigure(pt(px(0), y + h), D2D1_FIGURE_BEGIN_FILLED);
                        for (i, v) in vals.iter().enumerate() {
                            s.AddLine(pt(px(i), py(if v.is_finite() { *v } else { lo })));
                        }
                        s.AddLine(pt(px(n - 1), y + h));
                        s.EndFigure(D2D1_FIGURE_END_CLOSED);
                        let _ = s.Close();
                        let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: d2c(alpha(c, 0.32)) }, D2D1_GRADIENT_STOP { position: 1.0, color: d2c(alpha(c, 0.0)) }];
                        let rt: &ID2D1RenderTarget = self.dc;
                        if let Ok(coll) = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
                            if let Ok(b) = self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(x, y), endPoint: pt(x, y + h) }, None, &coll) {
                                self.dc.FillGeometry(&g, &b, None);
                            }
                        }
                    }
                }
            }
            if let Ok(g) = self.factory.CreatePathGeometry() {
                if let Ok(s) = g.Open() {
                    s.BeginFigure(pt(px(0), py(vals[0])), D2D1_FIGURE_BEGIN_HOLLOW);
                    for (i, v) in vals.iter().enumerate().skip(1) {
                        s.AddLine(pt(px(i), py(if v.is_finite() { *v } else { lo })));
                    }
                    s.EndFigure(D2D1_FIGURE_END_OPEN);
                    let _ = s.Close();
                    if let Some(b) = self.brush(c) {
                        let st = self.round_style();
                        self.dc.DrawGeometry(&g, &b, 1.6, st.as_ref());
                    }
                }
            }
        }
    }
    // ---------------------------------------------------------------- text
    fn format(&self, size: f32, weight: u32, font: Font, align: Align) -> Option<IDWriteTextFormat> {
        let family = match font {
            Font::Icons => "Segoe Fluent Icons",
            Font::Display => "Segoe UI Variable Display",
            Font::Text => "Segoe UI Variable Text",
        };
        unsafe {
            let f = self
                .dw
                .CreateTextFormat(&HSTRING::from(family), None, DWRITE_FONT_WEIGHT(weight as i32), DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, &HSTRING::from("en-us"))
                .ok()?;
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = f.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            if let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(&f) {
                let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
                let _ = f.SetTrimming(&trim, &sign);
            }
            Some(f)
        }
    }

    fn layout(&self, s: &str, size: f32, weight: u32, font: Font, align: Align, max_w: f32) -> Option<IDWriteTextLayout> {
        let f = self.format(size, weight, font, align)?;
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe { self.dw.CreateTextLayout(&wide, &f, max_w.max(1.0), size * 2.0).ok() }
    }

    /// Width and height of a string.
    pub fn measure(&self, s: &str, size: f32, weight: u32, font: Font) -> (f32, f32) {
        let Some(l) = self.layout(s, size, weight, font, Align::Left, 4000.0) else { return (0.0, size) };
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = l.GetMetrics(&mut m);
        }
        (m.widthIncludingTrailingWhitespace, m.height)
    }

    /// Draw text with its top at `y`. `x` is the left edge, centre or right edge per `align`.
    /// Trimmed with an ellipsis at `max_w`. Returns the drawn width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, weight: u32, c: Col, align: Align, max_w: f32, font: Font) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let mw = max_w.max(1.0);
        let Some(l) = self.layout(s, size, weight, font, align, mw) else { return 0.0 };
        let left = match align {
            Align::Left => x,
            Align::Center => x - mw / 2.0,
            Align::Right => x - mw,
        };
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = l.GetMetrics(&mut m);
            // Light cards: white emoji (clouds, snow, moons) vanish. The glyph drawn without
            // colour is its silhouette; a faint one just below gives them an edge.
            if self.light && has_emoji(s) {
                if let Some(sh) = self.brush((0.0, 0.0, 0.0, 0.16)) {
                    self.dc.DrawTextLayout(pt(left, y + (size * 0.035).max(0.75)), &l, &sh, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
            }
            if let Some(b) = self.brush(c) {
                self.dc.DrawTextLayout(pt(left, y), &l, &b, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
            }
        }
        m.width.min(mw)
    }
    /// Shorthands.
    pub fn tw(&self, s: &str, x: f32, y: f32, size: f32, weight: u32, c: Col, max_w: f32) -> f32 {
        self.text(s, x, y, size, weight, c, Align::Left, max_w, Font::Text)
    }
    pub fn tr(&self, s: &str, right: f32, y: f32, size: f32, weight: u32, c: Col) -> f32 {
        self.text(s, right, y, size, weight, c, Align::Right, right, Font::Text)
    }
    pub fn tc(&self, s: &str, cx: f32, y: f32, size: f32, weight: u32, c: Col, w: f32) -> f32 {
        self.text(s, cx, y, size, weight, c, Align::Center, w, Font::Text)
    }
    /// Big numbers.
    pub fn num(&self, s: &str, x: f32, y: f32, size: f32, weight: u32, c: Col) -> f32 {
        self.text(s, x, y, size, weight, c, Align::Left, self.w, Font::Display)
    }
    /// A Segoe Fluent Icons glyph centred on (cx, cy).
    pub fn glyph(&self, g: &str, cx: f32, cy: f32, size: f32, c: Col) {
        // Centre the glyph's ink (what you see), not its text box: icon fonts sit glyphs
        // at different heights in the line, which made icons look off-centre in circles.
        let Some(l) = self.layout(g, size, 400, Font::Icons, Align::Left, size * 3.0) else { return };
        let (mw, mh) = (size * 3.0, size * 2.0);
        let Ok(o) = (unsafe { l.GetOverhangMetrics() }) else { return };
        let (ink_l, ink_r, ink_t, ink_b) = (-o.left, mw + o.right, -o.top, mh + o.bottom);
        let origin = pt(cx - (ink_l + ink_r) / 2.0, cy - (ink_t + ink_b) / 2.0);
        if let Some(b) = self.brush(c) {
            unsafe { self.dc.DrawTextLayout(origin, &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE) };
        }
    }

    /// A service badge: rounded tile with a glyph or one or two letters.
    pub fn badge(&self, b: &Badge, x: f32, y: f32, size: f32) {
        let r = size * 0.26;
        self.rr(x, y, size, size, r, b.bg);
        if b.glyph {
            self.glyph(b.mark, x + size / 2.0, y + size / 2.0, size * 0.5, b.fg);
        } else {
            let fs = if b.mark.chars().count() > 1 { size * 0.38 } else { size * 0.52 };
            self.text(b.mark, x + size / 2.0, y + (size - fs * 1.33) / 2.0, fs, 700, b.fg, Align::Center, size, Font::Display);
        }
    }

    /// An image scaled to cover the rectangle, clipped to a rounded rectangle.
    pub fn image(&self, src: &IWICBitmapSource, x: f32, y: f32, w: f32, h: f32, r: f32) {
        unsafe {
            let Ok(bmp) = self.dc.CreateBitmapFromWicBitmap(src, None) else { return };
            let sz = bmp.GetSize();
            if sz.width <= 0.0 || sz.height <= 0.0 {
                return;
            }
            let k = (w / sz.width).max(h / sz.height);
            let (sw, sh) = (w / k, h / k);
            let srcr = rc((sz.width - sw) / 2.0, (sz.height - sh) / 2.0, sw, sh);
            let Ok(geo) = self.factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r }) else { return };
            let params = D2D1_LAYER_PARAMETERS1 {
                contentBounds: rc(x, y, w, h),
                geometricMask: std::mem::ManuallyDrop::new(Some(geo.cast().unwrap())),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            self.dc.PushLayer(&params, None);
            self.dc.DrawBitmap(&bmp, Some(&rc(x, y, w, h)), 1.0, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, Some(&srcr), None);
            self.dc.PopLayer();
            let _ = std::mem::ManuallyDrop::into_inner(params.geometricMask);
        }
    }
}

/// A service mark: a coloured tile with a glyph or letters (original marks, not logos).
#[derive(Clone, Copy, Debug)]
pub struct Badge {
    pub mark: &'static str,
    pub bg: Col,
    pub fg: Col,
    pub glyph: bool,
}

pub const fn mono(mark: &'static str, bg: u32, fg: u32) -> Badge {
    Badge { mark, bg: hex(bg), fg: hex(fg), glyph: false }
}
pub const fn gly(mark: &'static str, bg: u32, fg: u32) -> Badge {
    Badge { mark, bg: hex(bg), fg: hex(fg), glyph: true }
}

/// Does the string contain a pictograph (drawn from the colour emoji font)?
fn has_emoji(s: &str) -> bool {
    s.chars().any(|ch| matches!(ch as u32, 0x2600..=0x27BF | 0x1F300..=0x1FAFF))
}
