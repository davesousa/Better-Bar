//! Launchpad's widget columns (left and right), dragging widgets around (between columns,
//! from the gallery, and out to the dock), the widget gallery, the connect form and
//! Settings → Widgets. A child module of `launchpad`, so it shares its private state.

use super::*;
use crate::config::WRef;
use crate::widgets::catalog::{self as cat, Size, CATS};
use crate::widgets::{self, Def};

/// (column, index): 0 = left column, 1 = right column.
pub type WPos = (usize, usize);

/// A widget card in a column.
pub struct Card {
    pub def: &'static Def,
    pub size: Size,
    /// Positioned in the column's list; holds the shadow and the body.
    vis: ContainerVisual,
    /// The painted widget (and app icons); scales for press and lift.
    body: ContainerVisual,
    grip: ContainerVisual,
    surface: CompositionDrawingSurface,
    /// Top-left in list coordinates (physical px) and size.
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Default)]
pub struct Column {
    cards: Vec<Card>,
    inner: Option<ContainerVisual>,
    /// Window rectangle of the scrolling list, its scroll offset and content height.
    pub view: (f32, f32, f32, f32),
    pub scroll: f32,
    content_h: f32,
    /// Left edge of the cards inside the panel content (physical px) and column width.
    pad: f32,
    col_w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragSrc {
    Gallery(&'static str),
    Col(WPos),
}

/// Where a dragged widget would land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Column(usize, usize),
    Dock,
    None,
}

/// A widget being dragged.
pub struct WDrag {
    pub src: DragSrc,
    pub def: &'static Def,
    pub size: Size,
    ghost: ContainerVisual,
    gw: f32,
    gh: f32,
    grab: (f32, f32),
    pub zone: Zone,
    placeholder: Option<(usize, ShapeVisual)>,
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
    /// Waiting on a lookup (weather: finding the place), shown instead of the error.
    pub busy: Option<String>,
    /// The place being looked up: only that answer may close this form.
    pub pending: Option<String>,
}

const LINK: (f32, f32, f32, f32) = (0.29, 0.62, 1.0, 1.0);

type LocalForm = (&'static str, &'static str, &'static str, &'static [laminar_sys::feed::Field]);

/// Widget settings that aren't a service connection (Settings → Widgets).
pub const LOCAL_FORMS: &[LocalForm] = &[
    ("weather", "Weather location", "The place weather widgets show (any city or town name).", &[laminar_sys::feed::Field { key: "city", label: "City", placeholder: "Toronto", secret: false, required: true }]),
    ("clocks", "World clocks", "Cities for World Clocks and World Time, as Name=Windows time zone, separated by semicolons.", &[laminar_sys::feed::Field { key: "cities", label: "Cities", placeholder: "San Francisco=Pacific Standard Time; London=GMT Standard Time; Tokyo=Tokyo Standard Time", secret: false, required: false }]),
    ("revenue-goal", "Revenue goal", "The 30-day revenue goal the Revenue Goal widget measures against (in dollars).", &[laminar_sys::feed::Field { key: "goal", label: "Goal", placeholder: "10000", secret: false, required: false }]),
];

fn col_panel(ci: usize) -> PanelId {
    if ci == 0 {
        PanelId::Left
    } else {
        PanelId::Right
    }
}

impl Launchpad {
    fn wctx<'a>(&self, d: &'a Data<'a>, sample: bool) -> widgets::Ctx<'a> {
        widgets::Ctx {
            hub: d.hub,
            cfg: d.cfg,
            weather: d.weather,
            stats: d.stats,
            control: d.control,
            pinned: d.pinned,
            names: d.names,
            whatsapp: d.whatsapp,
            tray: d.tray_list,
            hover_tray: self.hover_tray,
            update: d.update,
            sample,
            now: widgets::Now::get(),
        }
    }

    /// The shadow under cards (cached per strength and scale).
    fn card_shadow(&mut self, c: &Compositor, gfx: &Gfx, strength: f32) -> Option<CompositionNineGridBrush> {
        if strength <= 0.01 {
            return None;
        }
        let s = self.scale;
        if let Some((k, sc, b)) = &self.shadow {
            if *k == strength && *sc == s {
                return Some(b.clone());
            }
        }
        let (radius, blur) = (crate::widgets::style::CARD_R * s, 16.0 * s);
        // Hollow: the cards are translucent glass, so a filled shadow would show through.
        let (surface, insets) = gfx.hollow_shadow_surface(radius, blur, 0.55 * strength, 5.0 * s).ok()?;
        let sb = c.CreateSurfaceBrushWithSurface(&surface).ok()?;
        let _ = sb.SetStretch(CompositionStretch::Fill);
        let nine = c.CreateNineGridBrush().ok()?;
        let _ = nine.SetSource(&sb);
        let _ = nine.SetInsets(insets);
        self.shadow = Some((strength, s, nine.clone()));
        Some(nine)
    }

    /// Build a card: shadow, painted body (with app and tray icons on top) and a grip.
    #[allow(clippy::too_many_arguments)]
    fn make_card(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, def: &'static Def, size: Size, w: f32, h: f32, sample: bool) -> Result<(Card, widgets::Painted)> {
        let s = self.scale;
        let surface = gfx.surface(w, h)?;
        let painted = widgets::render(gfx, &surface, def, &self.wctx(d, sample), w, h, s, false)?;
        let vis = c.CreateContainerVisual()?;
        vis.SetSize(v2(w, h))?;
        if let Some(nine) = self.card_shadow(c, gfx, d.cfg.card_shadow) {
            let blur = 16.0 * s;
            let sp = c.CreateSpriteVisual()?;
            sp.SetBrush(&nine)?;
            sp.SetSize(v2(w + 2.0 * blur, h + 2.0 * blur))?;
            sp.SetOffset(v3(-blur, -blur + 5.0 * s, 0.0))?;
            vis.Children()?.InsertAtTop(&sp)?;
        }
        let body = c.CreateContainerVisual()?;
        body.SetSize(v2(w, h))?;
        body.SetCenterPoint(v3(w / 2.0, h / 2.0, 0.0))?;
        body.Children()?.InsertAtTop(&ui::surface_sprite(c, &surface, w, h)?)?;
        for (x, y, sz, id) in &painted.icons {
            let brush = match id.strip_prefix("tray:").and_then(|k| k.parse::<u64>().ok()) {
                Some(k) => d.tray.iter().find(|t| t.key == k).and_then(|t| t.brush.as_ref()),
                None => d.brushes.get(id),
            };
            Self::icon(c, &body, brush, x * s, y * s, sz * s)?;
        }
        vis.Children()?.InsertAtTop(&body)?;
        // Grip: a small pill of dots at the top, shown while the pointer is over the card.
        let grip = c.CreateContainerVisual()?;
        let (gw, gh) = (22.0 * s, 4.0 * s);
        for k in 0..3 {
            let (dot, _, _) = ui::rounded(c, gh, gh, gh / 2.0, theme::ink(0.5))?;
            dot.SetOffset(v3(k as f32 * (gw - gh) / 2.0, 0.0, 0.0))?;
            grip.Children()?.InsertAtTop(&dot)?;
        }
        grip.SetOffset(v3(((w - gw) / 2.0).round(), (4.0 * s).round(), 0.0))?;
        grip.SetOpacity(0.0)?;
        body.Children()?.InsertAtTop(&grip)?;
        Ok((Card { def, size, vis, body, grip, surface, x: 0.0, y: 0.0, w, h }, painted))
    }

    // ------------------------------------------------------------ columns

    pub fn render_left_panel(&mut self, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        if self.open {
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
        self.render_column(0, c, gfx, d)
    }

    /// Lay out and paint column `ci`. Skipped mid-drag so cards stay where the drag put them.
    pub(super) fn render_column(&mut self, ci: usize, c: &Compositor, gfx: &Gfx, d: &Data) -> Result<()> {
        if self.wdrag.is_some() {
            return Ok(());
        }
        let pid = col_panel(ci);
        let Some((content, pw, ph, ox, oy)) = self.panel(pid).map(|(p, (x, y))| (p.content.clone(), p.w, p.h, x, y)) else { return Ok(()) };
        self.clear_hits(pid);
        content.Children()?.RemoveAll()?;
        if self.hover_card.map(|p| p.0 == ci).unwrap_or(false) {
            self.hover_card = None;
        }
        let s = self.scale;
        let pad = 16.0 * s;
        let cw = pw - 2.0 * pad;

        let mut top = pad;
        if ci == 0 {
            // Header: title and "Add" button.
            Self::txt(c, gfx, &content, "Widgets", TextStyle { px: 13.0 * s, weight: 600, color: theme::text_2(), ..Default::default() }, pad + 4.0 * s, pad + 4.0 * s)?;
            let (bw, bh) = (64.0 * s, 26.0 * s);
            let bx = pw - pad - bw;
            Self::rect(c, &content, bx, pad, bw, bh, bh / 2.0, theme::ink(0.10))?;
            Self::txt(c, gfx, &content, "+  Add", TextStyle { px: 12.0 * s, weight: 600, center: true, max_w: Some(bw), ..Default::default() }, bx, pad + 4.5 * s)?;
            self.hit(pid, (ox + bx, oy + pad, bw, bh), Act::AddWidgets, bh / 2.0, true, false);
            top = pad + bh + 12.0 * s;
        }

        let view_h = ph - top - 8.0 * s;
        let host = c.CreateContainerVisual()?;
        host.SetOffset(v3(0.0, top, 0.0))?;
        host.SetSize(v2(pw, view_h))?;
        host.SetClip(&c.CreateInsetClip()?)?;
        content.Children()?.InsertAtTop(&host)?;
        let inner = c.CreateContainerVisual()?;
        host.Children()?.InsertAtTop(&inner)?;
        {
            let col = &mut self.cols[ci];
            col.view = (ox, oy + top, pw, view_h);
            col.pad = pad;
            col.col_w = cw;
            col.cards.clear();
        }
        let refs: Vec<WRef> = if ci == 0 { d.cfg.launchpad_widgets.clone() } else { d.cfg.right_widgets.clone() };
        let items: Vec<(&'static Def, Size)> = refs.iter().filter_map(|r| cat::get(r.id()).map(|def| (def, r.size()))).collect();
        let (rects, total) = widgets::pack(&items.iter().map(|i| i.1).collect::<Vec<_>>(), cw / s);
        for (i, ((def, size), (lx, ly, lw, lh))) in items.into_iter().zip(rects).enumerate() {
            let (x, y, w, h) = ((lx * s).round(), (ly * s).round(), (lw * s).round(), (lh * s).round());
            let (mut card, painted) = self.make_card(c, gfx, d, def, size, w, h, false)?;
            card.x = x;
            card.y = y;
            card.vis.SetOffset(v3(pad + x, y, 0.0))?;
            inner.Children()?.InsertAtTop(&card.vis)?;
            // Hits are in window coordinates (list content), topmost first.
            for ((hx, hy, hw, hh), a) in painted.hits.iter().rev() {
                self.hit_col(ci, (ox + pad + x + hx * s, oy + top + y + hy * s, hw * s, hh * s), Act::Widget((ci, i), a.clone(), 0.0), 18.0 * s, false);
            }
            self.hit_col(ci, (ox + pad + x, oy + top + y, w, h), Act::Card((ci, i)), 18.0 * s, false);
            self.cols[ci].cards.push(card);
        }
        let mut y = total * s;
        if ci == 0 {
            let hint = if refs.is_empty() { "No widgets yet. Click + Add, or drag one here from the gallery." } else { "Drag to arrange · right-click to resize or remove" };
            Self::txt(c, gfx, &inner, hint, TextStyle { px: 11.0 * s, color: theme::text_3(), center: true, max_w: Some(cw), wrap: true, ..Default::default() }, pad, y + 2.0 * s)?;
            y += 30.0 * s;
        }
        let col = &mut self.cols[ci];
        col.content_h = y;
        col.scroll = col.scroll.min((y - view_h).max(0.0));
        inner.SetOffset(v3(0.0, -col.scroll, 0.0))?;
        col.inner = Some(inner);
        Ok(())
    }

    /// Redraw, in place, the cards whose content changes with time or live data (no relayout).
    pub fn repaint_live(&mut self, gfx: &Gfx, d: &Data) -> Result<()> {
        if !self.open || self.wdrag.is_some() {
            return Ok(());
        }
        let s = self.scale;
        let ctx = self.wctx(d, false);
        for col in &self.cols {
            for card in &col.cards {
                let live = card.def.face.live_secs() > 0 || matches!(card.def.face, cat::Face::Sys(_) | cat::Face::Media(_) | cat::Face::Tool(_));
                if live {
                    widgets::render(gfx, &card.surface, card.def, &ctx, card.w, card.h, s, false)?;
                }
            }
        }
        Ok(())
    }

    /// Which widget column (if any) is under the point.
    pub fn column_at(&self, x: f32, y: f32) -> Option<usize> {
        (0..2).find(|&ci| inside(self.cols[ci].view, x, y))
    }

    pub fn on_wheel_col(&mut self, ci: usize, delta: f32) {
        let col = &mut self.cols[ci];
        let max = (col.content_h - col.view.3).max(0.0);
        col.scroll = (col.scroll - delta).clamp(0.0, max);
        if let Some(inner) = &col.inner {
            let _ = inner.SetOffset(v3(0.0, -col.scroll, 0.0));
        }
        self.hover = None;
        if let Some((sh, _)) = &self.hover_shape {
            let _ = sh.SetOpacity(0.0);
        }
    }

    fn card(&self, p: WPos) -> Option<&Card> {
        self.cols.get(p.0).and_then(|c| c.cards.get(p.1))
    }

    /// Show the grip on the card under the pointer.
    pub(super) fn set_hover_card(&mut self, c: &Compositor, p: Option<WPos>) {
        if p == self.hover_card || self.wdrag.is_some() {
            return;
        }
        if let Some(old) = self.hover_card.and_then(|p| self.card(p)) {
            if let Ok(o) = old.grip.cast::<CompositionObject>() {
                let _ = ui::animate_scalar(c, &o, "Opacity", 0.0, 160);
            }
        }
        if let Some(new) = p.and_then(|p| self.card(p)) {
            if let Ok(o) = new.grip.cast::<CompositionObject>() {
                let _ = ui::animate_scalar(c, &o, "Opacity", 1.0, 160);
            }
        }
        self.hover_card = p;
    }

    /// Pressing a card: it dips slightly (holding lifts it, see `begin_drag`).
    pub fn press_card(&mut self, c: &Compositor, p: WPos) {
        self.pressed_card = Some(p);
        if let Some(o) = self.card(p).and_then(|card| card.body.cast::<CompositionObject>().ok()) {
            let _ = ui::animate_v3(c, &o, "Scale", v3(0.97, 0.97, 1.0), 120);
        }
    }

    pub fn release_card(&mut self, c: &Compositor) {
        if let Some(o) = self.pressed_card.take().and_then(|p| self.card(p)).and_then(|card| card.body.cast::<CompositionObject>().ok()) {
            let _ = ui::animate_v3(c, &o, "Scale", v3(1.0, 1.0, 1.0), 180);
        }
    }

    // ------------------------------------------------------------ dragging

    pub fn dragging(&self) -> bool {
        self.wdrag.is_some()
    }

    pub fn drag_info(&self) -> Option<(DragSrc, &'static Def, Size)> {
        self.wdrag.as_ref().map(|d| (d.src, d.def, d.size))
    }

    /// Lift a card (or a gallery preview) and start dragging it with the pointer at (x, y).
    pub fn begin_drag(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, src: DragSrc, x: f32, y: f32) -> Result<()> {
        self.pressed_card = None;
        if let Some(p) = self.hover_card.take().and_then(|p| self.card(p)) {
            let _ = p.grip.SetOpacity(0.0);
        }
        let s = self.scale;
        let (def, size, ghost, body, gw, gh, grab) = match src {
            DragSrc::Col((ci, i)) => {
                let col = &self.cols[ci];
                let Some(card) = col.cards.get(i) else { return Ok(()) };
                let (rx, ry) = (col.view.0 + col.pad + card.x, col.view.1 + card.y - col.scroll);
                // Move the card's visual out of its list onto the top of the window.
                if let Some(inner) = &col.inner {
                    inner.Children()?.Remove(&card.vis)?;
                }
                self.popup.root.Children()?.InsertAtTop(&card.vis)?;
                card.vis.SetOffset(v3(rx, ry, 0.0))?;
                (card.def, card.size, card.vis.clone(), card.body.clone(), card.w, card.h, (x - rx, y - ry))
            }
            DragSrc::Gallery(id) => {
                let Some(def) = cat::get(id) else { return Ok(()) };
                let col_w = if self.cols[0].col_w > 0.0 { self.cols[0].col_w } else { 268.0 * s };
                let (w, h) = widgets::card_size(def.size, col_w / s);
                let (card, _) = self.make_card(c, gfx, d, def, def.size, (w * s).round(), (h * s).round(), true)?;
                self.popup.root.Children()?.InsertAtTop(&card.vis)?;
                let grab = (card.w / 2.0, card.h / 2.0);
                card.vis.SetOffset(v3(x - grab.0, y - grab.1, 0.0))?;
                (def, def.size, card.vis.clone(), card.body.clone(), card.w, card.h, grab)
            }
        };
        // While held, the card is a solid object: back its translucent glass with the
        // panel colour so the slot, hint text and other cards don't show through it.
        let (backing, _, _) = ui::rounded(c, gw, gh, crate::widgets::style::CARD_R * s, theme::surface(0.94))?;
        body.Children()?.InsertAtBottom(&backing)?;
        // Lift: a little larger, with a deeper shadow under it.
        let lift = c.CreateVector3KeyFrameAnimation()?;
        let e = c.CreateCubicBezierEasingFunction(v2(0.3, 1.4), v2(0.6, 1.0))?;
        lift.InsertKeyFrameWithEasingFunction(1.0, v3(1.04, 1.04, 1.0), &e)?;
        lift.SetDuration(ui::ms(220))?;
        body.StartAnimation(&ui::hs("Scale"), &lift)?;
        if let Ok(kids) = ghost.Children() {
            if kids.Count()? > 1 {
                if let Some(Ok(sh)) = kids.First()?.into_iter().next().map(|v| v.cast::<Visual>()) {
                    let _ = ui::animate_v3(c, &sh.cast()?, "Offset", v3(-16.0 * s, -16.0 * s + 16.0 * s, 0.0), 220);
                    let _ = ui::animate_v3(c, &sh.cast()?, "Scale", v3(1.04, 1.06, 1.0), 220);
                }
            }
        }
        self.wdrag = Some(WDrag { src, def, size, ghost, gw, gh, grab, zone: Zone::None, placeholder: None });
        // Show the gap where the card was until the pointer goes elsewhere.
        if let DragSrc::Col((ci, i)) = src {
            self.show_target(c, ci, i);
            if let Some(w) = self.wdrag.as_mut() {
                w.zone = Zone::Column(ci, i);
            }
        }
        Ok(())
    }

    /// The pointer moved during a drag. `dock_top`: window y where the dock begins.
    pub fn drag_move(&mut self, c: &Compositor, x: f32, y: f32, dock_top: f32) -> Zone {
        let Some(w) = &self.wdrag else { return Zone::None };
        let _ = w.ghost.SetOffset(v3(x - w.grab.0, y - w.grab.1, 0.0));
        let (gcx, gcy) = (x - w.grab.0 + w.gw / 2.0, y - w.grab.1 + w.gh / 2.0);
        let old = w.zone;
        let pinned = cat::PINNED.iter().any(|(_, id)| *id == w.def.id);
        let s = self.scale;
        let zone = if y >= dock_top && !pinned {
            Zone::Dock
        } else if let Some(ci) = (0..2).find(|&ci| {
            let v = self.cols[ci].view;
            v.2 > 0.0 && gcx >= v.0 && gcx < v.0 + v.2 && y >= v.1 - 40.0 * s && y < v.1 + v.3 + 40.0 * s
        }) {
            Zone::Column(ci, self.best_slot(ci, gcx, gcy))
        } else {
            Zone::None
        };
        let _ = w.ghost.SetOpacity(if zone == Zone::Dock { 0.0 } else { 1.0 });
        if zone != old {
            if let Zone::Column(oci, _) = old {
                if !matches!(zone, Zone::Column(nci, _) if nci == oci) {
                    self.show_target(c, oci, usize::MAX);
                }
            }
            if let Zone::Column(ci, k) = zone {
                self.show_target(c, ci, k);
            }
            if let Some(w) = self.wdrag.as_mut() {
                w.zone = zone;
            }
        }
        zone
    }

    /// (card index, size) of a column's cards, leaving out the one being dragged.
    fn col_sizes(&self, ci: usize) -> Vec<(usize, Size)> {
        let skip = match self.wdrag.as_ref().map(|w| w.src) {
            Some(DragSrc::Col((sc, si))) if sc == ci => Some(si),
            _ => None,
        };
        self.cols[ci].cards.iter().enumerate().filter(|(i, _)| Some(*i) != skip).map(|(i, c)| (i, c.size)).collect()
    }

    /// The insertion index in column ci closest to the dragged card's centre (window px).
    fn best_slot(&self, ci: usize, gcx: f32, gcy: f32) -> usize {
        let Some(w) = &self.wdrag else { return 0 };
        let col = &self.cols[ci];
        let s = self.scale;
        let (lx, ly) = ((gcx - col.view.0 - col.pad) / s, (gcy - col.view.1 + col.scroll) / s);
        let sizes: Vec<Size> = self.col_sizes(ci).into_iter().map(|x| x.1).collect();
        let mut best = (f32::MAX, 0);
        for k in 0..=sizes.len() {
            let mut v = sizes.clone();
            v.insert(k, w.size);
            let (rects, _) = widgets::pack(&v, col.col_w / s);
            let r = rects[k];
            let (dx, dy) = (lx - (r.0 + r.2 / 2.0), ly - (r.1 + r.3 / 2.0));
            let dist = dx * dx * 0.6 + dy * dy;
            if dist < best.0 {
                best = (dist, k);
            }
        }
        best.1
    }

    /// Move column ci's cards to make room at slot k (usize::MAX: no room, close the gap).
    fn show_target(&mut self, c: &Compositor, ci: usize, k: usize) {
        let s = self.scale;
        let Some(dsize) = self.wdrag.as_ref().map(|w| w.size) else { return };
        let entries = self.col_sizes(ci);
        let mut sizes: Vec<Size> = entries.iter().map(|e| e.1).collect();
        let slot = (k != usize::MAX).then(|| k.min(sizes.len()));
        if let Some(k) = slot {
            sizes.insert(k, dsize);
        }
        let (pad, col_w, inner) = (self.cols[ci].pad, self.cols[ci].col_w, self.cols[ci].inner.clone());
        let (rects, _) = widgets::pack(&sizes, col_w / s);
        let mut ri = 0;
        let mut ph_rect = None;
        for (n, r) in rects.iter().enumerate() {
            if Some(n) == slot {
                ph_rect = Some(*r);
                continue;
            }
            let Some((idx, _)) = entries.get(ri) else { break };
            ri += 1;
            if let Some(o) = self.cols[ci].cards.get(*idx).and_then(|card| card.vis.cast::<CompositionObject>().ok()) {
                let _ = ui::animate_v3(c, &o, "Offset", v3(pad + (r.0 * s).round(), (r.1 * s).round(), 0.0), 200);
            }
        }
        // Placeholder: a soft dashed outline where the widget will land.
        let old = self.wdrag.as_mut().and_then(|w| w.placeholder.take());
        if let Some((pci, ph)) = old {
            if let Some(inn) = &self.cols[pci].inner {
                let _ = inn.Children().and_then(|k| k.Remove(&ph));
            }
        }
        if let (Some(r), Some(inner)) = (ph_rect, inner) {
            if let Ok((ph, _, shape)) = ui::rounded(c, (r.2 * s).round(), (r.3 * s).round(), crate::widgets::style::CARD_R * s, theme::ink(0.05)) {
                if let Ok(b) = c.CreateColorBrushWithColor(theme::ink(0.26)) {
                    let _ = shape.SetStrokeBrush(&b);
                }
                let _ = shape.SetStrokeThickness(1.5 * s);
                if let Ok(a) = shape.StrokeDashArray() {
                    let _ = a.Append(4.0);
                    let _ = a.Append(3.0);
                }
                let _ = ph.SetOffset(v3(pad + (r.0 * s).round(), (r.1 * s).round(), 0.0));
                let _ = inner.Children().and_then(|k| k.InsertAtBottom(&ph));
                if let Some(w) = self.wdrag.as_mut() {
                    w.placeholder = Some((ci, ph));
                }
            }
        }
    }

    /// Finish a drag: the visuals go away (the caller saves the new arrangement and
    /// redraws). Returns what was dragged and where it landed.
    pub fn end_drag(&mut self) -> Option<(DragSrc, &'static Def, Size, Zone)> {
        let w = self.wdrag.take()?;
        let _ = self.popup.root.Children().and_then(|k| k.Remove(&w.ghost));
        if let Some((pci, ph)) = &w.placeholder {
            if let Some(inn) = &self.cols[*pci].inner {
                let _ = inn.Children().and_then(|k| k.Remove(ph));
            }
        }
        Some((w.src, w.def, w.size, w.zone))
    }

    // ------------------------------------------------------------ gallery

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_gallery(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, content: &ContainerVisual, (ox, oy): (f32, f32), pw: f32, ph: f32, cat_i: usize) -> Result<()> {
        let s = self.scale;
        let pad = 26.0 * s;
        let (tw0, _) = Self::txt(c, gfx, content, "Add Widgets", TextStyle { px: 22.0 * s, weight: 600, display: true, ..Default::default() }, pad, 22.0 * s)?;
        Self::txt(c, gfx, content, "Drag a widget into either column or onto the dock, or use the buttons.", TextStyle { px: 12.0 * s, color: theme::text_3(), ..Default::default() }, pad + tw0 + 16.0 * s, 31.0 * s)?;
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

        let cell_w = (268.0 * s).round();
        let col_gap = 22.0 * s;
        let cols = (((pw - 2.0 * pad + col_gap) / (cell_w + col_gap)).floor() as usize).max(1);
        let grid_x = (pw - (cols as f32 * cell_w + (cols - 1) as f32 * col_gap)) / 2.0;
        let defs: Vec<&'static Def> = CATS.get(cat_i).map(|(c, _, _)| cat::in_cat(*c).collect()).unwrap_or_default();
        let in_use = |id: &str| d.cfg.launchpad_widgets.iter().chain(d.cfg.right_widgets.iter()).any(|w| w.id() == id);
        let mut gy = 4.0 * s;
        for row in defs.chunks(cols) {
            let mut row_h: f32 = 0.0;
            for (k, def) in row.iter().enumerate() {
                let cx = grid_x + k as f32 * (cell_w + col_gap);
                // The preview at the widget's own size (small ones centred in the cell).
                let (lw, lh) = widgets::card_size(def.size, cell_w / s);
                let (w, h) = ((lw * s).round(), (lh * s).round());
                let px = cx + ((cell_w - w) / 2.0).round();
                let (card, _) = self.make_card(c, gfx, d, def, def.size, w, h, true)?;
                card.vis.SetOffset(v3(px, gy, 0.0))?;
                inner.Children()?.InsertAtTop(&card.vis)?;
                self.hit(PanelId::Center, (ox + px, oy + top + gy, w, h), Act::GalleryItem(def.id), 18.0 * s, false, true);
                let mut ty = gy + h + 10.0 * s;
                Self::txt(c, gfx, &inner, def.name, TextStyle { px: 13.0 * s, weight: 600, max_w: Some(cell_w), ..Default::default() }, cx + 2.0 * s, ty)?;
                ty += 20.0 * s;
                let (_, ah) = Self::txt(c, gfx, &inner, def.about, TextStyle { px: 11.5 * s, color: theme::text_2(), max_w: Some(cell_w - 4.0 * s), wrap: true, ..Default::default() }, cx + 2.0 * s, ty)?;
                ty += ah.max(16.0 * s) + 8.0 * s;
                // Buttons: Launchpad, Dock, Connect.
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
                let pinned = cat::PINNED.iter().any(|(_, id)| *id == def.id);
                if in_use(def.id) {
                    if pinned {
                        button(self, "✓ Always on", Act::Section(4), true)?;
                    } else {
                        button(self, "✓ Launchpad", Act::RemoveWidget(def.id), true)?;
                    }
                } else {
                    button(self, "+ Launchpad", Act::AddWidget(def.id, false), false)?;
                }
                if !pinned {
                    button(self, "+ Dock", Act::AddWidget(def.id, true), false)?;
                }
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

    // ------------------------------------------------------------ Settings → Widgets

    /// Widget settings and every connection, one row per service (shared by every widget
    /// that uses it). Returns the y below the last row.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_widget_settings(&mut self, c: &Compositor, gfx: &Gfx, d: &Data, content: &ContainerVisual, (ox, oy): (f32, f32), pw: f32, mut y: f32) -> Result<f32> {
        let s = self.scale;
        let pad = 26.0 * s;
        let row_w = pw - 2.0 * pad;
        let rh = 50.0 * s;
        let rows: Vec<(&'static str, String, String, (f32, f32, f32, f32), &'static str)> = {
            let mut v = Vec::new();
            for (id, name, _, fields) in LOCAL_FORMS {
                let value = match *id {
                    "weather" => d.cfg.weather_city.clone(),
                    _ => d.cfg.widget_settings.get(*id).and_then(|m| m.get(fields[0].key)).cloned().unwrap_or_else(|| "Default".into()),
                };
                v.push((*id, name.to_string(), value, theme::text_2(), "Edit"));
            }
            for info in laminar_sys::connectors::all().into_iter().filter(|c| !c.fields.is_empty()) {
                let (status, col) = match (d.hub.feed(info.id), d.hub.err(info.id)) {
                    (_, Some(laminar_sys::feed::FeedError::NeedsSetup(_))) => ("Not connected".to_string(), theme::text_3()),
                    (_, Some(e)) => (e.to_string(), theme::DANGER),
                    (Some(_), None) => ("Connected".to_string(), (0.19, 0.82, 0.35, 1.0)),
                    (None, None) if d.cfg.widget_settings.contains_key(info.id) => ("Saved".to_string(), theme::text_2()),
                    (None, None) => ("Not connected".to_string(), theme::text_3()),
                };
                let users: Vec<&str> = cat::all().filter(|w| widgets::conns(w).contains(&info.id)).map(|w| w.name).take(3).collect();
                let label = if status == "Not connected" { "Connect" } else { "Edit" };
                v.push((info.id, info.name.to_string(), format!("{status} · {}", users.join(", ")), col, label));
            }
            v
        };
        let n_local = LOCAL_FORMS.len();
        for (group, range) in [("Widget settings", 0..n_local), ("Connections · one per service, shared by every widget that uses it", n_local..rows.len())] {
            Self::txt(c, gfx, content, group, TextStyle { px: 12.5 * s, weight: 600, color: theme::text_2(), ..Default::default() }, pad + 4.0 * s, y)?;
            y += 26.0 * s;
            Self::rect(c, content, pad, y, row_w, range.len() as f32 * rh, 14.0 * s, theme::ink(0.06))?;
            for (k, i) in range.clone().enumerate() {
                let (id, name, value, col, action) = &rows[i];
                let ry = y + k as f32 * rh;
                if k > 0 {
                    Self::rect(c, content, pad + 14.0 * s, ry, row_w - 28.0 * s, 1.0, 0.0, theme::hair())?;
                }
                Self::txt(c, gfx, content, name, TextStyle { px: 13.0 * s, ..Default::default() }, pad + 16.0 * s, ry + 8.0 * s)?;
                Self::txt(c, gfx, content, value, TextStyle { px: 11.0 * s, color: *col, max_w: Some(row_w - 150.0 * s), ..Default::default() }, pad + 16.0 * s, ry + 27.0 * s)?;
                let (aw, _) = Self::txt(c, gfx, content, action, TextStyle { px: 12.0 * s, weight: 600, ..Default::default() }, 0.0, -1000.0)?;
                Self::txt(c, gfx, content, action, TextStyle { px: 12.0 * s, weight: 600, color: LINK, ..Default::default() }, pad + row_w - 18.0 * s - aw, ry + (rh - 18.0 * s) / 2.0)?;
                self.hit(PanelId::Center, (ox + pad, oy + ry, row_w, rh), Act::Connect(id), 14.0 * s, true, true);
            }
            y += range.len() as f32 * rh + 22.0 * s;
        }
        Ok(y)
    }

    // ------------------------------------------------------------ connect form

    /// Open the form for a connector (or a local widget setting), filled with saved values.
    pub fn open_form(&mut self, conn: &str, cfg: &crate::config::Config) {
        let (id, name, about, help, fields): (&'static str, &'static str, &'static str, &'static str, &'static [laminar_sys::feed::Field]) = match laminar_sys::connectors::info(conn) {
            Some(i) => (i.id, i.name, i.about, i.help_url, i.fields),
            None => match LOCAL_FORMS.iter().find(|f| f.0 == conn) {
                Some((id, name, about, fields)) => (id, name, about, "", fields),
                None => return,
            },
        };
        let saved = cfg.widget_settings.get(id);
        let fields: Vec<FormField> = fields
            .iter()
            .map(|f| FormField {
                key: f.key,
                label: f.label,
                placeholder: f.placeholder,
                secret: f.secret,
                required: f.required,
                value: if f.secret {
                    String::new()
                } else if id == "weather" {
                    cfg.weather_city.clone()
                } else {
                    saved.and_then(|m| m.get(f.key)).cloned().unwrap_or_default()
                },
                saved: f.secret && laminar_sys::secrets::has(id, f.key),
            })
            .collect();
        self.form = Some(Form { conn: id.to_string(), name, about, help_url: help, fields, focus: 0, error: None, busy: None, pending: None });
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
        let back = if self.gallery.is_some() {
            "‹ Widgets"
        } else if self.settings.is_some() {
            "‹ Settings"
        } else {
            "‹ Back"
        };
        let (bw, bh) = Self::txt(c, gfx, content, back, TextStyle { px: 13.0 * s, weight: 600, color: LINK, ..Default::default() }, x0, y)?;
        self.hit(PanelId::Center, (ox + x0 - 8.0 * s, oy + y - 6.0 * s, bw + 16.0 * s, bh + 12.0 * s), Act::FormCancel, 8.0 * s, true, false);
        y += 38.0 * s;
        let local = LOCAL_FORMS.iter().any(|f| f.0 == form.conn);
        let title = if local { form.name.to_string() } else { format!("Connect {}", form.name) };
        Self::txt(c, gfx, content, &title, TextStyle { px: 22.0 * s, weight: 600, display: true, max_w: Some(w), ..Default::default() }, x0, y)?;
        y += 38.0 * s;
        let (_, ah) = Self::txt(c, gfx, content, form.about, TextStyle { px: 12.5 * s, color: theme::text_2(), max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
        y += ah + 8.0 * s;
        if !form.help_url.is_empty() {
            let (hw, hh) = Self::txt(c, gfx, content, "Where do I get this? ↗", TextStyle { px: 12.5 * s, weight: 500, color: LINK, ..Default::default() }, x0, y)?;
            self.hit(PanelId::Center, (ox + x0 - 6.0 * s, oy + y - 4.0 * s, hw + 12.0 * s, hh + 8.0 * s), Act::Uri(form.help_url.to_string()), 6.0 * s, true, false);
            y += hh + 14.0 * s;
        }
        if !local {
            let status = match (d.hub.feed(&form.conn), d.hub.err(&form.conn)) {
                (_, Some(e)) => Some((e.to_string(), theme::DANGER)),
                (Some(f), None) if !f.extra.contains_key("sample") => Some(("Connected and working.".to_string(), (0.19, 0.82, 0.35, 1.0))),
                _ => None,
            };
            if let Some((t, col)) = status {
                Self::txt(c, gfx, content, &t, TextStyle { px: 12.0 * s, weight: 500, color: col, max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
                y += 24.0 * s;
            }
            let users: Vec<&str> = cat::all().filter(|wd| widgets::conns(wd).contains(&form.conn.as_str())).map(|wd| wd.name).collect();
            if !users.is_empty() {
                let (_, uh) = Self::txt(c, gfx, content, &format!("Used by: {}", users.join(", ")), TextStyle { px: 11.5 * s, color: theme::text_3(), max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
                y += uh + 6.0 * s;
            }
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
        if let Some(b) = &form.busy {
            Self::txt(c, gfx, content, b, TextStyle { px: 12.0 * s, weight: 500, color: theme::text_2(), max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
            y += 24.0 * s;
        } else if let Some(e) = &form.error {
            Self::txt(c, gfx, content, e, TextStyle { px: 12.0 * s, weight: 500, color: theme::DANGER, max_w: Some(w), wrap: true, ..Default::default() }, x0, y)?;
            y += 24.0 * s;
        }
        let bh = 34.0 * s;
        let mut bx = x0;
        let can_disconnect = !local && (form.fields.iter().any(|f| f.saved) || d.cfg.widget_settings.contains_key(&form.conn));
        for (label, act, primary, danger) in [("Save", Act::FormSave, true, false), ("Cancel", Act::FormCancel, false, false)].into_iter().chain(can_disconnect.then_some(("Disconnect", Act::FormDisconnect, false, true))) {
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
