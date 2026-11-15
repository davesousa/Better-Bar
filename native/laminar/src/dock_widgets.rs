//! The dock's side of widgets: painting dock widgets, routing clicks from the dock and
//! Launchpad, the once-a-second refresh, widget menus and the connect form.
//! (A child module of `dock`, so it can use the dock's private state.)

use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use crate::widgets::catalog::{Face, SysKind, TimerKind};
use crate::widgets::{self, Now, WAct};

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
        if idx.iter().any(|i| self.wviews.get(i).map(|v| matches!(v.def.face, Face::Tool(widgets::catalog::ToolKind::Apps))).unwrap_or(false)) {
            self.lp_brushes(&pinned);
        }
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
                sample: false,
                now: Now::get(),
            };
            for i in idx {
                let Some(v) = self.wviews.get(i) else { continue };
                match widgets::render(&self.gfx, &v.surface, v.def, &ctx, v.w, v.h, self.scale, true) {
                    Ok(p) => results.push((*i, p)),
                    Err(e) => trace!("widget {} paint failed: {e:?}", v.def.id),
                }
            }
        }
        let c = self.compositor.clone();
        for (i, p) in results {
            let Some(v) = self.wviews.get_mut(&i) else { continue };
            // App icons sit on top of the painted bitmap (they're Composition brushes).
            if let Ok(kids) = v.tile.Children() {
                while kids.Count().unwrap_or(0) > 1 {
                    if let Ok(top) = kids.First().and_then(|it| {
                        let mut last = None;
                        for k in it {
                            last = Some(k);
                        }
                        last.ok_or_else(windows::core::Error::empty)
                    }) {
                        let _ = kids.Remove(&top);
                    } else {
                        break;
                    }
                }
                for (x, y, size, id) in &p.icons {
                    if let Some(b) = self.brushes.get(id) {
                        if let Ok(sp) = c.CreateSpriteVisual() {
                            let _ = sp.SetBrush(b);
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
        let left = self.geo.item_left(x, if self.hovering { 1.0 } else { 0.0 }, i);
        let (lx, ly) = ((x - left) / self.scale, (y - (self.geo.y0 + self.geo.p)) / self.scale);
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
            WAct::Launchpad => self.toggle_launchpad(),
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
            WAct::Bluetooth => self.lp_act(Act::Bluetooth),
            WAct::Dark => self.lp_act(Act::Dark),
            WAct::Mute => self.lp_act(Act::Mute),
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
            self.lp_render("left");
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
            for id in &self.cfg.launchpad_widgets {
                if let Some(d) = widgets::catalog::get(id) {
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
        if self.launchpad.open && self.lp_drag.is_none() {
            self.lp_render("live");
        }
    }

    pub(super) fn on_widget_replies(&mut self) {
        if self.hub.take_replies() {
            self.paint_dock_widgets(true);
            if self.launchpad.open && self.lp_drag.is_none() {
                self.lp_render("left");
                if self.launchpad.gallery.is_some() {
                    self.lp_render("center");
                }
            }
        }
    }

    // ------------------------------------------------------------ adding, removing, moving

    pub(super) fn add_widget(&mut self, id: &'static str, to_dock: bool) {
        trace!("add widget {id} to {}", if to_dock { "dock" } else { "launchpad" });
        if to_dock {
            self.save_and_apply(|c| {
                // Before the trailing separators (at the end of the pinned apps).
                c.pins.push(Pin::Widget { widget: id.to_string() });
            });
        } else {
            self.save_and_apply(|c| {
                if !c.launchpad_widgets.iter().any(|w| w == id) {
                    c.launchpad_widgets.push(id.to_string());
                }
            });
        }
        if self.launchpad.open {
            self.lp_render("all");
        }
    }

    pub(super) fn remove_lp_widget(&mut self, id: &str) {
        self.save_and_apply(|c| c.launchpad_widgets.retain(|w| w != id));
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
        trace!("connector {} settings saved", form.conn);
        self.hub.reset(&form.conn);
        let cfg = self.cfg.clone();
        let id: &str = &form.conn;
        if let Some(info) = laminar_sys::connectors::info(id) {
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

    /// Right-click on a Launchpad widget card.
    pub(super) fn lp_widget_menu(&mut self, i: usize) {
        let Some(def) = self.cfg.launchpad_widgets.get(i).and_then(|id| widgets::catalog::get(id)) else { return };
        self.menu_lpw = Some(i);
        let mut items = vec![MenuItem::header(def.name.to_string())];
        if widgets::conns(def).iter().any(|c| laminar_sys::connectors::info(c).map(|i| !i.fields.is_empty()).unwrap_or(false)) {
            items.push(MenuItem::item("Connect…", CMD_LPW_CONNECT));
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
        if i > 0 {
            items.push(MenuItem::item("Move Up", CMD_LPW_UP));
        }
        if i + 1 < self.cfg.launchpad_widgets.len() {
            items.push(MenuItem::item("Move Down", CMD_LPW_DOWN));
        }
        items.push(MenuItem::item("Add to Dock", CMD_LPW_TO_DOCK));
        items.push(MenuItem::sep());
        items.push(MenuItem::item("Remove Widget", CMD_LPW_REMOVE).danger());
        self.menu_ctx = None;
        self.menu_open = true;
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let _ = self.menu.show(&self.compositor, &self.gfx, &self.backdrop, items, p, false, self.scale, self.monitor);
    }

    /// Menu commands for widgets. Returns true if handled.
    pub(super) fn widget_menu_command(&mut self, cmd: usize) -> bool {
        let dock_def = self.menu_widget.and_then(|n| match self.cfg.pins.get(n) {
            Some(Pin::Widget { widget }) => widgets::catalog::get(widget),
            _ => None,
        });
        let lp_def = self.menu_lpw.and_then(|i| self.cfg.launchpad_widgets.get(i)).and_then(|id| widgets::catalog::get(id));
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
                    if self.cfg.launchpad_widgets.iter().any(|w| w == d.id) {
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
            CMD_WIDGET_RESET => {
                if let Some(Face::Timer(k)) = dock_def.map(|d| d.face) {
                    self.widget_act(WAct::TimerReset(k), 0.0);
                }
            }
            CMD_LPW_RESET => {
                if let Some(Face::Timer(k)) = lp_def.map(|d| d.face) {
                    self.widget_act(WAct::TimerReset(k), 0.0);
                }
            }
            c if (CMD_TIMER_MIN_BASE..CMD_TIMER_MIN_BASE + 1000).contains(&c) => {
                let mins = (c - CMD_TIMER_MIN_BASE) as u32;
                if let Some(t) = self.hub.timers.get_mut(&TimerKind::Countdown) {
                    t.reset(mins * 60);
                }
                self.paint_dock_widgets(true);
                if self.launchpad.open {
                    self.lp_render("left");
                }
            }
            CMD_LPW_REMOVE => {
                if let Some(d) = lp_def {
                    self.remove_lp_widget(d.id);
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
            CMD_LPW_UP | CMD_LPW_DOWN => {
                if let Some(i) = self.menu_lpw {
                    let j = if cmd == CMD_LPW_UP { i.wrapping_sub(1) } else { i + 1 };
                    if j < self.cfg.launchpad_widgets.len() {
                        self.save_and_apply(|c| c.launchpad_widgets.swap(i, j));
                        self.lp_render("left");
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
