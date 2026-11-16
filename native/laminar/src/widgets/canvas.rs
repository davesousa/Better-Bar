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

/// The system accents: (dark, light fill, light text). docs/widget-design.md §2.3.
/// Order: blue, green, red, orange, purple, yellow.
const ACCENTS: [(u32, u32, u32); 6] = [
    (0x0A84FF, 0x007AFF, 0x0062CC),
    (0x30D158, 0x34C759, 0x248A3D),
    (0xFF453A, 0xFF3B30, 0xD70015),
    (0xFF9F0A, 0xFF9500, 0xC45500),
    (0xBF5AF2, 0xAF52DE, 0x8944AB),
    (0xFFD60A, 0xFFCC00, 0xA05A00),
];

/// Line/area chart options (docs/widget-design.md §2.5).
#[derive(Clone, Copy, Debug)]
pub struct ChartOpts {
    /// Gradient fill under the line.
    pub fill: bool,
    /// End dot with a card-coloured ring (Medium/Large only).
    pub dot: bool,
    /// Line width: `style::STROKE` (≥ 40 tall) or `style::STROKE_SPARK`.
    pub stroke: f32,
    /// Catmull-Rom smoothing (use with ≥ 8 points).
    pub smooth: bool,
}

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
    /// Tabular figures, applied to every text layout.
    typo: Option<IDWriteTypography>,
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
        let typo = unsafe {
            dw.CreateTypography().ok().inspect(|t| {
                let _ = t.AddFontFeature(DWRITE_FONT_FEATURE { nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES, parameter: 1 });
            })
        };
        Self { dc, factory, dw, w, h, light, typo }
    }

    // ---------------------------------------------------------------- theme
    // Colour roles (docs/widget-design.md §2.3). Prefer these over `ink`.

    /// Neutral ink at an opacity (older layouts; new code uses the named roles below).
    pub fn ink(&self, a: f32) -> Col {
        if self.light {
            (0.08, 0.08, 0.11, a * 0.85)
        } else {
            (1.0, 1.0, 1.0, a)
        }
    }
    /// Primary text: values, titles, list labels.
    pub fn text1(&self) -> Col {
        if self.light {
            alpha(hex(0x1C1C1E), 1.0)
        } else {
            alpha(hex(0xFFFFFF), 0.95)
        }
    }
    /// Secondary text: captions, trailing values.
    pub fn text2(&self) -> Col {
        if self.light {
            alpha(hex(0x3C3C43), 0.72)
        } else {
            alpha(hex(0xEBEBF5), 0.64)
        }
    }
    /// Tertiary text: meta, axis ticks.
    pub fn text3(&self) -> Col {
        if self.light {
            alpha(hex(0x3C3C43), 0.46)
        } else {
            alpha(hex(0xEBEBF5), 0.38)
        }
    }
    /// Behind rings, meter bars and empty heat-map cells.
    pub fn track(&self) -> Col {
        if self.light {
            (0.0, 0.0, 0.0, 0.08)
        } else {
            (1.0, 1.0, 1.0, 0.12)
        }
    }
    /// Hairline separators.
    pub fn sep(&self) -> Col {
        if self.light {
            (0.0, 0.0, 0.0, 0.08)
        } else {
            (1.0, 1.0, 1.0, 0.10)
        }
    }
    /// Glyph tiles and secondary buttons.
    pub fn plate(&self) -> Col {
        if self.light {
            (0.0, 0.0, 0.0, 0.06)
        } else {
            (1.0, 1.0, 1.0, 0.12)
        }
    }
    /// The card fill behind a widget.
    pub fn card(&self) -> Col {
        if self.light {
            (1.0, 1.0, 1.0, 0.62)
        } else {
            (1.0, 1.0, 1.0, 0.07)
        }
    }
    /// The 1 px inner stroke around a card.
    pub fn card_edge(&self) -> Col {
        if self.light {
            (0.0, 0.0, 0.0, 0.06)
        } else {
            (1.0, 1.0, 1.0, 0.08)
        }
    }
    /// An opaque stand-in for the card colour (the ring around a chart's end dot).
    pub fn halo(&self) -> Col {
        if self.light {
            (0.97, 0.97, 0.98, 1.0)
        } else {
            (0.17, 0.17, 0.19, 1.0)
        }
    }

    // System accents. These return the dark values; drawing swaps them to the light fill
    // or light text variant in the light theme (see `tone`), so pass them straight through.
    pub fn blue(&self) -> Col {
        hex(ACCENTS[0].0)
    }
    #[allow(dead_code)]
    pub fn green(&self) -> Col {
        hex(ACCENTS[1].0)
    }
    #[allow(dead_code)]
    pub fn red(&self) -> Col {
        hex(ACCENTS[2].0)
    }
    #[allow(dead_code)]
    pub fn orange(&self) -> Col {
        hex(ACCENTS[3].0)
    }
    #[allow(dead_code)]
    pub fn purple(&self) -> Col {
        hex(ACCENTS[4].0)
    }
    #[allow(dead_code)]
    pub fn yellow(&self) -> Col {
        hex(ACCENTS[5].0)
    }

    /// Light theme: map a dark system accent to its light fill (`for_text` false) or light
    /// text variant (true), keeping its opacity. Every other colour passes through.
    pub fn tone(&self, c: Col, for_text: bool) -> Col {
        if !self.light {
            return c;
        }
        for (dark, fill, text) in ACCENTS {
            let d = hex(dark);
            if (c.0 - d.0).abs() < 0.003 && (c.1 - d.1).abs() < 0.003 && (c.2 - d.2).abs() < 0.003 {
                let n = hex(if for_text { text } else { fill });
                return (n.0, n.1, n.2, c.3);
            }
        }
        c
    }

    fn brush(&self, c: Col) -> Option<ID2D1SolidColorBrush> {
        self.brush_raw(self.tone(c, false))
    }
    fn brush_raw(&self, c: Col) -> Option<ID2D1SolidColorBrush> {
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
            let Some(b) = self.vgrad(y, y + h, top, bottom) else { return };
            let r = r.min(w / 2.0).min(h / 2.0);
            self.dc.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r }, &b);
        }
    }
    /// A vertical linear gradient brush from `top` (at y0) to `bottom` (at y1), toned.
    fn vgrad(&self, y0: f32, y1: f32, top: Col, bottom: Col) -> Option<ID2D1LinearGradientBrush> {
        unsafe {
            let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: d2c(self.tone(top, false)) }, D2D1_GRADIENT_STOP { position: 1.0, color: d2c(self.tone(bottom, false)) }];
            let rt: &ID2D1RenderTarget = self.dc;
            let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok()?;
            self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(0.0, y0), endPoint: pt(0.0, y1) }, None, &coll).ok()
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
        self.circle_stroke(cx, cy, r, self.track(), t);
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
        self.rr(x, y, w, h, h / 2.0, self.track());
        if frac > 0.0 {
            self.rr(x, y, (w * frac.clamp(0.0, 1.0)).max(h), h, h / 2.0, c);
        }
    }

    /// Line chart with a soft fill under it. `vals` oldest first. Stroke and smoothing
    /// follow the box: 2.0 when it is at least 40 tall, else 1.5; smooth with ≥ 8 points.
    pub fn area(&self, x: f32, y: f32, w: f32, h: f32, vals: &[f64], c: Col, fill: bool) {
        let stroke = if h >= 40.0 { super::style::STROKE } else { super::style::STROKE_SPARK };
        self.chart(x, y, w, h, vals, c, ChartOpts { fill, dot: false, stroke, smooth: vals.len() >= 8 });
    }

    /// Line/area chart (docs/widget-design.md §2.5): headroom above and below the data, a
    /// vertical gradient fill, optional Catmull-Rom smoothing and an end dot. All ink stays
    /// inside the box. `vals` oldest first.
    #[allow(clippy::too_many_arguments)]
    pub fn chart(&self, x: f32, y: f32, w: f32, h: f32, vals: &[f64], c: Col, o: ChartOpts) {
        use super::style::{CHART_DOT, CHART_PAD_HI, CHART_PAD_LO, FILL_DARK, FILL_LIGHT};
        let n = vals.len();
        if n < 2 || w <= 0.0 || h <= 0.0 {
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
        let first = lo;
        if (hi - lo).abs() < 1e-9 {
            hi = lo + 1.0;
            lo -= 1.0;
        }
        let range = hi - lo;
        let (lo, hi) = (lo - range * CHART_PAD_LO, hi + range * CHART_PAD_HI);
        // Keep the line's caps (and the dot) inside the box.
        let inset = o.stroke / 2.0;
        let right = if o.dot { CHART_DOT + 1.5 } else { inset };
        let (top, bottom) = (y + inset.max(if o.dot { CHART_DOT + 1.5 } else { 0.0 }), y + h - inset);
        let (x0, x1) = (x + inset, x + w - right);
        let pts: Vec<Vector2> = vals
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let v = if v.is_finite() { *v } else { first };
                pt(x0 + (x1 - x0) * i as f32 / (n - 1) as f32, bottom - ((v - lo) / (hi - lo)) as f32 * (bottom - top))
            })
            .collect();
        let trace = |s: &ID2D1GeometrySink| unsafe {
            if o.smooth && n >= 3 {
                for i in 0..n - 1 {
                    let p0 = pts[i.saturating_sub(1)];
                    let (p1, p2) = (pts[i], pts[i + 1]);
                    let p3 = pts[(i + 2).min(n - 1)];
                    let cy = |v: f32| v.clamp(top, bottom);
                    let c1 = pt(p1.X + (p2.X - p0.X) / 6.0, cy(p1.Y + (p2.Y - p0.Y) / 6.0));
                    let c2 = pt(p2.X - (p3.X - p1.X) / 6.0, cy(p2.Y - (p3.Y - p1.Y) / 6.0));
                    s.AddBezier(&D2D1_BEZIER_SEGMENT { point1: c1, point2: c2, point3: p2 });
                }
            } else {
                for p in pts.iter().skip(1) {
                    s.AddLine(*p);
                }
            }
        };
        unsafe {
            if o.fill {
                if let (Ok(g), Some(b)) = (self.factory.CreatePathGeometry(), self.vgrad(y, y + h, alpha(c, if self.light { FILL_LIGHT } else { FILL_DARK }), alpha(c, 0.0))) {
                    if let Ok(s) = g.Open() {
                        s.BeginFigure(pt(x0, y + h), D2D1_FIGURE_BEGIN_FILLED);
                        s.AddLine(pts[0]);
                        trace(&s);
                        s.AddLine(pt(x1, y + h));
                        s.EndFigure(D2D1_FIGURE_END_CLOSED);
                        let _ = s.Close();
                        self.dc.FillGeometry(&g, &b, None);
                    }
                }
            }
            if let Ok(g) = self.factory.CreatePathGeometry() {
                if let Ok(s) = g.Open() {
                    s.BeginFigure(pts[0], D2D1_FIGURE_BEGIN_HOLLOW);
                    trace(&s);
                    s.EndFigure(D2D1_FIGURE_END_OPEN);
                    let _ = s.Close();
                    if let Some(b) = self.brush(c) {
                        let st = self.round_style();
                        self.dc.DrawGeometry(&g, &b, o.stroke, st.as_ref());
                    }
                }
            }
        }
        if o.dot {
            let p = pts[n - 1];
            self.circle(p.X, p.Y, CHART_DOT + 1.5, self.halo());
            self.circle(p.X, p.Y, CHART_DOT, c);
        }
    }
    // ---------------------------------------------------------------- text
    /// Segoe UI Variable's optical size for a point size: Small under 12, Text to 20,
    /// Display from 20 (docs/widget-design.md §8.3). Icons keep Segoe Fluent Icons.
    pub fn family(size: f32, font: Font) -> &'static str {
        match font {
            Font::Icons => "Segoe Fluent Icons",
            _ if size < 12.0 => "Segoe UI Variable Small",
            _ if size < 20.0 => "Segoe UI Variable Text",
            _ => "Segoe UI Variable Display",
        }
    }

    fn format(&self, size: f32, weight: u32, font: Font, align: Align) -> Option<IDWriteTextFormat> {
        let family = Self::family(size, font);
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
        unsafe {
            let l = self.dw.CreateTextLayout(&wide, &f, max_w.max(1.0), size * 2.0).ok()?;
            // Tabular figures everywhere, so changing numbers don't jitter.
            if font != Font::Icons {
                if let Some(t) = &self.typo {
                    let _ = l.SetTypography(t, DWRITE_TEXT_RANGE { startPosition: 0, length: wide.len() as u32 });
                }
            }
            Some(l)
        }
    }

    /// (baseline, cap height) per em for a family, from its design metrics. Cached.
    fn font_ratios(&self, family: &'static str) -> (f32, f32) {
        thread_local! {
            static CACHE: std::cell::RefCell<Vec<(&'static str, (f32, f32))>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        if let Some(r) = CACHE.with(|c| c.borrow().iter().find(|(f, _)| *f == family).map(|(_, r)| *r)) {
            return r;
        }
        let r = self.measure_ratios(family).unwrap_or((1.064, 0.70));
        CACHE.with(|c| c.borrow_mut().push((family, r)));
        r
    }
    fn measure_ratios(&self, family: &'static str) -> Option<(f32, f32)> {
        unsafe {
            let mut coll = None;
            self.dw.GetSystemFontCollection(&mut coll, false).ok()?;
            let coll = coll?;
            let (mut idx, mut exists) = (0u32, windows::core::BOOL(0));
            coll.FindFamilyName(&HSTRING::from(family), &mut idx, &mut exists).ok()?;
            if !exists.as_bool() {
                return None;
            }
            let font = coll.GetFontFamily(idx).ok()?.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL).ok()?;
            let mut m = DWRITE_FONT_METRICS::default();
            font.GetMetrics(&mut m);
            let em = m.designUnitsPerEm.max(1) as f32;
            // Where the layout actually puts the baseline (line spacing included).
            let l = self.layout("0", 100.0, 400, Font::Text, Align::Left, 400.0)?;
            let mut lm = [DWRITE_LINE_METRICS::default()];
            let mut count = 0u32;
            let base = if l.GetLineMetrics(Some(&mut lm), &mut count).is_ok() && count > 0 { lm[0].baseline / 100.0 } else { m.ascent as f32 / em };
            Some((base, m.capHeight as f32 / em))
        }
    }
    /// Distance from a text box's top to its baseline at `size`.
    pub fn baseline(&self, size: f32) -> f32 {
        self.font_ratios(Self::family(size, Font::Text)).0 * size
    }
    /// Distance from a text box's top to the top of capitals (and figures) at `size`.
    pub fn cap_top(&self, size: f32) -> f32 {
        let (b, cap) = self.font_ratios(Self::family(size, Font::Text));
        (b - cap) * size
    }
    /// Cap height at `size`.
    pub fn cap_h(&self, size: f32) -> f32 {
        self.font_ratios(Self::family(size, Font::Text)).1 * size
    }

    /// Draw text by its baseline (the layouts in docs/widget-design.md are given this way).
    #[allow(clippy::too_many_arguments)]
    pub fn text_base(&self, s: &str, x: f32, baseline: f32, size: f32, weight: u32, c: Col, align: Align, max_w: f32) -> f32 {
        self.text(s, x, baseline - self.baseline(size), size, weight, c, align, max_w, Font::Text)
    }
    /// Draw text with the top of its capitals at `cap_top`.
    #[allow(clippy::too_many_arguments)]
    pub fn text_cap(&self, s: &str, x: f32, cap_top: f32, size: f32, weight: u32, c: Col, align: Align, max_w: f32) -> f32 {
        self.text(s, x, cap_top - self.cap_top(size), size, weight, c, align, max_w, Font::Text)
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
            if let Some(b) = self.brush_raw(self.tone(c, true)) {
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
        if let Some(b) = self.brush_raw(self.tone(c, true)) {
            unsafe { self.dc.DrawTextLayout(origin, &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE) };
        }
    }

    /// A service badge: rounded tile with a glyph or one or two letters. Brand colours are
    /// the same in both themes; near-black tiles get a faint edge in the dark theme.
    pub fn badge(&self, b: &Badge, x: f32, y: f32, size: f32) {
        let r = size * super::style::BADGE_R;
        if let Some(br) = self.brush_raw(b.bg) {
            unsafe { self.dc.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rc(x, y, size, size), radiusX: r, radiusY: r }, &br) };
        }
        let lum = 0.2126 * b.bg.0 + 0.7152 * b.bg.1 + 0.0722 * b.bg.2;
        if !self.light && lum < 0.16 {
            self.rr_stroke(x, y, size, size, r, (1.0, 1.0, 1.0, 0.14), 1.0);
        }
        if b.glyph {
            self.glyph(b.mark, x + size / 2.0, y + size / 2.0, size * 0.5, b.fg);
        } else {
            let fs = if b.mark.chars().count() > 1 { size * 0.38 } else { size * 0.52 };
            // Centre the capitals optically, not the text box.
            let top = y + size / 2.0 - self.cap_top(fs) - self.cap_h(fs) / 2.0;
            self.text(b.mark, x + size / 2.0, top, fs, 700, b.fg, Align::Center, size, Font::Display);
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
