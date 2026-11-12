//! Styled context menus: a glass panel with a soft shadow, an accent highlight
//! that glides between rows, check marks, shortcuts, separators and headers.
//! Mouse and keyboard (Up/Down/Enter/Esc). Closes when it loses focus.

use windows::core::{Interface, Result};
use windows::UI::Composition::*;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

use crate::gfx::{Gfx, TextStyle};
use crate::ui::{self, theme, v3, Popup};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Item,
    Separator,
    Header,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub kind: Kind,
    pub label: String,
    pub shortcut: Option<String>,
    pub cmd: usize,
    pub checked: bool,
    pub enabled: bool,
    pub danger: bool,
}

impl MenuItem {
    pub fn item(label: impl Into<String>, cmd: usize) -> Self {
        Self { kind: Kind::Item, label: label.into(), shortcut: None, cmd, checked: false, enabled: true, danger: false }
    }
    pub fn sep() -> Self {
        Self { kind: Kind::Separator, ..Self::item("", 0) }
    }
    pub fn header(label: impl Into<String>) -> Self {
        Self { kind: Kind::Header, ..Self::item(label, 0) }
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
}

struct Row {
    y: f32,
    h: f32,
    item: usize,
}

pub struct Menu {
    pub popup: Popup,
    items: Vec<MenuItem>,
    rows: Vec<Row>,
    highlight: Option<ShapeVisual>,
    hover: Option<usize>,
    origin: (f32, f32), // panel top-left inside the window
    panel_w: f32,
    scale: f32,
    restore_focus: HWND,
    pub open: bool,
}

impl Menu {
    pub fn new(c: &Compositor) -> Result<Self> {
        Ok(Self {
            popup: Popup::new(c, true)?,
            items: Vec::new(),
            rows: Vec::new(),
            highlight: None,
            hover: None,
            origin: (0.0, 0.0),
            panel_w: 0.0,
            scale: 1.0,
            restore_focus: HWND::default(),
            open: false,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.popup.hwnd
    }

    /// Show at `anchor` (screen px). `above`: open upwards from the anchor (dock menus).
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        c: &Compositor,
        gfx: &Gfx,
        backdrop: &CompositionBrush,
        items: Vec<MenuItem>,
        anchor: POINT,
        above: bool,
        scale: f32,
        monitor: RECT,
    ) -> Result<()> {
        let s = scale;
        self.scale = s;
        self.items = items;
        self.rows.clear();
        self.hover = None;
        let root = &self.popup.root;
        root.Children()?.RemoveAll()?;

        // Measure and render text first.
        let pad = 5.0 * s;
        let row_h = 26.0 * s;
        let sep_h = 9.0 * s;
        let head_h = 22.0 * s;
        let text_x = 26.0 * s;
        let mut labels = Vec::new();
        let mut max_w: f32 = 200.0 * s;
        for it in &self.items {
            match it.kind {
                Kind::Separator => labels.push(None),
                _ => {
                    let col = if !it.enabled || it.kind == Kind::Header {
                        theme::text_3()
                    } else if it.danger {
                        theme::DANGER
                    } else {
                        theme::text()
                    };
                    let weight = if it.kind == Kind::Header { 600 } else { 400 };
                    let px = if it.kind == Kind::Header { 11.5 } else { 13.0 } * s;
                    let (lab, lw, lh) = ui::text(c, gfx, &it.label, &TextStyle { px, weight, color: col, max_w: Some(300.0 * s), ..Default::default() })?;
                    let sc = match &it.shortcut {
                        Some(t) => Some(ui::text(c, gfx, t, &TextStyle { px: 12.0 * s, color: theme::text_3(), ..Default::default() })?),
                        None => None,
                    };
                    let sw = sc.as_ref().map(|x| x.1 + 24.0 * s).unwrap_or(0.0);
                    max_w = max_w.max(text_x + lw + sw + 14.0 * s);
                    labels.push(Some((lab, lw, lh, sc)));
                }
            }
        }
        let w = (max_w + 2.0 * pad).ceil();
        let mut y = pad;
        for (i, it) in self.items.iter().enumerate() {
            let h = match it.kind {
                Kind::Separator => sep_h,
                Kind::Header => head_h,
                Kind::Item => row_h,
            };
            self.rows.push(Row { y, h, item: i });
            y += h;
        }
        let h = (y + pad).ceil();

        let radius = 12.0 * s;
        let panel = ui::panel(c, gfx, backdrop, w, h, radius, theme::surface(0.74), s)?;
        let m = panel.margin;
        self.origin = (m, m);
        self.panel_w = w;

        // Hover highlight (moves between rows).
        let (hl, _, _) = ui::rounded(c, w - 2.0 * pad, row_h, 6.0 * s, theme::accent())?;
        hl.SetOpacity(0.0)?;
        hl.SetOffset(v3(pad, pad, 0.0))?;
        panel.content.Children()?.InsertAtTop(&hl)?;
        self.highlight = Some(hl);

        for (row, lab) in self.rows.iter().zip(labels) {
            let it = &self.items[row.item];
            match (it.kind, lab) {
                (Kind::Separator, _) => {
                    let (line, _, _) = ui::rounded(c, w - 2.0 * pad - 16.0 * s, 1.0, 0.0, theme::hair())?;
                    line.SetOffset(v3(pad + 8.0 * s, row.y + (row.h / 2.0).floor(), 0.0))?;
                    panel.content.Children()?.InsertAtTop(&line)?;
                }
                (_, Some((lab, _lw, lh, sc))) => {
                    let x = if it.kind == Kind::Header { pad + 10.0 * s } else { pad + text_x };
                    lab.SetOffset(v3(x, (row.y + (row.h - lh) / 2.0).round(), 0.0))?;
                    panel.content.Children()?.InsertAtTop(&lab)?;
                    if it.checked {
                        let (ck, _, ch) = ui::text(c, gfx, "\u{2713}", &TextStyle { px: 13.0 * s, weight: 600, ..Default::default() })?;
                        ck.SetOffset(v3(pad + 9.0 * s, (row.y + (row.h - ch) / 2.0).round(), 0.0))?;
                        panel.content.Children()?.InsertAtTop(&ck)?;
                    }
                    if let Some((sv, sw, sh)) = sc {
                        sv.SetOffset(v3(w - pad - 12.0 * s - sw, (row.y + (row.h - sh) / 2.0).round(), 0.0))?;
                        panel.content.Children()?.InsertAtTop(&sv)?;
                    }
                }
                _ => {}
            }
        }
        root.Children()?.InsertAtTop(&panel.root)?;

        // Position: next to the anchor, kept on screen.
        let (ww, wh) = ((w + 2.0 * m).ceil() as i32, (h + 2.0 * m).ceil() as i32);
        let left = anchor.x - m as i32;
        let top = if above { anchor.y - wh + m as i32 - (4.0 * s) as i32 } else { anchor.y - m as i32 };
        let r = ui::clamp_rect(RECT { left, top, right: left + ww, bottom: top + wh }, monitor);

        // Open animation: fade + slight scale from the anchor corner.
        let cx = (anchor.x - r.left) as f32 - m;
        let cy = if above { h } else { 0.0 };
        panel.body.SetCenterPoint(v3(cx, cy, 0.0))?;
        let body: CompositionObject = panel.body.cast()?;
        ui::animate_v3_from(c, &body, "Scale", v3(0.94, 0.94, 1.0), v3(1.0, 1.0, 1.0), 180)?;
        root.SetOpacity(0.0)?;
        ui::animate_scalar(c, &root.cast()?, "Opacity", 1.0, 110)?;

        self.restore_focus = unsafe { GetForegroundWindow() };
        self.popup.show(r, true);
        self.open = true;
        Ok(())
    }

    pub fn hide(&mut self, restore_focus: bool) {
        if !self.open {
            return;
        }
        self.open = false;
        self.popup.hide();
        if restore_focus && !self.restore_focus.is_invalid() {
            unsafe {
                let _ = SetForegroundWindow(self.restore_focus);
            }
        }
    }

    fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let (ox, oy) = self.origin;
        if x < ox || x > ox + self.panel_w {
            return None;
        }
        self.rows.iter().position(|r| y - oy >= r.y && y - oy < r.y + r.h)
    }

    fn selectable(&self, row: usize) -> bool {
        let it = &self.items[self.rows[row].item];
        it.kind == Kind::Item && it.enabled
    }

    fn set_hover(&mut self, c: &Compositor, row: Option<usize>) {
        let row = row.filter(|r| self.selectable(*r));
        if row == self.hover {
            return;
        }
        let first = self.hover.is_none();
        self.hover = row;
        let Some(hl) = &self.highlight else { return };
        let Ok(obj) = hl.cast::<CompositionObject>() else { return };
        match row {
            Some(r) => {
                let y = self.rows[r].y;
                let pad = 5.0 * self.scale;
                if first {
                    let _ = hl.SetOffset(v3(pad, y, 0.0));
                } else {
                    let _ = ui::animate_v3(c, &obj, "Offset", v3(pad, y, 0.0), 90);
                }
                let _ = ui::animate_scalar(c, &obj, "Opacity", 1.0, 60);
            }
            None => {
                let _ = ui::animate_scalar(c, &obj, "Opacity", 0.0, 90);
            }
        }
    }

    pub fn on_move(&mut self, c: &Compositor, x: f32, y: f32) {
        let r = self.row_at(x, y);
        self.set_hover(c, r);
    }

    /// A click: returns the command to run (the caller hides the menu).
    pub fn on_click(&mut self, x: f32, y: f32) -> Option<usize> {
        let r = self.row_at(x, y)?;
        if self.selectable(r) {
            Some(self.items[self.rows[r].item].cmd)
        } else {
            None
        }
    }

    /// Keyboard: returns Some(Some(cmd)) to run a command, Some(None) to close, None to keep open.
    pub fn on_key(&mut self, c: &Compositor, vk: u16) -> Option<Option<usize>> {
        let n = self.rows.len();
        let step = |from: Option<usize>, d: i32| -> Option<usize> {
            let mut i = from.map(|v| v as i32).unwrap_or(if d > 0 { -1 } else { n as i32 });
            for _ in 0..n {
                i = (i + d).rem_euclid(n as i32);
                if self.selectable(i as usize) {
                    return Some(i as usize);
                }
            }
            None
        };
        if vk == VK_DOWN.0 {
            let r = step(self.hover, 1);
            self.set_hover(c, r);
            None
        } else if vk == VK_UP.0 {
            let r = step(self.hover, -1);
            self.set_hover(c, r);
            None
        } else if vk == VK_RETURN.0 || vk == VK_SPACE.0 {
            self.hover.map(|r| Some(self.items[self.rows[r].item].cmd))
        } else if vk == VK_ESCAPE.0 {
            Some(None)
        } else {
            None
        }
    }
}
