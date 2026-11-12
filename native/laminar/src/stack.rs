//! Stack pop-up: a glass grid of the apps in a dock stack (folder).
//!
//! - Click an app to open it.
//! - Drag an app within the grid to reorder; the others glide aside to make room.
//! - Click the title (or choose **Rename Stack…** on the dock) to rename it inline:
//!   type, Enter saves, Esc cancels, clicking elsewhere saves.

use windows::core::{Interface, Result};
use windows::UI::Composition::*;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RETURN, VK_RIGHT};

use crate::gfx::{Gfx, TextStyle};
use crate::ui::{self, hs, ms, rgba, theme, v2, v3, Popup};

/// What the dock should do after a stack pop-up interaction.
pub enum StackEvent {
    Open(String),
    /// The stack's apps in their new order.
    Reorder(Vec<String>),
    Rename(String),
}

struct Cell {
    id: String,
    vis: ContainerVisual,
}

struct Drag {
    /// Index (into `cells`) of the app being dragged.
    from: usize,
    /// Grid position it would drop into.
    slot: usize,
    /// Pointer offset from the cell's top-left, in content coordinates.
    grab: (f32, f32),
}

struct Edit {
    text: String,
    /// The whole name is selected: typing replaces it.
    all: bool,
}

/// Grid layout in panel-content coordinates.
#[derive(Default, Clone, Copy)]
struct Grid {
    gx: f32,
    top: f32,
    cw: f32,
    ch: f32,
}

const MAX_NAME: usize = 40;

pub struct StackView {
    pub popup: Popup,
    cells: Vec<Cell>,
    highlight: Option<ShapeVisual>,
    hover: Option<usize>,
    pub open: bool,
    /// Index into cfg.pins of the stack shown.
    pub pin: usize,
    /// Panel margin (shadow room) inside the window; pointer coordinates minus this are
    /// panel-content coordinates.
    margin: f32,
    /// Visible panel size (content coordinates); the window is larger by the margin.
    panel_size: (f32, f32),
    scale: f32,
    grid: Grid,
    content: Option<ContainerVisual>,
    press: Option<(usize, f32, f32)>,
    drag: Option<Drag>,
    // Title / rename
    name: String,
    title_box: Option<ContainerVisual>,
    /// Where the title text starts (content coordinates) and how wide it may get.
    title_at: (f32, f32),
    title_max_w: f32,
    /// Clickable title area (content coordinates: x, y, w, h).
    title_rect: (f32, f32, f32, f32),
    edit: Option<Edit>,
}

pub struct StackApp<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub brush: Option<&'a CompositionSurfaceBrush>,
}

impl StackView {
    pub fn new(c: &Compositor) -> Result<Self> {
        Ok(Self {
            popup: Popup::new(c, true)?,
            cells: Vec::new(),
            highlight: None,
            hover: None,
            open: false,
            pin: usize::MAX,
            margin: 0.0,
            panel_size: (0.0, 0.0),
            scale: 1.0,
            grid: Grid::default(),
            content: None,
            press: None,
            drag: None,
            name: String::new(),
            title_box: None,
            title_at: (0.0, 0.0),
            title_max_w: 0.0,
            title_rect: (0.0, 0.0, 0.0, 0.0),
            edit: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.popup.hwnd
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        c: &Compositor,
        gfx: &Gfx,
        backdrop: &CompositionBrush,
        pin: usize,
        name: &str,
        apps: &[StackApp],
        icon_x: i32,
        bottom: i32,
        scale: f32,
        monitor: RECT,
    ) -> Result<()> {
        let s = scale;
        self.scale = s;
        self.pin = pin;
        self.cells.clear();
        self.hover = None;
        self.press = None;
        self.drag = None;
        self.edit = None;
        self.name = name.to_string();
        let n = apps.len().max(1);
        let cols = n.min(4);
        let rows = n.div_ceil(4);
        let (cw, ch) = ((92.0 * s).round(), (96.0 * s).round());
        let pad = 14.0 * s;
        let head = 40.0 * s;
        let w = (pad * 2.0 + cols as f32 * cw).max(240.0 * s).round();
        let h = (pad + head + rows as f32 * ch + pad * 0.6).round();

        let root = self.popup.root.clone();
        root.Children()?.RemoveAll()?;
        let panel = ui::panel(c, gfx, backdrop, w, h, 20.0 * s, theme::surface(0.70), s)?;
        let m = panel.margin;
        self.margin = m;
        self.panel_size = (w, h);
        self.content = Some(panel.content.clone());

        // Title (re-rendered in place while renaming).
        let title_box = c.CreateContainerVisual()?;
        panel.content.Children()?.InsertAtTop(&title_box)?;
        self.title_box = Some(title_box);
        self.title_max_w = w - 120.0 * s;
        self.title_at = (pad + 4.0 * s, 0.0);
        let th = gfx.text("Ag", &self.title_style(theme::text()))?.2;
        self.title_at.1 = (pad + (head - th) / 2.0 - 4.0 * s).round();
        self.render_title(c, gfx)?;

        let count = format!("{} {}", apps.len(), if apps.len() == 1 { "app" } else { "apps" });
        let (ct, cw2, cth) = ui::text(c, gfx, &count, &TextStyle { px: 12.0 * s, color: theme::text_3(), ..Default::default() })?;
        ct.SetOffset(v3(w - pad - cw2 - 4.0 * s, pad + (head - cth) / 2.0 - 4.0 * s, 0.0))?;
        panel.content.Children()?.InsertAtTop(&ct)?;

        self.grid = Grid { gx: (w - cols as f32 * cw) / 2.0, top: pad + head, cw, ch };
        let (hl, _, _) = ui::rounded(c, cw - 6.0 * s, ch - 6.0 * s, 14.0 * s, theme::fill_2())?;
        hl.SetOpacity(0.0)?;
        panel.content.Children()?.InsertAtTop(&hl)?;
        self.highlight = Some(hl);

        if apps.is_empty() {
            let (t, tw, _) = ui::text(c, gfx, "Drag apps onto this stack to add them.", &TextStyle { px: 12.5 * s, color: theme::text_3(), ..Default::default() })?;
            t.SetOffset(v3(((w - tw) / 2.0).round(), pad + head + 20.0 * s, 0.0))?;
            panel.content.Children()?.InsertAtTop(&t)?;
        }
        let icon = (54.0 * s).round();
        for (k, a) in apps.iter().enumerate() {
            // Each app is one container (icon + label) so it can glide as a unit.
            let vis = c.CreateContainerVisual()?;
            vis.SetSize(v2(cw, ch))?;
            vis.SetCenterPoint(v3(cw / 2.0, 6.0 * s + icon / 2.0, 0.0))?;
            let (x, y) = self.slot_pos(k);
            vis.SetOffset(v3(x, y, 0.0))?;
            if let Some(b) = a.brush {
                let sp = c.CreateSpriteVisual()?;
                sp.SetBrush(b)?;
                sp.SetSize(v2(icon, icon))?;
                sp.SetOffset(v3(((cw - icon) / 2.0).round(), 6.0 * s, 0.0))?;
                vis.Children()?.InsertAtTop(&sp)?;
            }
            let (lab, lw, _) = ui::text(c, gfx, a.name, &TextStyle { px: 11.5 * s, max_w: Some(cw - 8.0 * s), ..Default::default() })?;
            lab.SetOffset(v3(((cw - lw) / 2.0).round(), 6.0 * s + icon + 6.0 * s, 0.0))?;
            vis.Children()?.InsertAtTop(&lab)?;
            panel.content.Children()?.InsertAtTop(&vis)?;
            self.cells.push(Cell { id: a.id.to_string(), vis });
        }
        root.Children()?.InsertAtTop(&panel.root)?;

        let (ww, wh) = ((w + 2.0 * m).ceil() as i32, (h + 2.0 * m).ceil() as i32);
        let left = icon_x - ww / 2;
        let top = bottom - wh + m as i32;
        let r = ui::clamp_rect(RECT { left, top, right: left + ww, bottom: top + wh }, monitor);
        // Spring up out of the dock.
        let cx = (icon_x - r.left) as f32 - m;
        panel.body.SetCenterPoint(v3(cx, h, 0.0))?;
        let body: CompositionObject = panel.body.cast()?;
        ui::animate_v3_from(c, &body, "Scale", v3(0.85, 0.85, 1.0), v3(1.0, 1.0, 1.0), 260)?;
        root.SetOpacity(0.0)?;
        ui::animate_scalar(c, &root.cast()?, "Opacity", 1.0, 140)?;
        self.popup.show(r, true);
        self.open = true;
        Ok(())
    }

    pub fn hide(&mut self) {
        if self.open {
            self.open = false;
            self.press = None;
            self.drag = None;
            self.edit = None;
            self.popup.hide();
        }
    }

    // ------------------------------------------------------------ grid

    /// Top-left of grid position k (content coordinates).
    fn slot_pos(&self, k: usize) -> (f32, f32) {
        let g = self.grid;
        (g.gx + (k % 4) as f32 * g.cw, g.top + (k / 4) as f32 * g.ch)
    }

    /// Grid position under a content-coordinate point, if it's on an app.
    fn cell_at(&self, x: f32, y: f32) -> Option<usize> {
        (0..self.cells.len()).find(|&k| {
            let (cx, cy) = self.slot_pos(k);
            x >= cx && x < cx + self.grid.cw && y >= cy && y < cy + self.grid.ch
        })
    }

    /// Nearest grid position to a content-coordinate point (for dropping).
    fn nearest_slot(&self, x: f32, y: f32) -> usize {
        let g = self.grid;
        let n = self.cells.len();
        let cols = n.min(4).max(1) as i32;
        let rows = n.div_ceil(4).max(1) as i32;
        let col = (((x - g.gx) / g.cw).floor() as i32).clamp(0, cols - 1);
        let row = (((y - g.top) / g.ch).floor() as i32).clamp(0, rows - 1);
        ((row * 4 + col) as usize).min(n.saturating_sub(1))
    }

    /// True if a window point is in the transparent shadow margin, not on the panel.
    /// A click there is a click outside the stack (it looks like the dock or desktop).
    pub fn outside(&self, x: f32, y: f32) -> bool {
        let (cx, cy) = self.to_content(x, y);
        cx < 0.0 || cy < 0.0 || cx >= self.panel_size.0 || cy >= self.panel_size.1
    }

    fn to_content(&self, x: f32, y: f32) -> (f32, f32) {
        (x - self.margin, y - self.margin)
    }

    fn set_highlight(&mut self, c: &Compositor, h: Option<usize>) {
        if h == self.hover {
            return;
        }
        self.hover = h;
        if let Some(hl) = &self.highlight {
            let o: CompositionObject = hl.cast().unwrap();
            match h {
                Some(k) => {
                    let (x, y) = self.slot_pos(k);
                    let _ = hl.SetOffset(v3(x + 3.0 * self.scale, y + 3.0 * self.scale, 0.0));
                    let _ = ui::animate_scalar(c, &o, "Opacity", 1.0, 80);
                }
                None => {
                    let _ = ui::animate_scalar(c, &o, "Opacity", 0.0, 100);
                }
            }
        }
    }

    /// Glide every app to its grid position for `order` (indices into `cells`), except
    /// the one being dragged.
    fn layout(&self, c: &Compositor, order: &[usize], skip: Option<usize>) {
        for (pos, &idx) in order.iter().enumerate() {
            if Some(idx) == skip {
                continue;
            }
            let (x, y) = self.slot_pos(pos);
            if let Ok(o) = self.cells[idx].vis.cast::<CompositionObject>() {
                let _ = ui::animate_v3(c, &o, "Offset", v3(x, y, 0.0), 240);
            }
        }
    }

    fn order_for(&self, d: &Drag) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.cells.len()).filter(|&k| k != d.from).collect();
        order.insert(d.slot.min(order.len()), d.from);
        order
    }

    // ------------------------------------------------------------ pointer

    /// Button down (window coordinates). Clicking the title starts a rename; clicking
    /// anywhere while renaming saves it first.
    pub fn on_down(&mut self, c: &Compositor, gfx: &Gfx, x: f32, y: f32) -> Option<StackEvent> {
        let (cx, cy) = self.to_content(x, y);
        let r = self.title_rect;
        let on_title = cx >= r.0 && cx < r.0 + r.2 && cy >= r.1 && cy < r.1 + r.3;
        if on_title {
            if self.edit.is_none() {
                self.begin_rename(c, gfx);
            }
            return None;
        }
        let ev = self.commit_rename(c, gfx);
        self.press = self.cell_at(cx, cy).map(|k| (k, cx, cy));
        ev
    }

    pub fn on_move(&mut self, c: &Compositor, x: f32, y: f32) {
        let (cx, cy) = self.to_content(x, y);
        if let Some((k, px, py)) = self.press {
            if self.drag.is_none() && ((cx - px).powi(2) + (cy - py).powi(2)).sqrt() > 6.0 * self.scale && self.cells.len() > 1 {
                // Lift the app out of the grid.
                let (sx, sy) = self.slot_pos(k);
                self.drag = Some(Drag { from: k, slot: k, grab: (px - sx, py - sy) });
                self.set_highlight(c, None);
                let vis = &self.cells[k].vis;
                let _ = vis.StopAnimation(&hs("Offset"));
                if let Some(content) = &self.content {
                    if let Ok(ch) = content.Children() {
                        let _ = ch.Remove(vis);
                        let _ = ch.InsertAtTop(vis);
                    }
                }
                if let Ok(o) = vis.cast::<CompositionObject>() {
                    let _ = ui::animate_v3(c, &o, "Scale", v3(1.1, 1.1, 1.0), 140);
                    let _ = ui::animate_scalar(c, &o, "Opacity", 0.92, 140);
                }
            }
        }
        if let Some(d) = &self.drag {
            let (from, grab, old_slot) = (d.from, d.grab, d.slot);
            let _ = self.cells[from].vis.SetOffset(v3(cx - grab.0, cy - grab.1, 0.0));
            // Drop position follows the centre of the dragged icon, not the pointer.
            let (mx, my) = (cx - grab.0 + self.grid.cw / 2.0, cy - grab.1 + self.grid.ch / 2.0);
            let slot = self.nearest_slot(mx, my);
            if slot != old_slot {
                if let Some(d) = self.drag.as_mut() {
                    d.slot = slot;
                }
                let d = self.drag.as_ref().unwrap();
                let order = self.order_for(d);
                self.layout(c, &order, Some(from));
            }
            return;
        }
        let h = self.cell_at(cx, cy);
        self.set_highlight(c, h);
    }

    pub fn on_up(&mut self, c: &Compositor, x: f32, y: f32) -> Option<StackEvent> {
        let (cx, cy) = self.to_content(x, y);
        let press = self.press.take();
        if let Some(d) = self.drag.take() {
            let order = self.order_for(&d);
            let (sx, sy) = self.slot_pos(d.slot);
            if let Ok(o) = self.cells[d.from].vis.cast::<CompositionObject>() {
                let _ = ui::animate_v3(c, &o, "Offset", v3(sx, sy, 0.0), 220);
                let _ = ui::animate_v3(c, &o, "Scale", v3(1.0, 1.0, 1.0), 220);
                let _ = ui::animate_scalar(c, &o, "Opacity", 1.0, 160);
            }
            if d.slot == d.from {
                return None;
            }
            let mut old: Vec<Option<Cell>> = std::mem::take(&mut self.cells).into_iter().map(Some).collect();
            self.cells = order.iter().map(|&k| old[k].take().unwrap()).collect();
            return Some(StackEvent::Reorder(self.cells.iter().map(|c| c.id.clone()).collect()));
        }
        match (press, self.cell_at(cx, cy)) {
            (Some((k, _, _)), Some(j)) if k == j => Some(StackEvent::Open(self.cells[k].id.clone())),
            _ => None,
        }
    }

    /// Mouse capture lost mid-drag: put everything back.
    pub fn cancel_drag(&mut self, c: &Compositor) {
        self.press = None;
        if let Some(d) = self.drag.take() {
            let order: Vec<usize> = (0..self.cells.len()).collect();
            self.layout(c, &order, None);
            if let Ok(o) = self.cells[d.from].vis.cast::<CompositionObject>() {
                let _ = ui::animate_v3(c, &o, "Scale", v3(1.0, 1.0, 1.0), 200);
                let _ = ui::animate_scalar(c, &o, "Opacity", 1.0, 160);
            }
        }
    }

    // ------------------------------------------------------------ rename

    fn title_style(&self, color: (f32, f32, f32, f32)) -> TextStyle {
        TextStyle { px: 15.0 * self.scale, weight: 600, display: true, max_w: Some(self.title_max_w), color, ..Default::default() }
    }

    /// Start renaming with the whole name selected (typing replaces it).
    pub fn begin_rename(&mut self, c: &Compositor, gfx: &Gfx) {
        self.edit = Some(Edit { text: self.name.clone(), all: true });
        let _ = self.render_title(c, gfx);
    }

    /// Finish renaming. Returns the new name if it changed (blank keeps the old one).
    pub fn commit_rename(&mut self, c: &Compositor, gfx: &Gfx) -> Option<StackEvent> {
        let e = self.edit.take()?;
        let name = e.text.trim().to_string();
        let changed = !name.is_empty() && name != self.name;
        if changed {
            self.name = name.clone();
        }
        let _ = self.render_title(c, gfx);
        changed.then_some(StackEvent::Rename(name))
    }

    pub fn key_char(&mut self, c: &Compositor, gfx: &Gfx, ch: char) {
        let Some(e) = self.edit.as_mut() else { return };
        if ch == '\u{1}' {
            e.all = true; // Ctrl+A
        } else if ch.is_control() {
            return;
        } else {
            if e.all {
                e.text.clear();
                e.all = false;
            }
            if e.text.chars().count() < MAX_NAME {
                e.text.push(ch);
            }
        }
        let _ = self.render_title(c, gfx);
    }

    /// Keys while the pop-up has focus. Returns (handled, event).
    pub fn key_down(&mut self, c: &Compositor, gfx: &Gfx, vk: u16) -> (bool, Option<StackEvent>) {
        if self.edit.is_none() {
            return (false, None);
        }
        if vk == VK_RETURN.0 {
            return (true, self.commit_rename(c, gfx));
        }
        if vk == VK_ESCAPE.0 {
            self.edit = None;
            let _ = self.render_title(c, gfx);
            return (true, None);
        }
        let e = self.edit.as_mut().unwrap();
        if vk == VK_BACK.0 {
            if e.all {
                e.text.clear();
                e.all = false;
            } else {
                e.text.pop();
            }
        } else if [VK_LEFT.0, VK_RIGHT.0, VK_HOME.0, VK_END.0].contains(&vk) {
            e.all = false; // caret to the end (the only caret position)
        } else {
            return (true, None);
        }
        let _ = self.render_title(c, gfx);
        (true, None)
    }

    /// Draw the title: plain text, or a text field with selection or a blinking caret.
    fn render_title(&mut self, c: &Compositor, gfx: &Gfx) -> Result<()> {
        let Some(bx) = self.title_box.clone() else { return Ok(()) };
        bx.Children()?.RemoveAll()?;
        let s = self.scale;
        let (tx, ty) = self.title_at;
        let th = gfx.text("Ag", &self.title_style(theme::text()))?.2;
        let (text, color) = match &self.edit {
            Some(e) if e.text.is_empty() => ("Stack Name".to_string(), theme::text_3()),
            Some(e) => (e.text.clone(), theme::text()),
            None => (self.name.clone(), theme::text()),
        };
        let (label, tw, _) = ui::text(c, gfx, &text, &self.title_style(color))?;
        label.SetOffset(v3(tx, ty, 0.0))?;
        match &self.edit {
            None => {
                bx.Children()?.InsertAtTop(&label)?;
                self.title_rect = (tx - 6.0 * s, ty - 4.0 * s, tw + 12.0 * s, th + 8.0 * s);
            }
            Some(e) => {
                let (fw, fh) = (self.title_max_w + 12.0 * s, th + 8.0 * s);
                let (field, _, _) = ui::rounded(c, fw, fh, 7.0 * s, theme::fill_2())?;
                field.SetOffset(v3(tx - 6.0 * s, ty - 4.0 * s, 0.0))?;
                bx.Children()?.InsertAtTop(&field)?;
                let (ring, _, ring_shape) = ui::rounded(c, fw, fh, 7.0 * s, rgba(0, 0, 0, 0.0))?;
                ring_shape.SetStrokeBrush(&c.CreateColorBrushWithColor(theme::accent())?)?;
                ring_shape.SetStrokeThickness(1.5 * s)?;
                ring.SetOffset(v3(tx - 6.0 * s, ty - 4.0 * s, 0.0))?;
                bx.Children()?.InsertAtTop(&ring)?;
                let empty = e.text.is_empty();
                if e.all && !empty {
                    let a = theme::accent();
                    let (sel, _, _) = ui::rounded(c, tw + 4.0 * s, th + 2.0 * s, 3.0 * s, rgba(a.R, a.G, a.B, 0.55))?;
                    sel.SetOffset(v3(tx - 2.0 * s, ty - 1.0 * s, 0.0))?;
                    bx.Children()?.InsertAtTop(&sel)?;
                }
                bx.Children()?.InsertAtTop(&label)?;
                if !e.all || empty {
                    let caret = c.CreateSpriteVisual()?;
                    caret.SetBrush(&c.CreateColorBrushWithColor(theme::accent())?)?;
                    caret.SetSize(v2((1.5 * s).max(1.0).round(), th))?;
                    caret.SetOffset(v3(tx + if empty { 0.0 } else { tw } + 1.0 * s, ty, 0.0))?;
                    let blink = c.CreateScalarKeyFrameAnimation()?;
                    let lin = c.CreateLinearEasingFunction()?;
                    blink.InsertKeyFrameWithEasingFunction(0.0, 1.0, &lin)?;
                    blink.InsertKeyFrameWithEasingFunction(0.48, 1.0, &lin)?;
                    blink.InsertKeyFrameWithEasingFunction(0.52, 0.0, &lin)?;
                    blink.InsertKeyFrameWithEasingFunction(0.96, 0.0, &lin)?;
                    blink.InsertKeyFrameWithEasingFunction(1.0, 1.0, &lin)?;
                    blink.SetDuration(ms(1060))?;
                    blink.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
                    caret.StartAnimation(&hs("Opacity"), &blink)?;
                    bx.Children()?.InsertAtTop(&caret)?;
                }
                self.title_rect = (tx - 6.0 * s, ty - 4.0 * s, fw, fh);
            }
        }
        Ok(())
    }
}
