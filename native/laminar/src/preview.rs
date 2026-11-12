//! Live window previews shown when hovering a running app in the dock.
//!
//! The thumbnails are real DWM thumbnails (`DwmRegisterThumbnail`): the Windows
//! compositor draws a live, GPU-scaled copy of each window into our pop-up. No
//! pixels are copied into this process, so previews cost almost nothing.

use windows::core::{Interface, Result};
use windows::UI::Composition::*;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::UI::WindowsAndMessaging::IsIconic;

use crate::gfx::{Gfx, TextStyle};
use crate::tracker::Win;
use crate::ui::{self, rgba, theme, v3, Panel, Popup};

struct Thumb {
    hwnd: HWND,
    thumb: Option<isize>,
    /// Card rectangle in pop-up client pixels.
    card: (f32, f32, f32, f32),
    close: (f32, f32, f32, f32),
    highlight: ShapeVisual,
    close_bg: ShapeVisual,
}

pub struct Preview {
    pub popup: Popup,
    thumbs: Vec<Thumb>,
    hover: Option<usize>,
    hover_close: bool,
    pub open: bool,
    /// The dock item these previews belong to.
    pub key: String,
}

pub enum Hit {
    Card(HWND),
    Close(HWND),
}

impl Preview {
    pub fn new(c: &Compositor) -> Result<Self> {
        Ok(Self { popup: Popup::new(c, false)?, thumbs: Vec::new(), hover: None, hover_close: false, open: false, key: String::new() })
    }

    pub fn hwnd(&self) -> HWND {
        self.popup.hwnd
    }

    fn unregister(&mut self) {
        for t in &mut self.thumbs {
            if let Some(h) = t.thumb.take() {
                unsafe {
                    let _ = DwmUnregisterThumbnail(h);
                }
            }
        }
        self.thumbs.clear();
    }

    /// Show previews for `wins` above the icon centred at `icon_x` (screen px);
    /// `bottom` is the screen y the panel's bottom edge should sit at.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        c: &Compositor,
        gfx: &Gfx,
        backdrop: &CompositionBrush,
        key: &str,
        wins: &[Win],
        app_icon: Option<&CompositionSurfaceBrush>,
        icon_x: i32,
        bottom: i32,
        scale: f32,
        monitor: RECT,
    ) -> Result<()> {
        self.unregister();
        self.key = key.to_string();
        let s = scale;
        let wins: Vec<&Win> = wins.iter().take(6).collect();
        let pad = 10.0 * s;
        let gap = 8.0 * s;
        let card_w = (228.0 * s).round();
        let head_h = 26.0 * s;
        // Thumbnail heights follow each window's aspect ratio (within limits).
        let shot_h = (card_w - 12.0 * s) * 0.62;
        let card_h = (head_h + shot_h + 12.0 * s).round();
        let n = wins.len().max(1) as f32;
        let w = (pad * 2.0 + n * card_w + (n - 1.0) * gap).round();
        let h = (pad * 2.0 + card_h).round();

        let root = &self.popup.root;
        root.Children()?.RemoveAll()?;
        let panel: Panel = ui::panel(c, gfx, backdrop, w, h, 16.0 * s, theme::surface(0.70), s)?;
        let m = panel.margin;

        let (ww, wh) = ((w + 2.0 * m).ceil() as i32, (h + 2.0 * m).ceil() as i32);
        let left = icon_x - ww / 2;
        let top = bottom - wh + m as i32;
        let r = ui::clamp_rect(RECT { left, top, right: left + ww, bottom: top + wh }, monitor);

        for (i, win) in wins.iter().enumerate() {
            let x = pad + i as f32 * (card_w + gap);
            let y = pad;
            // Hover highlight behind the card.
            let (hl, _, _) = ui::rounded(c, card_w, card_h, 10.0 * s, theme::fill_2())?;
            hl.SetOffset(v3(x, y, 0.0))?;
            hl.SetOpacity(0.0)?;
            panel.content.Children()?.InsertAtTop(&hl)?;
            // Header: app icon + title + close button.
            if let Some(b) = app_icon {
                let ic = c.CreateSpriteVisual()?;
                ic.SetBrush(b)?;
                ic.SetSize(ui::v2(16.0 * s, 16.0 * s))?;
                ic.SetOffset(v3(x + 8.0 * s, y + (head_h - 16.0 * s) / 2.0 + 2.0 * s, 0.0))?;
                panel.content.Children()?.InsertAtTop(&ic)?;
            }
            let title = if win.title.is_empty() { "Window".to_string() } else { win.title.clone() };
            let (t, _tw, th) = ui::text(c, gfx, &title, &TextStyle { px: 12.0 * s, max_w: Some(card_w - 60.0 * s), ..Default::default() })?;
            t.SetOffset(v3(x + 30.0 * s, (y + (head_h - th) / 2.0 + 2.0 * s).round(), 0.0))?;
            panel.content.Children()?.InsertAtTop(&t)?;
            let cs = 20.0 * s;
            let close = (x + card_w - cs - 5.0 * s, y + (head_h - cs) / 2.0 + 2.0 * s, cs, cs);
            let (cb, _, _) = ui::rounded(c, cs, cs, cs / 2.0, rgba(232, 17, 35, 1.0))?;
            cb.SetOffset(v3(close.0, close.1, 0.0))?;
            cb.SetOpacity(0.0)?;
            panel.content.Children()?.InsertAtTop(&cb)?;
            let (xg, xw, xh) = ui::text(c, gfx, "\u{2715}", &TextStyle { px: 10.0 * s, color: theme::text(), ..Default::default() })?;
            xg.SetOffset(v3((close.0 + (cs - xw) / 2.0).round(), (close.1 + (cs - xh) / 2.0).round(), 0.0))?;
            xg.SetOpacity(0.75)?;
            panel.content.Children()?.InsertAtTop(&xg)?;

            // Thumbnail area (DWM draws the live window here).
            let sx = x + 6.0 * s;
            let sy = y + head_h + 4.0 * s;
            let sw = card_w - 12.0 * s;
            let (well, _, _) = ui::rounded(c, sw, shot_h, 6.0 * s, rgba(0, 0, 0, 0.18))?;
            well.SetOffset(v3(sx, sy, 0.0))?;
            panel.content.Children()?.InsertAtTop(&well)?;

            // A minimized window still has a preview: DWM keeps the last picture it drew, as
            // the Windows taskbar shows. As a fallback underneath (DWM draws its thumbnail on
            // top of our content), show the app icon and "Minimized" in case it has none.
            let minimized = unsafe { IsIconic(win.hwnd).as_bool() };
            if minimized {
                let isz = 40.0 * s;
                let (mt, mw, mh) = ui::text(c, gfx, "Minimized", &TextStyle { px: 12.0 * s, color: theme::text_3(), ..Default::default() })?;
                let top = sy + (shot_h - isz - 6.0 * s - mh) / 2.0;
                if let Some(b) = app_icon {
                    let ic = c.CreateSpriteVisual()?;
                    ic.SetBrush(b)?;
                    ic.SetSize(ui::v2(isz, isz))?;
                    ic.SetOffset(v3((sx + (sw - isz) / 2.0).round(), top.round(), 0.0))?;
                    panel.content.Children()?.InsertAtTop(&ic)?;
                }
                mt.SetOffset(v3((sx + (sw - mw) / 2.0).round(), (top + isz + 6.0 * s).round(), 0.0))?;
                panel.content.Children()?.InsertAtTop(&mt)?;
            }
            let mut thumb = None;
            if let Ok(h) = unsafe { DwmRegisterThumbnail(self.popup.hwnd, win.hwnd) } {
                // Fit the source into the well, keeping its aspect ratio.
                let src = unsafe { DwmQueryThumbnailSourceSize(h) }.unwrap_or_default();
                if minimized && (src.cx < 200 || src.cy < 100) {
                    // Only the minimized title-bar stub (no saved picture): keep the fallback.
                    unsafe {
                        let _ = DwmUnregisterThumbnail(h);
                    }
                    self.thumbs.push(Thumb {
                        hwnd: win.hwnd,
                        thumb: None,
                        card: (m + x, m + y, card_w, card_h),
                        close: (m + close.0, m + close.1, close.2, close.3),
                        highlight: hl,
                        close_bg: cb,
                    });
                    continue;
                }
                let (srcw, srch) = (src.cx.max(1) as f32, src.cy.max(1) as f32);
                let k = (sw / srcw).min(shot_h / srch);
                let (dw, dh) = (srcw * k, srch * k);
                let dx = m + sx + (sw - dw) / 2.0;
                let dy = m + sy + (shot_h - dh) / 2.0;
                let props = DWM_THUMBNAIL_PROPERTIES {
                    dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
                    rcDestination: RECT { left: dx.round() as i32, top: dy.round() as i32, right: (dx + dw).round() as i32, bottom: (dy + dh).round() as i32 },
                    opacity: 255,
                    fVisible: BOOL(1),
                    fSourceClientAreaOnly: BOOL(0),
                    ..Default::default()
                };
                unsafe {
                    let _ = DwmUpdateThumbnailProperties(h, &props);
                }
                thumb = Some(h);
            }
            self.thumbs.push(Thumb {
                hwnd: win.hwnd,
                thumb,
                card: (m + x, m + y, card_w, card_h),
                close: (m + close.0, m + close.1, close.2, close.3),
                highlight: hl,
                close_bg: cb,
            });
        }
        root.Children()?.InsertAtTop(&panel.root)?;
        // Rise in from just below.
        let body: CompositionObject = panel.body.cast()?;
        ui::animate_v3_from(c, &body, "Offset", v3(m, m + 8.0 * s, 0.0), v3(m, m, 0.0), 200)?;
        root.SetOpacity(0.0)?;
        ui::animate_scalar(c, &root.cast()?, "Opacity", 1.0, 140)?;
        self.popup.show(r, false);
        self.hover = None;
        self.open = true;
        Ok(())
    }

    pub fn hide(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.popup.hide();
        self.unregister();
    }

    fn hit(&self, x: f32, y: f32) -> Option<(usize, bool)> {
        let inside = |r: (f32, f32, f32, f32)| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3;
        self.thumbs.iter().position(|t| inside(t.card)).map(|i| (i, inside(self.thumbs[i].close)))
    }

    pub fn on_move(&mut self, c: &Compositor, x: f32, y: f32) {
        let h = self.hit(x, y);
        let (card, close) = (h.map(|v| v.0), h.map(|v| v.1).unwrap_or(false));
        if card != self.hover {
            if let Some(o) = self.hover.and_then(|i| self.thumbs.get(i)) {
                let _ = ui::animate_scalar(c, &o.highlight.cast().unwrap(), "Opacity", 0.0, 120);
                let _ = ui::animate_scalar(c, &o.close_bg.cast().unwrap(), "Opacity", 0.0, 80);
            }
            if let Some(n) = card.and_then(|i| self.thumbs.get(i)) {
                let _ = ui::animate_scalar(c, &n.highlight.cast().unwrap(), "Opacity", 1.0, 100);
            }
            self.hover = card;
            self.hover_close = false;
        }
        if close != self.hover_close {
            if let Some(t) = self.hover.and_then(|i| self.thumbs.get(i)) {
                let _ = ui::animate_scalar(c, &t.close_bg.cast().unwrap(), "Opacity", if close { 1.0 } else { 0.0 }, 80);
            }
            self.hover_close = close;
        }
    }

    /// The window whose card is at (x, y), close button included (for drags).
    pub fn window_at(&self, x: f32, y: f32) -> Option<HWND> {
        self.hit(x, y).map(|(i, _)| self.thumbs[i].hwnd)
    }

    pub fn on_click(&self, x: f32, y: f32) -> Option<Hit> {
        let (i, close) = self.hit(x, y)?;
        let h = self.thumbs[i].hwnd;
        Some(if close { Hit::Close(h) } else { Hit::Card(h) })
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.unregister();
    }
}
