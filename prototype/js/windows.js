/* Laminar Dock prototype — a tiny window manager that simulates the real
   Windows desktop: open, focus, minimize (into the dock icon), restore,
   maximize, close, drag, snap-to-top, and "peek". */
(function (LD) {
  'use strict';
  const desk = document.getElementById('desktop');
  const wins = [];
  let z = 20, uid = 0, focusCounter = 0;
  let focusedId = null;

  LD.badges = { chat: 3 };

  const SIZES = {
    notes: [740, 480], files: [780, 470], browser: [860, 540], music: [660, 400],
    chat: [700, 440], laminar: [880, 600],
  };

  function workArea() {
    const s = LD.settings;
    const reserve = s.autoHide ? 0 : (s.iconSize + s.dockPadding * 2 + 16);
    return { x: 0, y: 0, w: innerWidth, h: innerHeight - reserve };
  }

  const CAPS = `
    <button class="cap min" title="Minimize"><svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1"/></svg></button>
    <button class="cap max" title="Maximize"><svg width="10" height="10" viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" stroke-width="1"/></svg></button>
    <button class="cap close" title="Close"><svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1"/></svg></button>`;

  function get(id) { return wins.find(w => w.id === id); }

  function build(w, o) {
    const tpl = LD.templates[w.app] || LD.templates.generic;
    const el = document.createElement('div');
    el.className = 'win';
    el.dataset.id = w.id;
    el.innerHTML = `<div class="win-bar">
        <span class="win-ico">${LD.icon(w.app)}</span>
        <span class="win-title">${LD.esc(w.title)}</span>
        <div class="win-caps">${CAPS}</div>
      </div>
      <div class="win-body">${tpl.html(w, o)}</div>`;

    el.addEventListener('pointerdown', () => { if (focusedId !== w.id) focus(w.id); }, true);
    el.querySelector('.cap.min').addEventListener('click', () => minimize(w.id));
    el.querySelector('.cap.max').addEventListener('click', () => toggleMax(w.id));
    el.querySelector('.cap.close').addEventListener('click', () => close(w.id));

    const bar = el.querySelector('.win-bar');
    bar.addEventListener('dblclick', e => { if (!e.target.closest('.cap')) toggleMax(w.id); });
    bar.addEventListener('pointerdown', e => {
      if (e.button !== 0 || e.target.closest('.cap')) return;
      const sx = e.clientX, sy = e.clientY;
      let offX = sx - w.x, offY = sy - w.y, moved = false;
      const move = ev => {
        const dx = ev.clientX - sx, dy = ev.clientY - sy;
        if (!moved && Math.hypot(dx, dy) < 4) return;
        if (!moved && w.max) {            // drag a maximized window: restore under the pointer
          const ratio = sx / innerWidth;
          w.max = false;
          offX = w.w * ratio; offY = sy;
        }
        moved = true;
        w.x = Math.round(ev.clientX - offX);
        w.y = Math.round(Math.max(0, ev.clientY - offY));
        place(w);
      };
      const up = ev => {
        removeEventListener('pointermove', move);
        removeEventListener('pointerup', up);
        if (moved && ev.clientY <= 2) { toggleMax(w.id, true); }  // snap to top = maximize
      };
      addEventListener('pointermove', move);
      addEventListener('pointerup', up);
    });

    if (tpl.mount) tpl.mount(el.querySelector('.win-body'), w);
    return el;
  }

  function rectOf(w) {
    if (w.max) { const a = workArea(); return { x: a.x, y: a.y, w: a.w, h: a.h }; }
    return { x: w.x, y: w.y, w: w.w, h: w.h };
  }

  function place(w) {
    const r = rectOf(w);
    Object.assign(w.el.style, { left: r.x + 'px', top: r.y + 'px', width: r.w + 'px', height: r.h + 'px' });
    w.el.classList.toggle('max', !!w.max);
  }

  function open(appId, o = {}) {
    const app = LD.APPS[appId] || { name: 'App' };
    const tpl = LD.templates[appId] || LD.templates.generic;
    const wa = workArea();
    const sz = SIZES[appId] || [720, 460];
    const ww = Math.min(o.w || sz[0], wa.w - 60), hh = Math.min(o.h || sz[1], wa.h - 60);
    const n = wins.length % 6;
    const w = {
      id: ++uid, app: appId,
      title: o.title || (tpl.title ? tpl.title(o) : app.name),
      x: o.x != null ? o.x : Math.round((wa.w - ww) / 2 + n * 26 - 60),
      y: o.y != null ? o.y : Math.round(Math.max(20, (wa.h - hh) / 2 - 30 + n * 22)),
      w: ww, h: hh, min: false, max: !!o.max, lastFocus: 0,
    };
    w.el = build(w, o);
    desk.appendChild(w.el);
    wins.push(w);
    place(w);
    if (o.minimized) {
      w.min = true;
      w.el.classList.add('minimized');
      w.el.style.zIndex = ++z;
    } else {
      if (!o.silent) {
        w.el.animate([{ opacity: 0, transform: 'scale(.94) translateY(14px)' }, { opacity: 1, transform: 'none' }],
          { duration: LD.ms(420), easing: LD.springEasing || 'ease-out' });
      }
      focus(w.id, true);
    }
    LD.emit('windows');
    return w;
  }

  function focus(id, quiet) {
    const w = get(id);
    if (!w) return;
    if (w.min) { restore(id); return; }
    w.el.style.zIndex = ++z;
    w.lastFocus = ++focusCounter;
    focusedId = id;
    wins.forEach(x => x.el.classList.toggle('focused', x.id === id));
    if (LD.badges[w.app]) { setTimeout(() => { delete LD.badges[w.app]; LD.emit('windows'); }, 900); }
    if (!quiet) LD.emit('windows');
  }

  function focusTopmost() {
    const vis = wins.filter(w => !w.min).sort((a, b) => (+b.el.style.zIndex) - (+a.el.style.zIndex));
    if (vis[0]) focus(vis[0].id, true);
    else { focusedId = null; wins.forEach(x => x.el.classList.remove('focused')); }
  }

  function genieFrames(w, r) {
    const a = rectOf(w);
    const tx = r.left + r.width / 2 - (a.x + a.w / 2);
    const ty = r.top + r.height / 2 - (a.y + a.h / 2);
    const sx = r.width / a.w, sy = r.height / a.h;
    if (LD.settings.minimizeEffect === 'genie') {
      return [
        { transform: 'none', opacity: 1, offset: 0 },
        { transform: `translate(${tx * 0.16}px, ${ty * 0.4}px) scale(${Math.max(sx * 3, 0.52)}, .84)`, opacity: 1, offset: 0.4 },
        { transform: `translate(${tx * 0.8}px, ${ty * 0.88}px) scale(${sx * 1.5}, ${Math.max(sy * 1.6, 0.16)})`, opacity: 0.85, offset: 0.8 },
        { transform: `translate(${tx}px, ${ty}px) scale(${sx}, ${sy})`, opacity: 0, offset: 1 },
      ];
    }
    return [
      { transform: 'none', opacity: 1 },
      { transform: `translate(${tx}px, ${ty}px) scale(${sx}, ${sy})`, opacity: 0 },
    ];
  }

  function minimize(id) {
    const w = get(id);
    if (!w || w.min) return;
    w.min = true;
    w.el.classList.remove('focused');
    if (focusedId === id) { focusedId = null; focusTopmost(); }
    LD.emit('windows');
    const r = LD.dock.iconRect(w.app);
    w.el.style.transformOrigin = 'center';
    const anim = w.el.animate(genieFrames(w, r), { duration: LD.ms(500), easing: 'cubic-bezier(.5,0,.3,1)' });
    anim.onfinish = () => { w.el.classList.add('minimized'); };
  }

  function restore(id) {
    const w = get(id);
    if (!w || !w.min) return;
    w.min = false;
    w.el.classList.remove('minimized');
    const r = LD.dock.iconRect(w.app);
    const f = genieFrames(w, r).reverse().map(k => Object.assign({}, k, k.offset != null ? { offset: 1 - k.offset } : {}));
    w.el.animate(f, { duration: LD.ms(460), easing: 'cubic-bezier(.2,.75,.25,1)' });
    focus(id);
  }

  function close(id) {
    const w = get(id);
    if (!w) return;
    wins.splice(wins.indexOf(w), 1);
    const el = w.el;
    el.style.pointerEvents = 'none';
    el.animate([{ opacity: 1, transform: 'none' }, { opacity: 0, transform: 'scale(.95)' }], { duration: LD.ms(180), easing: 'ease-in' })
      .onfinish = () => el.remove();
    if (focusedId === id) { focusedId = null; focusTopmost(); }
    LD.emit('windows');
  }

  function toggleMax(id, force) {
    const w = get(id);
    if (!w) return;
    w.max = force != null ? force : !w.max;
    w.el.classList.add('geo-anim');
    place(w);
    setTimeout(() => w.el.classList.remove('geo-anim'), 450);
  }

  function relayout() { wins.forEach(w => { if (w.max) place(w); }); }

  function byApp(appId) { return wins.filter(w => w.app === appId); }
  function runningApps() {
    const seen = [];
    wins.forEach(w => { if (!seen.includes(w.app)) seen.push(w.app); });
    return seen;
  }
  function mostRecent(appId) {
    return byApp(appId).sort((a, b) => b.lastFocus - a.lastFocus || b.id - a.id)[0];
  }

  function peek(id) {
    wins.forEach(w => w.el.classList.toggle('peek-target', w.id === id));
    desk.classList.add('peeking');
  }
  function unpeek() {
    desk.classList.remove('peeking');
    wins.forEach(w => w.el.classList.remove('peek-target'));
  }

  // Clicking empty desktop clears focus (no app is "active").
  desk.addEventListener('pointerdown', e => {
    if (e.target === desk && focusedId != null) {
      focusedId = null;
      wins.forEach(x => x.el.classList.remove('focused'));
      LD.emit('windows');
    }
  });
  desk.addEventListener('contextmenu', e => {
    if (e.target !== desk) return;
    e.preventDefault();
    LD.menu(e.clientX, e.clientY, [
      { label: 'Open Launchpad', shortcut: 'Ctrl+Space', onClick: () => LD.launchpad.open() },
      { label: 'Laminar Settings…', onClick: () => LD.openSettings() },
      { sep: true },
      { label: 'Wallpaper', submenu: ['dusk', 'dawn', 'graphite'].map(k => ({
        label: k[0].toUpperCase() + k.slice(1), checked: LD.settings.wallpaper === k, onClick: () => LD.set('wallpaper', k),
      })) },
    ]);
  });
  addEventListener('resize', relayout);

  LD.wm = {
    open, focus, minimize, restore, close, toggleMax, relayout, workArea, peek, unpeek,
    get, byApp, runningApps, mostRecent,
    get focusedId() { return focusedId; },
    focusedApp() { const w = get(focusedId); return w ? w.app : null; },
    all: () => wins.slice(),
  };
})(window.LD);
