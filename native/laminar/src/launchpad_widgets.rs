//! Launchpad's widget column (drag up and down to reorder), the widget gallery and the
//! connector form. A child module of `launchpad`, so it shares its private state.

use super::*;
use crate::widgets::catalog::{self as cat, CATS};
use crate::widgets::{self, Def};

/// A widget card in the left column.
pub struct Card {
    pub def: &'static Def,
    vis: ContainerVisual,
    surface: CompositionDrawingSurface,
    /// Top of the card in list coordinates (physical px) and its size.
    y: f32,
    w: f32,
    h: f32,
}

/// A card being dragged.
pub struct Reorder {
    from: usize,
    to: usize,
    /// Pointer y minus the card's top (window px) when the drag started.
    grab: f32,
}

#[derive(Clone)]
pub struct FormField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub secret: bool,
    pub required: bool,
    /// What's typed (secrets: only what's typed now; the saved value is never read back).
    pub value: String,
    /// A secret is already stored in Credential Manager.
    pub saved: bool,
}

#[derive(Clone)]
pub struct Form {
    pub conn: String,
    pub name: &'static str,
    pub about: &'static str,
    pub help_url: &'static str,
    pub fields: Vec<FormField>,
    pub focus: usize,
    pub error: Option<String>,
}

const LINK: (f32, f32, f32, f32) = (0.29, 0.62, 1.0, 1.0);

impl Launchpad {
    fn wctx<'a>(d: &'a Data<'a>, sample: bool) -> widgets::Ctx<'a> {
        widgets::Ctx {
            hub: d.hub,
            cfg: d.cfg,
            weather: d.weather,
            stats: d.stats,
            control: d.control,
            pinned: d.pinned,
            names: d.names,
            whatsapp: d.whatsapp,
            sample,
            now: widgets::Now::get(),
        }
    }

    /// A widget as a container: its painted bitmap plus app icons on top.
    fn widget_visual(c: &Compositor, gfx: &Gfx, d: &Data, def: &'static Def, w: f32, h: f32, scale: f32, sample: bool) -> Result<(ContainerVisual, CompositionDrawingSurface, widgets::Painted)> {
        let surface = gfx.surface(w, h)?;
        let painted = widgets::render(gfx, &surface, def, &Self::wctx(d, sample), w, h, scale, false)?;
        let vis = c.CreateContainerVisual()?;
        vis.SetSize(v2(w, h))?;
        vis.Children()?.InsertAtTop(&ui::surface_sprite(c, &surface, w, h)?)?;
        for (x, y, size, id) in &painted.icons {
            Self::icon(c, &vis, d.brushes.get(id), x * scale, y * scale, size * scale)?;
        }
        Ok((vis, surface, painted))
    }

    // ------------------------------------------------------------ left column

    pub fn render_left_panel(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        if self.open && self.reorder.is_none() {
            self.render_left(c, gfx, d)
        } else {
            Ok(())
        }
    }

    pub fn render_center_panel(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        if self.open {
            self.render_center(c, gfx, d)
        } else {
            Ok(())
        }
    }

    pub(super) fn render_widget_column(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        if self.reorder.is_some() {
            // Mid-drag: keep the cards where the drag put them (data refreshes wait).
            return Ok(());
        }
        let Some((content, pw, ph, ox, oy)) = self.panel(PanelId::Left).map(|(p, (x, y))| (p.content.clone(), p.w, p.h, x, y)) else { return Ok(()) };
        self.clear_hits(PanelId::Left);
        content.Children()?.RemoveAll()?;
        self.cards.clear();
        let s = self.scale;
        let pad = 16.0 * s;
        let cw = pw - 2.0 * pad;

        // Header: title and "Add" button.
        Self::txt(c, gfx, &content, "Widgets", TextStyle { px: 13.0 * s, weight: 600, color: theme::text_2(), ..Default::default() }, pad + 4.0 * s, pad + 4.0 * s)?;
        let (bw, bh) = (64.0 * s, 26.0 * s);
        let bx = pw - pad - bw;
        Self::rect(c, &content, bx, pad, bw, bh, bh / 2.0, theme::ink(0.10))?;
        Self::txt(c, gfx, &content, "+  Add", TextStyle { px: 12.0 * s, weight: 600, center: true, max_w: Some(bw), ..Default::default() }, bx, pad + 4.5 * s)?;
        self.hit(PanelId::Left, (ox + bx, oy + pad, bw, bh), Act::AddWidgets, bh / 2.0, true, false);

        // Scrolling list.
        let top = pad + bh + 12.0 * s;
        let view_h = ph - top - 8.0 * s;
        let host = c.CreateContainerVisual()?;
        host.SetOffset(v3(0.0, top, 0.0))?;
        host.SetSize(v2(pw, view_h))?;
        host.SetClip(&c.CreateInsetClip()?)?;
        content.Children()?.InsertAtTop(&host)?;
        let inner = c.CreateContainerVisual()?;
        host.Children()?.InsertAtTop(&inner)?;
        self.left_view = (ox, oy + top, pw, view_h);
        let mut y = 0.0;
        let ids = d.cfg.launchpad_widgets.clone();
        for (i, id) in ids.iter().enumerate() {
            let Some(def) = cat::get(id) else { continue };
            let h = (widgets::card_h(def) * s).round();
            let (vis, surface, painted) = Self::widget_visual(c, gfx, d, def, cw, h, s, false)?;
            vis.SetOffset(v3(pad, y, 0.0))?;
            inner.Children()?.InsertAtTop(&vis)?;
            // Hits are in window coordinates (list content), topmost first.
            for ((hx, hy, hw, hh), a) in painted.hits.iter().rev() {
                self.hit_left((ox + pad + hx * s, oy + top + y + hy * s, hw * s, hh * s), Act::Widget(i, a.clone(), 0.0), 18.0 * s, false);
            }
            self.hit_left((ox + pad, oy + top + y, cw, h), Act::Card(i), 18.0 * s, false);
            self.cards.push(Card { def, vis, surface, y, w: cw, h });
            y += h + widgets::GAP * s;
        }
        if ids.is_empty() {
            Self::txt(c, gfx, &inner, "No widgets yet. Click + Add to choose some.", TextStyle { px: 12.0 * s, color: theme::text_3(), max_w: Some(cw), wrap: true, ..Default::default() }, pad + 4.0 * s, 8.0 * s)?;
        } else {
            Self::txt(c, gfx, &inner, "Drag to reorder · right-click for options", TextStyle { px: 11.0 * s, color: theme::text_3(), center: true, max_w: Some(cw), ..Default::default() }, pad, y + 2.0 * s)?;
            y += 24.0 * s;
        }
        self.left_content_h = y;
        self.left_scroll = self.left_scroll.min((y - view_h).max(0.0));
        inner.SetOffset(v3(0.0, -self.left_scroll, 0.0))?;
        self.left_inner = Some(inner);
        Ok(())
    }

    /// Redraw the cards whose content changes with time (clocks, timers, live stats),
    /// in place, without rebuilding the column.
    pub fn repaint_live(&mut self, gfx: &Gfx, d: &Data) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let s = self.scale;
        let ctx = Self::wctx(d, false);
        for card in &self.cards {
            let live = card.def.face.live_secs() > 0 || matches!(card.def.face, cat::Face::Sys(_) | cat::Face::Media(_));
            if live {
                widgets::render(gfx, &card.surface, card.def, &ctx, card.w, card.h, s, false)?;
            }
        }
        Ok(())
    }

    pub fn on_wheel_left(&mut self, delta: f32) {
        let max = (self.left_content_h - self.left_view.3).max(0.0);
        self.left_scroll = (self.left_scroll - delta).clamp(0.0, max);
        if let Some(inner) = &self.left_inner {
            let _ = inner.SetOffset(v3(0.0, -self.left_scroll, 0.0));
        }
        self.hover = None;
        if let Some((sh, _)) = &self.hover_shape {
            let _ = sh.SetOpacity(0.0);
        }
    }

    /// Is the point over the left column's list?
    pub fn over_left(&self, x: f32, y: f32) -> bool {
        inside(self.left_view, x, y)
    }

    // ------------------------------------------------------------ reorder by dragging

    pub fn reordering(&self) -> bool {
        self.reorder.is_some()
    }

    pub fn begin_reorder(&mut self, c: &Compositor, i: usize, y: f32) {
        let Some(card) = self.cards.get(i) else { return };
        let top = self.left_view.1 + card.y - self.left_scroll;
        self.reorder = Some(Reorder { from: i, to: i, grab: y - top });
        if let Some(inner) = &self.left_inner {
            // Lift: on top of the others, slightly larger.
            if let Ok(kids) = inner.Children() {
                let _ = kids.Remove(&card.vis);
                let _ = kids.InsertAtTop(&card.vis);
            }
        }
        let _ = card.vis.SetCenterPoint(v3(card.w / 2.0, card.h / 2.0, 0.0));
        if let Ok(o) = card.vis.cast::<CompositionObject>() {
            let _ = ui::animate_v3(c, &o, "Scale", v3(1.03, 1.03, 1.0), 140);
            let _ = ui::animate_scalar(c, &o, "Opacity", 0.92, 140);
        }
        self.hover = None;
        if let Some((sh, _)) = &self.hover_shape {
            let _ = sh.SetOpacity(0.0);
        }
    }

    /// The pointer moved during a reorder: the card follows it, the others make room.
    pub fn reorder_move(&mut self, c: &Compositor, y: f32) {
        let Some(r) = &self.reorder else { return };
        let (from, grab, old_to) = (r.from, r.grab, r.to);
        let s = self.scale;
        let gap = widgets::GAP * s;
        let pad = 16.0 * s;
        let list_y = y - grab - self.left_view.1 + self.left_scroll;
        let max_y = (self.left_content_h - self.cards[from].h).max(0.0);
        let _ = self.cards[from].vis.SetOffset(v3(pad, list_y.clamp(-10.0 * s, max_y + 10.0 * s), 0.0));
        // New slot: how many other cards' middles lie above the dragged card's middle.
        let mid = list_y + self.cards[from].h / 2.0;
        let mut to = 0;
        let mut acc = 0.0;
        for (k, card) in self.cards.iter().enumerate() {
            if k == from {
                continue;
            }
            if acc + card.h / 2.0 < mid {
                to += 1;
            }
            acc += card.h + gap;
        }
        if to == old_to {
            return;
        }
        if let Some(r) = self.reorder.as_mut() {
            r.to = to;
        }
        // Lay the others out with a gap where it would land.
        let dragged_h = self.cards[from].h + gap;
        let mut yy = 0.0;
        let mut slot = 0;
        for (k, card) in self.cards.iter().enumerate() {
            if k == from {
                continue;
            }
            if slot == to {
                yy += dragged_h;
            }
            if let Ok(o) = card.vis.cast::<CompositionObject>() {
                let _ = ui::animate_v3(c, &o, "Offset", v3(pad, yy, 0.0), 180);
            }
            yy += card.h + gap;
            slot += 1;
        }
    }

    /// Finish a reorder: (from, to) list positions, or None if it didn't move.
    pub fn end_reorder(&mut self) -> Option<(usize, usize)> {
        let r = self.reorder.take()?;
        (r.from != r.to).then_some((r.from, r.to))
    }

    // ------------------------------------------------------------ gallery

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_gallery(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, content: &ContainerVisual, (ox, oy): (f32, f32), pw: f32, ph: f32, cat_i: usize) -> Result<()> {
        let s = self.scale;
        let pad = 26.0 * s;
        Self::txt(c, gfx, content, "Add Widgets", TextStyle { px: 22.0 * s, weight: 600, display: true, ..Default::default() }, pad, 22.0 * s)?;
        let (dw, dh) = Self::txt(c, gfx, content, "Done", TextStyle { px: 13.0 * s, weight: 600, color: LINK, ..Default::default() }, 0.0, -1000.0)?;
        let dx = pw - pad - dw - 8.0 * s;
        Self::txt(c, gfx, content, "Done", TextStyle { px: 13.0 * s, weight: 600, color: LINK, ..Default::default() }, dx, 30.0 * s)?;
        self.hit(PanelId::Center, (ox + dx - 10.0 * s, oy + 24.0 * s, dw + 20.0 * s, dh + 12.0 * s), Act::CloseGallery, 8.0 * s, true, false);

        // Category chips (wrapping).
        let mut x = pad;
        let mut y = 70.0 * s;
        let chip_h = 30.0 * s;
        for (k, (_, name, _)) in CATS.iter().enumerate() {
            let st = TextStyle { px: 12.5 * s, weight: if k == cat_i { 600 } else { 500 }, color: if k == cat_i { theme::WHITE } else { theme::text() }, ..Default::default() };
            let (tw, _) = Self::txt(c, gfx, content, name, st.clone(), 0.0, -1000.0)?;
            let w = tw + 26.0 * s;
            if x + w > pw - pad {
                x = pad;
                y += chip_h + 8.0 * s;
            }
            Self::rect(c, content, x, y, w, chip_h, chip_h / 2.0, if k == cat_i { theme::accent() } else { theme::ink(0.10) })?;
            Self::txt(c, gfx, content, name, st, x + 13.0 * s, y + 6.0 * s)?;
            if k != cat_i {
                self.hit(PanelId::Center, (ox + x, oy + y, w, chip_h), Act::GalleryCat(k), chip_h / 2.0, true, false);
            }
            x += w + 8.0 * s;
        }
        y += chip_h + 10.0 * s;
        Self::txt(c, gfx, content, CATS.get(cat_i).map(|c| c.2).unwrap_or(""), TextStyle { px: 12.0 * s, color: theme::text_2(), ..Default::default() }, pad, y)?;
        y += 26.0 * s;

        // Scrolling grid of previews.
        let top = y;
        let view_h = ph - top - 8.0 * s;
        let host = c.CreateContainerVisual()?;
        host.SetOffset(v3(0.0, top, 0.0))?;
        host.SetSize(v2(pw, view_h))?;
        host.SetClip(&c.CreateInsetClip()?)?;
        content.Children()?.InsertAtTop(&host)?;
        let inner = c.CreateContainerVisual()?;
        host.Children()?.InsertAtTop(&inner)?;
        self.scroll_view = (ox, oy + top, pw, view_h);

        let card_w = (268.0 * s).round();
        let col_gap = 22.0 * s;
        let cols = (((pw - 2.0 * pad + col_gap) / (card_w + col_gap)).floor() as usize).max(1);
        let grid_x = (pw - (cols as f32 * card_w + (cols - 1) as f32 * col_gap)) / 2.0;
        let defs: Vec<&'static Def> = CATS.get(cat_i).map(|(c, _, _)| cat::in_cat(*c).collect()).unwrap_or_default();
        let mut gy = 4.0 * s;
        for row in defs.chunks(cols) {
            let mut row_h: f32 = 0.0;
            for (k, def) in row.iter().enumerate() {
                let cx = grid_x + k as f32 * (card_w + col_gap);
                let h = (widgets::card_h(def) * s).round();
                let (vis, _, _) = Self::widget_visual(c, gfx, d, def, card_w, h, s, true)?;
                vis.SetOffset(v3(cx, gy, 0.0))?;
                inner.Children()?.InsertAtTop(&vis)?;
                let mut ty = gy + h + 10.0 * s;
                Self::txt(c, gfx, &inner, def.name, TextStyle { px: 13.0 * s, weight: 600, max_w: Some(card_w), ..Default::default() }, cx + 2.0 * s, ty)?;
                ty += 20.0 * s;
                let (_, ah) = Self::txt(c, gfx, &inner, def.about, TextStyle { px: 11.5 * s, color: theme::text_2(), max_w: Some(card_w - 4.0 * s), wrap: true, ..Default::default() }, cx + 2.0 * s, ty)?;
                ty += ah.max(16.0 * s) + 8.0 * s;
                // Buttons: Launchpad, Dock, Connect.
                let in_lp = d.cfg.launchpad_widgets.iter().any(|w| w == def.id);
                let bh = 26.0 * s;
                let mut bx = cx;
                let mut button = |me: &mut Self, label: &str, act: Act, on: bool| -> Result<()> {
                    let st = TextStyle { px: 11.5 * s, weight: 600, color: if on { theme::WHITE } else { theme::text() }, ..Default::default() };
                    let (tw, _) = Self::txt(c, gfx, &inner, label, st.clone(), 0.0, -1000.0)?;
                    let w = tw + 22.0 * s;
                    Self::rect(c, &inner, bx, ty, w, bh, bh / 2.0, if on { theme::accent() } else { theme::ink(0.12) })?;
                    Self::txt(c, gfx, &inner, label, st, bx + 11.0 * s, ty + 4.5 * s)?;
                    me.hit(PanelId::Center, (ox + bx, oy + top + ty, w, bh), act, bh / 2.0, true, true);
                    bx += w + 6.0 * s;
                    Ok(())
                };
                if in_lp {
                    button(self, "✓ Launchpad", Act::RemoveWidget(def.id), true)?;
                } else {
                    button(self, "+ Launchpad", Act::AddWidget(def.id, false), false)?;
                }
                button(self, "+ Dock", Act::AddWidget(def.id, true), false)?;
                if let Some(conn) = widgets::conns(def).into_iter().find(|c| laminar_sys::connectors::info(c).map(|i| !i.fields.is_empty()).unwrap_or(false)) {
                    let ok = d.hub.feed(conn).is_some() && d.hub.err(conn).is_none();
                    button(self, if ok { "Connected ✓" } else { "Connect…" }, Act::Connect(conn), false)?;
                }
                row_h = row_h.max(ty + bh - gy);
            }
            gy += row_h + 26.0 * s;
        }
        self.content_h = gy;
        self.scroll_y = self.scroll_y.min((gy - view_h).max(0.0));
        inner.SetOffset(v3(0.0, -self.scroll_y, 0.0))?;
        self.scroll_host = Some(inner);
        Ok(())
    }

    // ------------------------------------------------------------ connect form

    /// Open the form for a connector, filled with its saved settings.
    pub fn open_form(&mut self, conn: &str, cfg: &crate::config::Config) {
        let Some(info) = laminar_sys::connectors::info(conn) else { return };
        let saved = cfg.widget_settings.get(conn);
        let fields: Vec<FormField> = info
            .fields
            .iter()
            .map(|f| FormField {
                key: f.key,
                label: f.label,
                placeholder: f.placeholder,
                secret: f.secret,
                required: f.required,
                value: if f.secret { String::new() } else { saved.and_then(|m| m.get(f.key)).cloned().unwrap_or_default() },
                saved: f.secret && laminar_sys::secrets::has(conn, f.key),
            })
            .collect();
        self.form = Some(Form { conn: conn.to_string(), name: info.name, about: info.about, help_url: info.help_url, fields, focus: 0, error: None });
        self.settings = None;
        self.query.clear();
    }

    /// Typing into the focused field. Returns true if something changed.
    pub fn form_char(&mut self, ch: char) -> bool {
        let Some(f) = self.form.as_mut() else { return false };
        let Some(field) = f.fields.get_mut(f.focus) else { return false };
        if ch.is_control() {
            return false;
        }
        field.value.push(ch);
        f.error = None;
        true
    }

    pub fn form_paste(&mut self, text: &str) -> bool {
        let Some(f) = self.form.as_mut() else { return false };
        let Some(field) = f.fields.get_mut(f.focus) else { return false };
        field.value.push_str(text.trim());
        true
    }

    pub fn form_backspace(&mut self) -> bool {
        let Some(f) = self.form.as_mut() else { return false };
        let Some(field) = f.fields.get_mut(f.focus) else { return false };
        field.value.pop().is_some()
    }

    pub fn form_next(&mut self, back: bool) {
        if let Some(f) = self.form.as_mut() {
            let n = f.fields.len().max(1);
            f.focus = if back { (f.focus + n - 1) % n } else { (f.focus + 1) % n };
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_form(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, content: &ContainerVisual, (ox, oy): (f32, f32), pw: f32, _ph: f32) -> Result<()> {
        let Some(form) = self.form.clone() else { return Ok(()) };
        let s = self.scale;
        let pad = 26.0 * s;
        let w = (pw - 2.0 * pad).min(560.0 * s);
        let x0 = (pw - w) / 2.0;
        let mut y = 22.0 * s;
        let back = if self.gallery.is_some() { "‹ Widgets" } else { "‹ Back" };
        let (bw, bh) = Self::txt(c, gfx, content, back, TextStyle { px: 13.0 * s, weight: 600, color: LINK, ..Default::default() }, x0, y)?;
        self.hit(PanelId::Center, (ox + x0 - 8.0 * s, oy + y - 6.0 * s, bw + 16.0 * s, bh + 12.0 * s), Act::FormCancel, 8.0 * s, true, false);
        y += 38.0 * s;
        Self::txt(c, gfx, content, &format!("Connect {}", form.name), TextStyle { px: 22.0 * s, weight: 600, display: true, max_w: Some(w), ..Default::default() }, x0, y)?;
        y += 38.0 * s;
        let (_, ah) = Self::txt(c, gfx, content, form.about, TextStyle { px: 12.5 * s, color: theme::text_2(), max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
        y += ah + 8.0 * s;
        if !form.help_url.is_empty() {
            let (hw, hh) = Self::txt(c, gfx, content, "Where do I get this? ↗", TextStyle { px: 12.5 * s, weight: 500, color: LINK, ..Default::default() }, x0, y)?;
            self.hit(PanelId::Center, (ox + x0 - 6.0 * s, oy + y - 4.0 * s, hw + 12.0 * s, hh + 8.0 * s), Act::Uri(form.help_url.to_string()), 6.0 * s, true, false);
            y += hh + 14.0 * s;
        }
        // Current state of the connector.
        let status = match (d.hub.feed(&form.conn), d.hub.err(&form.conn)) {
            (_, Some(e)) => Some((e.to_string(), theme::DANGER)),
            (Some(f), None) if f.extra.get("sample").is_none() => Some(("Connected and working.".to_string(), (0.19, 0.82, 0.35, 1.0))),
            _ => None,
        };
        if let Some((t, col)) = status {
            Self::txt(c, gfx, content, &t, TextStyle { px: 12.0 * s, weight: 500, color: col, max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
            y += 24.0 * s;
        }
        y += 6.0 * s;

        for (k, f) in form.fields.iter().enumerate() {
            let label = if f.required { f.label.to_string() } else { format!("{} (optional)", f.label) };
            Self::txt(c, gfx, content, &label, TextStyle { px: 12.5 * s, weight: 600, ..Default::default() }, x0, y)?;
            y += 22.0 * s;
            let fh = 40.0 * s;
            let focused = k == form.focus;
            let (field, _, shape) = ui::rounded(c, w, fh, 10.0 * s, theme::ink(0.10))?;
            shape.SetStrokeBrush(&c.CreateColorBrushWithColor(if focused { theme::accent() } else { theme::ink(0.14) })?)?;
            shape.SetStrokeThickness(if focused { 2.0 } else { 1.0 })?;
            field.SetOffset(v3(x0, y, 0.0))?;
            content.Children()?.InsertAtTop(&field)?;
            let (shown, col) = if !f.value.is_empty() {
                (if f.secret { "•".repeat(f.value.chars().count().min(48)) } else { f.value.clone() }, theme::text())
            } else if f.saved {
                ("Saved in Credential Manager — type to replace".to_string(), theme::text_3())
            } else {
                (f.placeholder.to_string(), theme::text_3())
            };
            let (tw, _) = Self::txt(c, gfx, content, &shown, TextStyle { px: 13.0 * s, color: col, max_w: Some(w - 28.0 * s), ..Default::default() }, x0 + 14.0 * s, y + 11.0 * s)?;
            if focused {
                let cx = x0 + 14.0 * s + if f.value.is_empty() { 0.0 } else { tw.min(w - 28.0 * s) };
                let (caret, _, _) = ui::rounded(c, (1.5 * s).max(1.0), 20.0 * s, 0.0, theme::accent())?;
                caret.SetOffset(v3(cx, y + 10.0 * s, 0.0))?;
                content.Children()?.InsertAtTop(&caret)?;
            }
            self.hit(PanelId::Center, (ox + x0, oy + y, w, fh), Act::FormFocus(k), 10.0 * s, false, false);
            y += fh + 14.0 * s;
        }
        if form.fields.iter().any(|f| f.secret) {
            Self::txt(c, gfx, content, "Keys are kept in Windows Credential Manager on this PC (never in config.json) and only sent to the service they belong to.", TextStyle { px: 11.5 * s, color: theme::text_3(), max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
            y += 52.0 * s;
        }
        if let Some(e) = &form.error {
            Self::txt(c, gfx, content, e, TextStyle { px: 12.0 * s, weight: 500, color: theme::DANGER, max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
            y += 24.0 * s;
        }
        // Buttons.
        let bh = 34.0 * s;
        let mut bx = x0;
        for (label, act, primary, danger) in [
            ("Save", Act::FormSave, true, false),
            ("Cancel", Act::FormCancel, false, false),
        ]
        .into_iter()
        .chain((form.fields.iter().any(|f| f.saved) || d.cfg.widget_settings.contains_key(&form.conn)).then_some(("Disconnect", Act::FormDisconnect, false, true)))
        {
            let st = TextStyle { px: 12.5 * s, weight: 600, color: if primary { theme::WHITE } else if danger { theme::DANGER } else { theme::text() }, ..Default::default() };
            let (tw, _) = Self::txt(c, gfx, content, label, st.clone(), 0.0, -1000.0)?;
            let bw = tw + 36.0 * s;
            Self::rect(c, content, bx, y, bw, bh, 9.0 * s, if primary { theme::accent() } else { theme::ink(0.12) })?;
            Self::txt(c, gfx, content, label, st, bx + 18.0 * s, y + (bh - 18.0 * s) / 2.0)?;
            self.hit(PanelId::Center, (ox + bx, oy + y, bw, bh), act, 9.0 * s, true, false);
            bx += bw + 10.0 * s;
        }
        Ok(())
    }
}
