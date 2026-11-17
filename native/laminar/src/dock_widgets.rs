//! The dock's side of widgets: painting dock widgets, routing clicks from the dock and
//! Launchpad, the once-a-second refresh, dragging widgets (in Launchpad and out to the
//! dock), widget menus and the connect form.
//! (A child module of `dock`, so it can use the dock's private state.)

use super::*;
use crate::config::WRef;
use crate::launchpad::{DragSrc, WPos, Zone};
use crate::widgets::catalog::{Face, Size, SysKind, TimerKind, ToolKind};
use crate::widgets::{self, Now, WAct};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

/// Menu commands: widget sizes (base + size index).
const CMD_SIZE_BASE: usize = 3900;
/// Menu command: move a Launchpad widget to the other column.
const CMD_LPW_OTHER_COL: usize = 3910;
/// "Change Location…" and the like (a widget's on-this-PC settings), from the dock / Launchpad.
pub(super) const CMD_WIDGET_LOCAL: usize = 80;
const CMD_LPW_LOCAL: usize = 81;

fn size_code(s: Size) -> usize {
    match s {
        Size::Small => 0,
        Size::Medium => 1,
        Size::Large => 2,
    }
}

impl Dock {
    /// Is WhatsApp running, and does it want attention?
    pub(super) fn whatsapp_state(&self) -> Option<bool> {
        let wins: Vec<&Win> = self.windows.iter().filter(|w| w.app.contains("whatsapp") || w.exe.contains("whatsapp")).collect();
        if wins.is_empty() {
            return None;
        }
        Some(wins.iter().any(|w| self.attention.contains(&(w.hwnd.0 as isize))))
    }

    /// Paint dock widgets. `all` false: only those that show live system stats.
    pub(super) fn paint_dock_widgets(&mut self, all: bool) {
        let idx: Vec<usize> = self
            .wviews
            .iter()
            .filter(|(_, v)| all || matches!(v.def.face, Face::Sys(SysKind::Resource | SysKind::Cpu | SysKind::Memory | SysKind::Network)))
            .map(|(i, _)| *i)
            .collect();
        self.paint_dock_set(&idx);
    }

    fn paint_dock_set(&mut self, idx: &[usize]) {
        if idx.is_empty() {
            return;
        }
        let pinned = self.lp_pinned();
        if idx.iter().any(|i| self.wviews.get(i).map(|v| matches!(v.def.face, Face::Tool(ToolKind::Apps))).unwrap_or(false)) {
            self.lp_brushes(&pinned);
        }
        let tray_views = if idx.iter().any(|i| self.wviews.get(i).map(|v| matches!(v.def.face, Face::Tool(ToolKind::Tray))).unwrap_or(false)) { self.tray_views() } else { Vec::new() };
        let tray: Vec<(u64, String)> = tray_views.iter().map(|t| (t.key, t.tip.clone())).collect();
        let wa = self.whatsapp_state();
        let mut results = Vec::new();
        {
            let ctx = widgets::Ctx {
                hub: &self.hub,
                cfg: &self.cfg,
                weather: self.weather.as_ref(),
                stats: self.stats.as_ref(),
                control: &self.control,
                pinned: &pinned,
                names: &self.names,
                whatsapp: wa,
                tray: &tray,
                hover_tray: None,
                update: self.update.as_ref(),
                sample: false,
                now: Now::get(),
            };
            for i in idx {
                let Some(v) = self.wviews.get(i) else { continue };
                match widgets::render(&self.gfx, &v.surface, v.def, &ctx, v.w * v.res, v.h * v.res, self.scale * v.res, true) {
                    Ok(p) => results.push((*i, p)),
                    Err(e) => trace!("widget {} paint failed: {e:?}", v.def.id),
                }
            }
        }
        let c = self.compositor.clone();
        for (i, p) in results {
            let Some(v) = self.wviews.get_mut(&i) else { continue };
            // App and tray icons sit on top of the painted bitmap (they're Composition brushes).
            if let Ok(kids) = v.tile.Children() {
                while kids.Count().unwrap_or(0) > 1 {
                    let last = kids.First().ok().and_then(|it| it.into_iter().last());
                    match last {
                        Some(top) => {
                            let _ = kids.Remove(&top);
                        }
                        None => break,
                    }
                }
                for (x, y, size, id) in &p.icons {
                    let brush = match id.strip_prefix("tray:").and_then(|k| k.parse::<u64>().ok()) {
                        Some(k) => tray_views.iter().find(|t| t.key == k).and_then(|t| t.brush.clone()),
                        None => self.brushes.get(id).cloned(),
                    };
                    if let Some(b) = brush {
                        if let Ok(sp) = c.CreateSpriteVisual() {
                            let _ = sp.SetBrush(&b);
                            let _ = sp.SetSize(v2(size * self.scale, size * self.scale));
                            let _ = sp.SetOffset(v3(x * self.scale, y * self.scale, 0.0));
                            let _ = kids.InsertAtTop(&sp);
                        }
                    }
                }
            }
            v.painted = p;
        }
    }

    /// A click on a dock widget: find what's under the pointer inside it.
    pub(super) fn widget_click(&mut self, i: usize) {
        let Some(v) = self.wviews.get(&i) else { return };
        let (x, y) = self.ptr_xy;
        let (left, top, sc) = self.geo.item_rect(x, if self.hovering { 1.0 } else { 0.0 }, i);
        let k = self.scale * sc;
        let (lx, ly) = ((x - left) / k, (y - top) / k);
        trace!("widget {} clicked at ({lx:.0}, {ly:.0})", v.def.id);
        if let Some((a, frac)) = v.painted.at(lx, ly) {
            self.widget_act(a, frac);
        }
    }

    /// Do what a widget click asks.
    pub(super) fn widget_act(&mut self, a: WAct, frac: f32) {
        trace!("widget action {a:?}");
        match a {
            WAct::Open(target) => {
                self.close_launchpad();
                apps::launch(&target);
            }
            WAct::Connect(conn) => self.open_connect(conn),
            WAct::Search => {
                if !self.launchpad.open {
                    self.open_launchpad();
                }
            }
            WAct::Timer(TimerKind::Stopwatch) => self.hub.stopwatch.toggle(),
            WAct::Timer(k) => {
                if let Some(t) = self.hub.timers.get_mut(&k) {
                    t.toggle();
                }
            }
            WAct::TimerReset(TimerKind::Stopwatch) => self.hub.stopwatch = Default::default(),
            WAct::TimerReset(k) => {
                if let Some(t) = self.hub.timers.get_mut(&k) {
                    let total = match k {
                        TimerKind::Pomodoro => {
                            t.phase = 0;
                            25 * 60
                        }
                        _ => t.total,
                    };
                    t.reset(total);
                }
            }
            WAct::Wifi => self.lp_act(Act::Wifi),
            WAct::WifiMenu => {
                if !self.launchpad.open {
                    self.open_launchpad();
                }
                self.lp_act(Act::WifiMenu);
            }
            WAct::Bluetooth => self.lp_act(Act::Bluetooth),
            WAct::Dark => self.lp_act(Act::Dark),
            WAct::Mute => self.lp_act(Act::Mute),
            WAct::Snip => self.lp_act(Act::Snip),
            WAct::Lock => self.lp_act(Act::Lock),
            WAct::Power => self.lp_act(Act::Power),
            WAct::Tray(key) => self.tray_click(key, crate::tray::Click::Left),
            WAct::Mic => {
                let m = !self.hub.mic_muted.unwrap_or(false);
                self.hub.set_mic(m);
            }
            WAct::KeepAwake => {
                let on = !self.hub.keep_awake;
                self.hub.set_keep_awake(on);
            }
            WAct::ShowDesktop => {
                self.close_launchpad();
                send_keys(&[VK_LWIN, VIRTUAL_KEY(b'D' as u16)]);
                self.hub.desktop_shown = !self.hub.desktop_shown;
            }
            WAct::Language => send_keys(&[VK_LWIN, VK_SPACE]),
            WAct::MediaPrev => self.lp_act(Act::MediaPrev),
            WAct::MediaPlay => self.lp_act(Act::MediaPlay),
            WAct::MediaNext => self.lp_act(Act::MediaNext),
            WAct::Volume => {
                self.control.volume = Some(frac);
                self.services.send(Req::SetVolume(frac));
            }
            WAct::Brightness => self.hub.set_brightness(frac),
            WAct::SetWallpaper => {
                if let Some(p) = self.hub.feed("wallpaper").and_then(|f| f.extra.get("image_path").cloned()) {
                    self.hub.apply_wallpaper(p);
                }
            }
            WAct::App(id) => {
                self.close_launchpad();
                self.open_app(&id);
            }
            WAct::File(path) => {
                self.close_launchpad();
                apps::launch(&path);
            }
            WAct::Notes => {
                self.close_launchpad();
                let p = widgets::hub::notes_path();
                if !p.exists() {
                    let _ = std::fs::create_dir_all(config::dir());
                    let _ = std::fs::write(&p, "Notes\n");
                }
                apps::edit_file(&p);
            }
            WAct::WhatsApp => {
                self.close_launchpad();
                let win = self.windows.iter().find(|w| w.app.contains("whatsapp") || w.exe.contains("whatsapp")).map(|w| w.hwnd);
                match win {
                    Some(h) => tracker::activate(h),
                    None => apps::launch("whatsapp:"),
                }
            }
        }
        self.paint_dock_widgets(true);
        if self.launchpad.open {
            self.lp_render("live");
        }
    }

    /// Once a second: timers, screen time, live widgets, connector refreshes.
    pub(super) fn widget_tick(&mut self) {
        self.wtick += 1;
        let fg_app = self.windows.iter().find(|w| w.hwnd == self.fg).map(|w| {
            self.names.get(&w.app).cloned().unwrap_or_else(|| {
                let base = w.exe.rsplit('\\').next().unwrap_or(&w.exe);
                let base = base.trim_end_matches(".exe");
                let mut ch = base.chars();
                ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default()
            })
        });
        let done = self.hub.tick(fg_app.as_deref(), &Now::get());
        if !done.is_empty() {
            unsafe {
                let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(MB_ICONASTERISK);
            }
            trace!("timers finished: {done:?}");
        }
        // Connectors used by the dock's widgets and (while open) Launchpad's.
        let mut conns: Vec<&'static str> = Vec::new();
        for v in self.wviews.values() {
            conns.extend(widgets::conns(v.def));
        }
        if self.launchpad.open {
            for w in self.cfg.launchpad_widgets.iter().chain(self.cfg.right_widgets.iter()) {
                if let Some(d) = widgets::catalog::get(w.id()) {
                    conns.extend(widgets::conns(d));
                }
            }
            // The gallery's previews show real data where it needs no setup.
            if let Some((c, _, _)) = self.launchpad.gallery.and_then(|k| widgets::catalog::CATS.get(k)) {
                for d in widgets::catalog::in_cat(*c) {
                    conns.extend(widgets::conns(d));
                }
            }
        }
        conns.sort();
        conns.dedup();
        if !conns.is_empty() {
            let cfg = self.cfg.clone();
            self.hub.want(&cfg, &conns);
        }
        // Stats for system widgets in the dock (Launchpad asks for its own while open).
        let needs_stats = self.wviews.values().any(|v| matches!(v.def.face, Face::Sys(SysKind::Resource | SysKind::Cpu | SysKind::Memory | SysKind::Network)));
        if needs_stats && !self.launchpad.open && self.wtick % 2 == 0 {
            self.services.send(Req::Stats);
        }
        // Redraw what changes with time.
        let t = self.wtick;
        let running = |d: &widgets::Def, hub: &widgets::Hub| match d.face {
            Face::Timer(TimerKind::Stopwatch) => hub.stopwatch.start.is_some(),
            Face::Timer(k) => hub.timers.get(&k).map(|c| c.running()).unwrap_or(false) || !done.is_empty(),
            _ => true,
        };
        let due: Vec<usize> = self
            .wviews
            .iter()
            .filter(|(_, v)| {
                let live = v.def.face.live_secs() as u64;
                live > 0 && t % live == 0 && running(v.def, &self.hub)
            })
            .map(|(i, _)| *i)
            .collect();
        self.paint_dock_set(&due);
        if self.launchpad.open && self.lp_drag.is_none() && !self.launchpad.dragging() {
            self.lp_render("live");
        }
    }

    pub(super) fn on_widget_replies(&mut self) {
        // Collect finished jobs first: a place lookup only lands in `geocoded` here.
        let changed = self.hub.take_replies();
        let geo = self.hub.geocoded.take();
        if let Some((city, res)) = geo {
            // Only the answer this open form is waiting for (not one from a cancelled or
            // earlier lookup).
            let weather_form = self.launchpad.form.as_ref().map(|f| f.conn == "weather" && f.busy.is_some() && f.pending.as_deref() == Some(city.as_str())).unwrap_or(false);
            match res {
                // The form was cancelled while looking up: leave the location as it was.
                _ if !weather_form => trace!("weather lookup for {city} ignored (form closed)"),
                Some((lat, lon, label)) => {
                    trace!("weather location: {label}");
                    let name = label.split(',').next().unwrap_or(&city).trim().to_string();
                    self.save_and_apply(|c| {
                        c.weather_city = name;
                        c.weather_lat = lat;
                        c.weather_lon = lon;
                    });
                    self.services.send(Req::Weather(lat, lon));
                    self.launchpad.form = None;
                }
                None => {
                    if let Some(f) = self.launchpad.form.as_mut() {
                        f.busy = None;
                        f.pending = None;
                        f.error = Some(format!("Couldn’t find “{city}”. Check the spelling or try a nearby city (an internet connection is needed)."));
                    }
                }
            }
            if self.launchpad.open {
                self.lp_render("all");
            }
        }
        if changed {
            self.paint_dock_widgets(true);
            if self.launchpad.open && self.lp_drag.is_none() && !self.launchpad.dragging() {
                self.lp_render("live");
                if self.launchpad.gallery.is_some() || self.launchpad.settings.is_some() || self.launchpad.form.is_some() {
                    self.lp_render("center");
                }
            }
        }
    }

    // ------------------------------------------------------------ adding, removing, moving

    pub(super) fn add_widget(&mut self, id: &'static str, to_dock: bool) {
        trace!("add widget {id} to {}", if to_dock { "dock" } else { "launchpad" });
        if to_dock {
            self.save_and_apply(|c| c.pins.push(Pin::Widget { widget: id.to_string(), size: None }));
        } else {
            self.save_and_apply(|c| {
                if !c.launchpad_widgets.iter().chain(c.right_widgets.iter()).any(|w| w.id() == id) {
                    c.launchpad_widgets.push(WRef::Id(id.to_string()));
                }
            });
        }
        if self.launchpad.open {
            self.lp_render("all");
        }
    }

    pub(super) fn remove_lp_widget(&mut self, id: &str) {
        self.save_and_apply(|c| {
            c.launchpad_widgets.retain(|w| w.id() != id);
            c.right_widgets.retain(|w| w.id() != id);
        });
        if self.launchpad.open {
            self.lp_render("all");
        }
    }

    fn column_mut(c: &mut Config, ci: usize) -> &mut Vec<WRef> {
        if ci == 0 {
            &mut c.launchpad_widgets
        } else {
            &mut c.right_widgets
        }
    }

    fn column(&self, ci: usize) -> &Vec<WRef> {
        if ci == 0 {
            &self.cfg.launchpad_widgets
        } else {
            &self.cfg.right_widgets
        }
    }

    // ------------------------------------------------------------ dragging from Launchpad

    /// The window y (Launchpad coordinates) where the dock begins.
    pub(super) fn dock_top_in_launchpad(&self) -> f32 {
        let lp_top = self.monitor.top as f32;
        let dock_top = (self.monitor.bottom - self.geo.win_h as i32) as f32 + self.geo.y0 - 12.0 * self.scale;
        dock_top - lp_top
    }

    /// A press on a widget card or a gallery preview turned into a drag.
    pub(super) fn lp_begin_drag(&mut self, src: DragSrc, x: f32, y: f32) {
        let pinned = self.lp_pinned();
        let tray = self.tray_views();
        let tray_list: Vec<(u64, String)> = tray.iter().map(|t| (t.key, t.tip.clone())).collect();
        let wa = self.whatsapp_state();
        let app_version = crate::updater::version_label();
        let app_update = self.update_status_line();
        let app_update_ver = self.app_update.clone();
        let lp_wins = self.lp_windows();
        let data = lp::Data {
            apps: &self.lp_apps,
            brushes: &self.brushes,
            pinned: &pinned,
            names: &self.names,
            control: &self.control,
            stats: self.stats.as_ref(),
            weather: self.weather.as_ref(),
            files: &self.files,
            cfg: &self.cfg,
            user: &self.user_name,
            wifi: self.wifi.as_ref(),
            tray: &tray,
            tray_list: &tray_list,
            update: self.update.as_ref(),
            app_version: &app_version,
            app_update: &app_update,
            update_ui: Self::update_ui_for(&app_update_ver),
            hub: &self.hub,
            whatsapp: wa,
            windows: &lp_wins,
            history: &self.history,
        };
        let c = self.compositor.clone();
        if let Err(e) = self.launchpad.begin_drag(&c, &self.gfx, &data, src, x, y) {
            trace!("widget drag failed: {e:?}");
        }
        trace!("widget drag start: {src:?}");
        unsafe {
            SetCapture(self.launchpad.hwnd());
            SetCursor(LoadCursorW(None, IDC_SIZEALL).ok());
        }
    }

    /// The pointer moved during a Launchpad widget drag.
    pub(super) fn lp_drag_move(&mut self, x: f32, y: f32) {
        let c = self.compositor.clone();
        let zone = self.launchpad.drag_move(&c, x, y, self.dock_top_in_launchpad());
        unsafe {
            SetCursor(LoadCursorW(None, IDC_SIZEALL).ok());
        }
        // Over the dock: the dock opens a gap and shows its own floating copy.
        let in_dock = matches!(&self.drag, Some(d) if d.ext_widget.is_some());
        if zone == Zone::Dock {
            if !in_dock {
                if let Some((_, def, size)) = self.launchpad.drag_info() {
                    self.start_widget_drag(def, size);
                }
            }
            let (sx, sy) = (x + self.monitor.left as f32, y + self.monitor.top as f32);
            let win_top = (self.monitor.bottom - self.geo.win_h as i32) as f32;
            self.drag_move(sx - self.monitor.left as f32, sy - win_top);
        } else if in_dock {
            if let Some(d) = self.drag.take() {
                let id = d.ext.clone().unwrap_or_default();
                self.end_external_drag(d, &id, false);
            }
        }
    }

    /// A widget from Launchpad enters the dock: open a gap for it and show it under the pointer.
    fn start_widget_drag(&mut self, def: &'static widgets::Def, size: Size) {
        let ww = widgets::dock_width(def, size, self.geo.b);
        let width = ww + self.geo.g;
        if let Err(e) = self.show_widget_ghost(def, size, ww) {
            trace!("widget ghost failed: {e:?}");
        }
        self.drag = Some(Drag { index: usize::MAX, insert: None, removing: false, width, merge: None, merge_ready: false, ext: Some(format!("::widget:{}", def.id)), ext_widget: Some(Pin::Widget { widget: def.id.to_string(), size: (size != def.size).then_some(size) }) });
        if self.tucked {
            self.set_tucked(false);
        }
        self.hovering = true;
        let _ = self.animate_amount(0.0, 120);
        self.apply_region();
    }

    /// The floating copy of a widget while it's over the dock.
    fn show_widget_ghost(&mut self, def: &'static widgets::Def, size: Size, ww: f32) -> Result<()> {
        let c = self.compositor.clone();
        if self.ghost.is_none() {
            let popup = crate::ui::Popup::new(&c, false)?;
            let body = c.CreateContainerVisual()?;
            popup.root.Children()?.InsertAtTop(&body)?;
            self.ghost = Some(Ghost { popup, body, size: 0 });
        }
        let s = self.scale;
        let h = self.geo.b;
        let surface = self.gfx.surface(ww, h)?;
        let pinned = self.lp_pinned();
        {
            let ctx = widgets::Ctx {
                hub: &self.hub,
                cfg: &self.cfg,
                weather: self.weather.as_ref(),
                stats: self.stats.as_ref(),
                control: &self.control,
                pinned: &pinned,
                names: &self.names,
                whatsapp: None,
                tray: &[],
                hover_tray: None,
                update: self.update.as_ref(),
                sample: true,
                now: Now::get(),
            };
            widgets::render(&self.gfx, &surface, def, &ctx, ww, h, s, true)?;
        }
        let _ = size;
        let side = (ww.max(h) + 24.0 * s).ceil();
        let g = self.ghost.as_mut().unwrap();
        g.body.Children()?.RemoveAll()?;
        g.size = side as i32;
        g.body.SetSize(v2(side, side))?;
        g.body.SetOpacity(0.95)?;
        g.body.SetScale(v3(1.0, 1.0, 1.0))?;
        let sp = crate::ui::surface_sprite(&c, &surface, ww, h)?;
        sp.SetOffset(v3((side - ww) / 2.0, (side - h) / 2.0, 0.0))?;
        g.body.Children()?.InsertAtTop(&sp)?;
        let mut p = POINT::default();
        let _ = get_cursor_pos(&mut p);
        let half = (side / 2.0) as i32;
        g.popup.show(RECT { left: p.x - half, top: p.y - half, right: p.x + half, bottom: p.y + half }, false);
        Ok(())
    }

    /// The button was released during a Launchpad widget drag: put the widget where it landed.
    pub(super) fn lp_end_drag(&mut self) {
        unsafe {
            let _ = ReleaseCapture();
        }
        let dock_drop = matches!(&self.drag, Some(d) if d.ext_widget.is_some());
        let Some((src, def, size, zone)) = self.launchpad.end_drag() else { return };
        trace!("widget drag end: {src:?} -> {zone:?}");
        if dock_drop && zone == Zone::Dock {
            if let Some(d) = self.drag.take() {
                let id = d.ext.clone().unwrap_or_default();
                self.end_external_drag(d, &id, true);
            }
            // Moved out of a column: it now lives in the dock.
            if let DragSrc::Col((ci, i)) = src {
                self.save_and_apply(|c| {
                    let col = Self::column_mut(c, ci);
                    if i < col.len() {
                        col.remove(i);
                    }
                });
            }
        } else if let Zone::Column(ci, k) = zone {
            let entry = WRef::new(def.id, size);
            self.save_and_apply(|c| {
                // Control Center widgets exist once: a gallery drop moves the existing one.
                let single = widgets::catalog::CONTROL.iter().any(|d| d.id == def.id);
                let src = match src {
                    DragSrc::Col(p) => Some(p),
                    DragSrc::Gallery(_) if single => (0..2).find_map(|cc| Self::column_mut(c, cc).iter().position(|w| w.id() == def.id).map(|i| (cc, i))),
                    DragSrc::Gallery(_) => None,
                };
                let item = match src {
                    Some((sc, si)) if si < Self::column_mut(c, sc).len() => Self::column_mut(c, sc).remove(si),
                    _ => entry,
                };
                let col = Self::column_mut(c, ci);
                col.insert(k.min(col.len()), item);
            });
        } else if dock_drop {
            if let Some(d) = self.drag.take() {
                let id = d.ext.clone().unwrap_or_default();
                self.end_external_drag(d, &id, false);
            }
        }
        if self.launchpad.open {
            self.lp_render("all");
        }
    }

    /// Show the connect form for a connector in Launchpad (opening it if needed).
    pub(super) fn open_connect(&mut self, conn: &str) {
        if !self.launchpad.open {
            self.open_launchpad();
        }
        self.launchpad.open_form(conn, &self.cfg);
        self.lp_render("all");
    }

    /// Save the open connect form: plain settings to config.json, secrets to Credential Manager.
    pub(super) fn save_form(&mut self) {
        let Some(form) = self.launchpad.form.clone() else { return };
        if form.busy.is_some() {
            return;
        }
        let mut missing = Vec::new();
        for f in &form.fields {
            let have = !f.value.trim().is_empty() || (f.secret && f.saved);
            if f.required && !have {
                missing.push(f.label);
            }
        }
        if !missing.is_empty() {
            if let Some(fm) = self.launchpad.form.as_mut() {
                fm.error = Some(format!("Still needed: {}", missing.join(", ")));
            }
            self.lp_render("center");
            return;
        }
        // Weather: look the place up first (on a worker thread). The form stays open saying
        // so; it closes when the place is found, or shows why not.
        if form.conn == "weather" {
            let city = form.fields.first().map(|f| f.value.trim().to_string()).unwrap_or_default();
            if let Some(fm) = self.launchpad.form.as_mut() {
                fm.error = None;
                fm.busy = Some(format!("Looking up “{city}”…"));
                fm.pending = Some(city.clone());
            }
            self.hub.geocode(city);
            self.lp_render("center");
            return;
        }
        for f in form.fields.iter().filter(|f| f.secret && !f.value.trim().is_empty()) {
            if !laminar_sys::secrets::set(&form.conn, f.key, f.value.trim()) {
                if let Some(fm) = self.launchpad.form.as_mut() {
                    fm.error = Some("Windows Credential Manager refused to save the key.".into());
                }
                self.lp_render("center");
                return;
            }
        }
        let conn = form.conn.clone();
        self.save_and_apply(|c| {
            let m = c.widget_settings.entry(conn.clone()).or_default();
            for f in form.fields.iter().filter(|f| !f.secret) {
                if f.value.trim().is_empty() {
                    m.remove(f.key);
                } else {
                    m.insert(f.key.to_string(), f.value.trim().to_string());
                }
            }
            if m.is_empty() {
                c.widget_settings.remove(&conn);
            }
        });
        trace!("widget settings saved: {}", form.conn);
        self.hub.reset(&form.conn);
        let cfg = self.cfg.clone();
        if let Some(info) = laminar_sys::connectors::info(&form.conn) {
            self.hub.want(&cfg, &[info.id]);
        }
        self.launchpad.form = None;
        self.lp_render("all");
        self.paint_dock_widgets(true);
    }

    /// Forget a connector's saved keys and settings.
    pub(super) fn disconnect_form(&mut self) {
        let Some(form) = self.launchpad.form.clone() else { return };
        for f in form.fields.iter().filter(|f| f.secret) {
            laminar_sys::secrets::delete(&form.conn, f.key);
        }
        let conn = form.conn.clone();
        self.save_and_apply(|c| {
            c.widget_settings.remove(&conn);
        });
        self.hub.reset(&form.conn);
        self.launchpad.form = None;
        self.lp_render("all");
        self.paint_dock_widgets(true);
    }

    // ------------------------------------------------------------ menus

    fn size_items(&self, def: &widgets::Def, cur: Size) -> Vec<MenuItem> {
        let _ = def;
        Size::ALL.iter().map(|s| MenuItem::item(s.label(), CMD_SIZE_BASE + size_code(*s)).checked(*s == cur)).collect()
    }

    /// Right-click on a Launchpad widget card.
    pub(super) fn lp_widget_menu(&mut self, p: WPos) {
        let Some(w) = self.column(p.0).get(p.1).cloned() else { return };
        let Some(def) = widgets::catalog::get(w.id()) else { return };
        self.menu_lpw = Some(p);
        self.menu_widget = None;
        let mut items = vec![MenuItem::header(def.name.to_string())];
        if widgets::conns(def).iter().any(|c| laminar_sys::connectors::info(c).map(|i| !i.fields.is_empty()).unwrap_or(false)) {
            items.push(MenuItem::item("Connect…", CMD_LPW_CONNECT));
        }
        if let Some((_, label)) = widgets::local_form(def) {
            items.push(MenuItem::item(label, CMD_LPW_LOCAL));
        }
        if matches!(def.face, Face::Timer(_)) {
            items.push(MenuItem::item("Reset", CMD_LPW_RESET));
        }
        if matches!(def.face, Face::Timer(TimerKind::Countdown)) {
            let cur = self.hub.timers.get(&TimerKind::Countdown).map(|t| t.total as usize / 60).unwrap_or(0);
            for mins in TIMER_MINUTES {
                items.push(MenuItem::item(format!("{mins} min"), CMD_TIMER_MIN_BASE + mins).checked(mins == cur));
            }
        }
        items.push(MenuItem::sep());
        items.extend(self.size_items(def, w.size()));
        items.push(MenuItem::sep());
        if p.1 > 0 {
            items.push(MenuItem::item("Move Up", CMD_LPW_UP));
        }
        if p.1 + 1 < self.column(p.0).len() {
            items.push(MenuItem::item("Move Down", CMD_LPW_DOWN));
        }
        items.push(MenuItem::item(if p.0 == 0 { "Move to Right Column" } else { "Move to Left Column" }, CMD_LPW_OTHER_COL));
        let pinned = widgets::catalog::PINNED.iter().any(|(_, id)| *id == def.id);
        if !pinned {
            items.push(MenuItem::item("Add to Dock", CMD_LPW_TO_DOCK));
            items.push(MenuItem::sep());
            items.push(MenuItem::item("Remove Widget", CMD_LPW_REMOVE).danger());
        } else {
            items.push(MenuItem::sep());
            items.push(MenuItem::item("Turn Off in Settings…", CMD_LPW_REMOVE));
        }
        self.menu_ctx = None;
        self.menu_open = true;
        let mut pt = POINT::default();
        let _ = get_cursor_pos(&mut pt);
        let _ = self.menu.show(&self.compositor, &self.gfx, &self.backdrop, items, pt, false, self.scale, self.monitor);
    }

    /// Size items for a dock widget's menu.
    pub(super) fn dock_size_items(&self, key: &str) -> Vec<MenuItem> {
        match widget_of(key) {
            Some(def) => self.size_items(def, widget_size(key)),
            None => Vec::new(),
        }
    }

    /// Menu commands for widgets. Returns true if handled.
    pub(super) fn widget_menu_command(&mut self, cmd: usize) -> bool {
        let dock_def = self.menu_widget.and_then(|n| match self.cfg.pins.get(n) {
            Some(Pin::Widget { widget, .. }) => widgets::catalog::get(widget),
            _ => None,
        });
        let lp = self.menu_lpw;
        let lp_def = lp.and_then(|p| self.column(p.0).get(p.1)).and_then(|w| widgets::catalog::get(w.id()));
        let first_conn = |d: &widgets::Def| widgets::conns(d).into_iter().find(|c| laminar_sys::connectors::info(c).map(|i| !i.fields.is_empty()).unwrap_or(false));
        match cmd {
            CMD_WIDGET_REMOVE => {
                if let Some(n) = self.menu_widget.take() {
                    self.save_and_apply(|c| {
                        if matches!(c.pins.get(n), Some(Pin::Widget { .. })) {
                            c.pins.remove(n);
                        }
                    });
                }
            }
            CMD_WIDGET_TO_LP => {
                if let Some(d) = dock_def {
                    if self.cfg.launchpad_widgets.iter().chain(self.cfg.right_widgets.iter()).any(|w| w.id() == d.id) {
                        self.remove_lp_widget(d.id);
                    } else {
                        self.add_widget(d.id, false);
                    }
                }
            }
            CMD_WIDGET_CONNECT => {
                if let Some(c) = dock_def.and_then(first_conn) {
                    self.open_connect(c);
                }
            }
            CMD_WIDGET_LOCAL => {
                if let Some((form, _)) = dock_def.and_then(widgets::local_form) {
                    self.open_connect(form);
                }
            }
            CMD_LPW_LOCAL => {
                if let Some((form, _)) = lp_def.and_then(widgets::local_form) {
                    self.open_connect(form);
                }
            }
            CMD_WIDGET_RESET => {
                if let Some(Face::Timer(k)) = dock_def.map(|d| d.face) {
                    self.widget_act(WAct::TimerReset(k), 0.0);
                }
            }
            c if (CMD_SIZE_BASE..CMD_SIZE_BASE + 3).contains(&c) => {
                let size = Size::ALL[c - CMD_SIZE_BASE];
                if let Some(n) = self.menu_widget {
                    self.save_and_apply(|cfg| {
                        if let Some(Pin::Widget { widget, size: sz }) = cfg.pins.get_mut(n) {
                            let default = widgets::catalog::get(widget).map(|d| d.size);
                            *sz = (Some(size) != default).then_some(size);
                        }
                    });
                } else if let Some((ci, i)) = lp {
                    self.save_and_apply(|cfg| {
                        let col = Self::column_mut(cfg, ci);
                        if let Some(w) = col.get_mut(i) {
                            *w = WRef::new(&w.id().to_string(), size);
                        }
                    });
                    self.lp_render("all");
                }
            }
            CMD_LPW_OTHER_COL => {
                if let Some((ci, i)) = lp {
                    self.save_and_apply(|cfg| {
                        let from = Self::column_mut(cfg, ci);
                        if i < from.len() {
                            let w = from.remove(i);
                            Self::column_mut(cfg, 1 - ci).push(w);
                        }
                    });
                    self.lp_render("all");
                }
            }
            CMD_LPW_REMOVE => {
                if let Some(d) = lp_def {
                    if widgets::catalog::PINNED.iter().any(|(_, id)| *id == d.id) {
                        self.lp_act(Act::OpenSettings);
                        self.lp_act(Act::Section(4));
                    } else if let Some((ci, i)) = lp {
                        self.save_and_apply(|cfg| {
                            let col = Self::column_mut(cfg, ci);
                            if i < col.len() {
                                col.remove(i);
                            }
                        });
                        self.lp_render("all");
                    }
                }
            }
            CMD_LPW_TO_DOCK => {
                if let Some(d) = lp_def {
                    self.add_widget(d.id, true);
                }
            }
            CMD_LPW_CONNECT => {
                if let Some(c) = lp_def.and_then(first_conn) {
                    self.open_connect(c);
                }
            }
            CMD_LPW_RESET => {
                if let Some(Face::Timer(k)) = lp_def.map(|d| d.face) {
                    self.widget_act(WAct::TimerReset(k), 0.0);
                }
            }
            c if c >= CMD_TIMER_MIN_BASE && TIMER_MINUTES.contains(&(c - CMD_TIMER_MIN_BASE)) => {
                let mins = (c - CMD_TIMER_MIN_BASE) as u32;
                if let Some(t) = self.hub.timers.get_mut(&TimerKind::Countdown) {
                    t.reset(mins * 60);
                }
                self.paint_dock_widgets(true);
                if self.launchpad.open {
                    self.lp_render("live");
                }
            }
            CMD_LPW_UP | CMD_LPW_DOWN => {
                if let Some((ci, i)) = lp {
                    let j = if cmd == CMD_LPW_UP { i.wrapping_sub(1) } else { i + 1 };
                    if j < self.column(ci).len() {
                        self.save_and_apply(|c| Self::column_mut(c, ci).swap(i, j));
                        self.lp_render("all");
                    }
                }
            }
            _ => return false,
        }
        self.menu_widget = None;
        self.menu_lpw = None;
        true
    }
}

/// Press and release keys (a shortcut such as Win+D), in order then in reverse.
fn send_keys(keys: &[VIRTUAL_KEY]) {
    let mut inputs = Vec::new();
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } },
    };
    for k in keys {
        inputs.push(key(*k, false));
    }
    for k in keys.iter().rev() {
        inputs.push(key(*k, true));
    }
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
