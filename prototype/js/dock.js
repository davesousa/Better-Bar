/* Laminar Dock prototype — the dock.
   Responsibilities: layout (pinned items, running apps, fixed items), keyed
   rendering with enter/leave/reorder animations, magnification, labels,
   click behaviour, drag & drop (reorder, remove, pin, make stacks),
   context menus and auto-hide. */
(function (LD) {
  'use strict';
  const S = () => LD.settings;
  const dockEl = document.getElementById('dock');
  const wrap = document.getElementById('dock-wrap');
  const labelEl = document.getElementById('dock-label');

  const els = new Map();     // key -> element
  const scale = new Map();   // key -> current magnification scale
  let list = [];             // current layout (array of items with .key)
  let pointerX = null;       // pointer x while hovering the dock
  let anchorX = null;        // last pointer x (keeps the dock steady while it settles)
  let hoverKey = null;
  let raf = 0, lastT = 0;
  let rendered = false;
  let drag = null;

  /* ---------------- Layout ---------------- */
  function keyOf(it) {
    if (it.type === 'app') return 'app:' + it.id;
    if (it.type === 'sep') return 'sep:' + it.id;
    if (it.type === 'stack') return 'stack:' + it.id;
    return it.type;          // launch, trash, clock, ph
  }
  const wf = it => (it.type === 'clock' ? 1.3 : 1);

  function computeLayout() {
    const L = [{ type: 'launch', key: 'launch', fixed: true }];
    const pinnedApps = new Set();
    LD.state.dock.forEach(it => {
      L.push(Object.assign({}, it, { key: keyOf(it), pinned: true, ref: it }));
      if (it.type === 'app') pinnedApps.add(it.id);
    });
    const running = LD.wm.runningApps().filter(a => !pinnedApps.has(a) && a !== 'trash');
    // While a running app is being dragged into the pinned area it's represented by the placeholder.
    if (running.length) {
      L.push({ type: 'sep', key: 'sep:run', auto: true, fixed: true });
      running.forEach(a => L.push({ type: 'app', id: a, key: 'app:' + a, pinned: false }));
    }
    L.push({ type: 'sep', key: 'sep:end', auto: true, fixed: true });
    L.push({ type: 'trash', key: 'trash', fixed: true });
    if (S().showClock) L.push({ type: 'clock', key: 'clock', fixed: true });
    return L;
  }

  /* ---------------- Elements ---------------- */
  function stackTileHTML(st) {
    return `<div class="stack-tile">${st.apps.slice(0, 4).map(a => LD.icon(a)).join('')}</div>`;
  }
  function clockHTML() {
    const d = new Date();
    const t = d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' }).replace(/\s?[AP]M$/i, '');
    const ds = d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' });
    return `<div class="clock-tile"><b>${t}</b><span>${ds}</span></div>`;
  }

  function createEl(it) {
    const el = document.createElement('div');
    el.className = 'dock-item kind-' + it.type;
    el.dataset.key = it.key;
    if (it.type === 'sep') return el;
    if (it.type === 'clock') { el.style.setProperty('--wf', 1.3); el.innerHTML = clockHTML(); return el; }
    if (it.type === 'ph') return el;
    let tile = '';
    if (it.type === 'app') tile = LD.icon(it.id);
    else if (it.type === 'launch') tile = LD.icon('launch');
    else if (it.type === 'trash') tile = LD.icon('trash');
    else if (it.type === 'stack') tile = stackTileHTML(it);
    el.innerHTML = `<div class="tile">${tile}</div><span class="ind"></span><span class="badge"></span>`;
    el._sig = it.type === 'stack' ? it.apps.slice(0, 4).join(',') : '';
    return el;
  }

  function updateEl(el, it) {
    if (it.type === 'stack') {
      const sig = it.apps.slice(0, 4).join(',');
      if (sig !== el._sig) { el.querySelector('.tile').innerHTML = stackTileHTML(it); el._sig = sig; }
    }
    if (it.type === 'app') {
      const wins = LD.wm.byApp(it.id);
      el.classList.toggle('running', wins.length > 0);
      el.classList.toggle('multi', wins.length > 1);
      el.classList.toggle('focused', LD.wm.focusedApp() === it.id);
      const b = el.querySelector('.badge');
      const n = LD.badges[it.id] || '';
      if (b.textContent !== String(n)) b.textContent = n;
    }
    if (it.type === 'launch') el.classList.toggle('focused', LD.launchpad && LD.launchpad.isOpen());
    el.classList.toggle('drag-src', !!(drag && (drag.slotKey === it.key || drag.hideKey === it.key)));
  }

  /* ---------------- Render (keyed, animated) ---------------- */
  function render(opts = {}) {
    const next = computeLayout();
    const nextKeys = new Set(next.map(i => i.key));
    const before = opts.flip ? snapshot() : null;

    // Leaving elements collapse in place, then disappear.
    els.forEach((el, key) => {
      if (!nextKeys.has(key) && !el._leaving) {
        el._leaving = true;
        els.delete(key);
        scale.delete(key);
        if (!rendered || opts.instant) { el.remove(); return; }
        const w = el.offsetWidth;
        el.style.pointerEvents = 'none';
        el.animate([{ width: w + 'px', opacity: 1 }, { width: '0px', opacity: 0, marginLeft: '0px', marginRight: '0px' }],
          { duration: LD.ms(260), easing: 'cubic-bezier(.3,.7,.2,1)', fill: 'forwards' }).onfinish = () => el.remove();
      }
    });

    // Create and order.
    let cursor = dockEl.firstElementChild;
    next.forEach(it => {
      let el = els.get(it.key);
      const isNew = !el;
      if (isNew) { el = createEl(it); els.set(it.key, el); }
      while (cursor && cursor._leaving) cursor = cursor.nextElementSibling;
      if (cursor === el) cursor = cursor.nextElementSibling;
      else dockEl.insertBefore(el, cursor);
      updateEl(el, it);
      if (isNew && rendered && !opts.instant) {
        if (it.type === 'sep') el.animate([{ opacity: 0 }, { opacity: 0.34 }], { duration: LD.ms(240) });
        else {
          const w = S().iconSize * wf(it);
          el.animate([{ width: '0px', opacity: 0, transform: 'scale(.4)' }, { width: w + 'px', opacity: 1, transform: 'none' }],
            { duration: LD.ms(380), easing: LD.springEasing });
        }
      }
    });
    list = next;
    if (before) flip(before);
    rendered = true;
    dockEl.classList.toggle('ind-bar', S().indicatorStyle === 'bar');
    syncRefract();
  }

  function snapshot() {
    const m = new Map();
    els.forEach((el, key) => m.set(key, el.getBoundingClientRect().left));
    return m;
  }
  function flip(before) {
    els.forEach((el, key) => {
      if (!before.has(key) || el._leaving) return;
      const dx = before.get(key) - el.getBoundingClientRect().left;
      if (Math.abs(dx) > 0.5) {
        el.animate([{ transform: `translateX(${dx}px)` }, { transform: 'none' }],
          { duration: LD.ms(380), easing: LD.springEasing, composite: 'add' });
      }
    });
  }

  /* ---------------- Magnification ---------------- */
  function baseLayout() {
    const s = S(), I = s.iconSize, G = s.iconSpacing;
    let x = 0;
    const out = list.map(it => {
      const w = it.type === 'sep' ? 13 + G : I * wf(it) + G;
      const o = { left: x, w };
      x += w;
      return o;
    });
    const total = x + 2 * s.dockPadding;
    const left0 = innerWidth / 2 - total / 2 + s.dockPadding;
    out.forEach(o => { o.left += left0; o.center = o.left + o.w / 2; });
    return out;
  }

  function targets(base) {
    const s = S();
    const out = list.map(it => (it.type === 'sep' ? 1 : 1));
    if (!s.magnify || drag || pointerX == null || s.magnifySize <= s.iconSize) return out;
    const M = s.magnifySize / s.iconSize;
    const range = s.magnifyRange * (s.iconSize + s.iconSpacing);
    list.forEach((it, i) => {
      if (it.type === 'sep') return;
      const d = Math.abs(pointerX - base[i].center);
      const f = d < range ? (Math.cos(Math.PI * d / range) + 1) / 2 : 0;
      out[i] = 1 + (M - 1) * f;
    });
    return out;
  }

  function kick() {
    if (!raf) { lastT = performance.now(); raf = requestAnimationFrame(tick); }
  }

  function tick(t) {
    const dt = Math.min(0.05, Math.max(0.001, (t - lastT) / 1000));
    lastT = t;
    const k = S().reduceMotion ? 1 : 1 - Math.exp(-dt * 22 / S().animSpeed);
    const base = baseLayout();
    const tg = targets(base);
    let moving = false;
    list.forEach((it, i) => {
      if (it.type === 'sep') return;
      const el = els.get(it.key);
      if (!el) return;
      const cur = scale.has(it.key) ? scale.get(it.key) : 1;
      let n = cur + (tg[i] - cur) * k;
      if (Math.abs(tg[i] - n) < 0.002) n = tg[i]; else moving = true;
      if (n !== cur) { scale.set(it.key, n); el.style.setProperty('--s', n.toFixed(4)); }
    });
    applyShift(base);
    positionLabel();
    syncRefract();
    if (moving) raf = requestAnimationFrame(tick);
    else { raf = 0; if (pointerX == null) { anchorX = null; dockEl.style.transform = ''; } }
  }

  /* Keep the point under the cursor fixed while icons grow (the dock expands
     outward from the pointer, like macOS) instead of drifting sideways. */
  function applyShift(base) {
    if (anchorX == null) { dockEl.style.transform = ''; return; }
    const s = S(), I = s.iconSize, G = s.iconSpacing;
    let total = 2 * s.dockPadding;
    const neww = list.map(it => {
      const w = it.type === 'sep' ? 13 + G : I * wf(it) * (scale.get(it.key) || 1) + G;
      total += w;
      return w;
    });
    let x = innerWidth / 2 - total / 2 + s.dockPadding;
    let shift = 0;
    for (let i = 0; i < list.length; i++) {
      const b = base[i];
      if (anchorX < b.left + b.w || i === list.length - 1) {
        const f = LD.clamp((anchorX - b.left) / b.w, 0, 1);
        shift = anchorX - (x + f * neww[i]);
        break;
      }
      x += neww[i];
    }
    // Never push the dock off-screen.
    const half = total / 2;
    shift = LD.clamp(shift, half - innerWidth / 2 + 4, innerWidth / 2 - half - 4);
    dockEl.style.transform = Math.abs(shift) < 0.2 ? '' : `translateX(${shift.toFixed(2)}px)`;
  }

  dockEl.addEventListener('pointermove', e => {
    if (drag) return;
    pointerX = e.clientX;
    anchorX = e.clientX;
    dockEl.classList.toggle('magnifying', S().magnify);
    kick();
  });
  dockEl.addEventListener('pointerleave', () => {
    pointerX = null;
    dockEl.classList.remove('magnifying');
    setHover(null);
    kick();
  });

  /* ---------------- Labels & previews ---------------- */
  function labelFor(key) {
    const it = list.find(i => i.key === key);
    if (!it) return '';
    if (it.type === 'app') return LD.APPS[it.id] ? LD.APPS[it.id].name : it.id;
    if (it.type === 'launch') return 'Launchpad';
    if (it.type === 'trash') return 'Recycle Bin';
    if (it.type === 'stack') return it.name;
    if (it.type === 'clock') return new Date().toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
    return '';
  }
  let labelSuppressed = false;
  function setHover(key) {
    if (key === hoverKey) return;
    hoverKey = key;
    const it = list.find(i => i.key === key);
    if (it && it.type === 'app' && LD.wm.byApp(it.id).length) LD.previews.hoverIcon(it.id, els.get(key));
    else LD.previews.leaveIcon(!!it);
    updateLabel();
  }
  function updateLabel() {
    const txt = hoverKey && !drag && !labelSuppressed && !LD.stacks.isOpen() && !LD.menu.isOpen() ? labelFor(hoverKey) : '';
    if (txt) { labelEl.textContent = txt; labelEl.classList.add('show'); positionLabel(); }
    else labelEl.classList.remove('show');
  }
  function positionLabel() {
    if (!hoverKey) return;
    const el = els.get(hoverKey);
    if (!el) return;
    const r = el.getBoundingClientRect();
    labelEl.style.left = (r.left + r.width / 2) + 'px';
    labelEl.style.top = 'auto';
    labelEl.style.bottom = (innerHeight - r.top + 10) + 'px';
  }
  dockEl.addEventListener('pointerover', e => {
    if (drag) return;
    const el = e.target.closest('.dock-item');
    setHover(el && !el.classList.contains('kind-sep') ? el.dataset.key : null);
  });

  /* ---------------- Clicks ---------------- */
  function itemByKey(key) { return list.find(i => i.key === key); }

  function bounce(el) {
    if (!S().bounceOnLaunch || S().reduceMotion) return 0;
    el.classList.remove('bouncing');
    void el.offsetWidth;
    el.classList.add('bouncing');
    setTimeout(() => el.classList.remove('bouncing'), 1000);
    return 520;
  }

  function launch(appId, o = {}) {
    const el = els.get('app:' + appId);
    const delay = el ? bounce(el) : 0;
    if (!el) render();               // app shows up as a running item
    setTimeout(() => {
      LD.wm.open(appId, o);
      const e2 = els.get('app:' + appId);
      if (!el && e2) bounce(e2);
    }, delay);
  }
  LD.launch = launch;
  /** Open an app from Launchpad or a stack: focus it if running, otherwise launch it. */
  LD.openApp = function (appId) {
    const w = LD.wm.mostRecent(appId);
    if (!w) { launch(appId); return; }
    if (w.min) LD.wm.restore(w.id); else LD.wm.focus(w.id);
  };

  function activateApp(appId, e) {
    const wins = LD.wm.byApp(appId);
    if (e && (e.shiftKey || e.button === 1)) { launch(appId); return; }       // new window
    if (!wins.length) { launch(appId); return; }
    const focusedApp = LD.wm.focusedApp();
    if (focusedApp === appId && S().clickToMinimize) {
      if (wins.length === 1) LD.wm.minimize(wins[0].id);
      else LD.previews.showPinned(appId, els.get('app:' + appId));      // pick a window
      return;
    }
    const w = LD.wm.mostRecent(appId);
    if (w.min) LD.wm.restore(w.id); else LD.wm.focus(w.id);
  }

  function clickItem(it, e) {
    LD.previews.hide();
    if (it.type === 'launch') { LD.launchpad.toggle(); return; }
    if (it.type === 'app') { if (LD.launchpad.isOpen()) LD.launchpad.close(); activateApp(it.id, e); return; }
    if (it.type === 'stack') { LD.stacks.toggle(it.ref, els.get(it.key)); return; }
    if (it.type === 'trash') { activateApp('trash', e); return; }
    if (it.type === 'clock') { LD.launchpad.toggle(); return; }
  }

  dockEl.addEventListener('pointerdown', e => {
    const el = e.target.closest('.dock-item');
    if (!el || e.button === 2) return;
    const it = itemByKey(el.dataset.key);
    if (!it || it.type === 'ph') return;
    if (e.button === 1) { e.preventDefault(); if (it.type === 'app') launch(it.id); return; }
    const sx = e.clientX, sy = e.clientY;
    el.classList.add('pressed');
    let started = false;
    const move = ev => {
      if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) > 6) {
        started = true;
        el.classList.remove('pressed');
        beginDockDrag(it, el, ev);
      }
      if (started) dragMove(ev);
    };
    const up = ev => {
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', up);
      el.classList.remove('pressed');
      if (started) dragEnd(ev);
      else clickItem(it, ev);
    };
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
  });
  dockEl.addEventListener('auxclick', e => e.preventDefault());

  /* ---------------- Drag & drop ---------------- */
  function makeGhost(iconHTML, x, y) {
    const g = document.createElement('div');
    g.className = 'drag-ghost';
    const sz = S().iconSize;
    g.style.width = sz + 'px'; g.style.height = sz + 'px';
    g.style.left = x + 'px'; g.style.top = y + 'px';
    g.innerHTML = iconHTML + '<span class="rm glass strong">Remove</span>';
    document.body.appendChild(g);
    return g;
  }

  function beginDockDrag(it, el, e) {
    if (it.fixed) return;                     // launch, trash, clock, auto separators stay put
    const html = it.type === 'sep'
      ? `<div style="width:100%;height:100%;display:grid;place-items:center"><div style="width:3px;height:70%;border-radius:2px;background:var(--ind);opacity:.6"></div></div>`
      : el.querySelector('.tile').innerHTML;
    if (it.pinned) {
      startDrag({ source: 'dock', entry: it.ref, html, from: el.getBoundingClientRect() }, e);
    } else {
      startDrag({ source: 'running', entry: { type: 'app', id: it.id }, html, from: el.getBoundingClientRect(), hideKey: it.key }, e);
    }
  }

  /** Public: start dragging an app in from somewhere else (Launchpad, a stack). */
  LD.dockDragIn = function (appId, e, fromRect, source = 'launchpad', extra = {}) {
    startDrag(Object.assign({ source, entry: { type: 'app', id: appId }, html: LD.icon(appId), from: fromRect }, extra), e);
    dragMove(e);
    const move = ev => dragMove(ev);
    const up = ev => { removeEventListener('pointermove', move); removeEventListener('pointerup', up); dragEnd(ev); };
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
  };

  function startDrag(d, e) {
    LD.previews.hide();
    LD.stacks.close();
    LD.menu.close();
    pointerX = null;
    dockEl.classList.remove('magnifying');
    drag = Object.assign(d, {
      snapshot: JSON.stringify(LD.state.dock),
      ghost: makeGhost(d.html, e.clientX, e.clientY),
      removing: false, merge: null, mergeT: 0,
      slot: d.source === 'dock' ? d.entry : { type: 'ph' },
    });
    drag.slotKey = keyOf(drag.slot);
    if (d.hideKey && els.get(d.hideKey)) els.get(d.hideKey).classList.add('drag-src');
    setHover(null);
    reveal(true);
    render();
    kick();
  }

  function pinnedIndexAt(x) {
    // Count pinned (non-slot) entries whose centre is left of x.
    const dr = dockEl.getBoundingClientRect();
    let count = 0, hit = null;
    LD.state.dock.forEach(entry => {
      if (entry === drag.slot) return;
      const el = els.get(keyOf(entry));
      if (!el) return;
      const c = dr.left + el.offsetLeft + el.offsetWidth / 2;
      const w = el.offsetWidth;
      if (Math.abs(x - c) < w * 0.3 && (entry.type === 'app' || entry.type === 'stack')) hit = entry;
      if (c < x) count++;
    });
    return { count, hit };
  }

  function placeSlot(count) {
    const arr = LD.state.dock.filter(e => e !== drag.slot);
    const cur = LD.state.dock.indexOf(drag.slot);
    const curCount = cur < 0 ? -1 : LD.state.dock.slice(0, cur).filter(e => e !== drag.slot).length;
    if (cur >= 0 && curCount === count) return;
    arr.splice(count, 0, drag.slot);
    LD.state.dock = arr;
    render({ flip: true });
  }
  function removeSlot() {
    if (!LD.state.dock.includes(drag.slot)) return;
    LD.state.dock = LD.state.dock.filter(e => e !== drag.slot);
    render({ flip: true });
  }

  function setMerge(entry) {
    if (drag.merge === entry) return;
    clearTimeout(drag.mergeT);
    if (drag.merge) { const o = els.get(keyOf(drag.merge)); o && o.classList.remove('merge-target'); }
    drag.merge = entry; drag.mergeReady = false;
    if (!entry) return;
    drag.mergeT = setTimeout(() => {
      drag.mergeReady = true;
      const el = els.get(keyOf(entry)); el && el.classList.add('merge-target');
    }, entry.type === 'stack' ? 160 : 520);
  }

  function dragMove(e) {
    if (!drag) return;
    drag.ghost.style.left = e.clientX + 'px';
    drag.ghost.style.top = e.clientY + 'px';
    const dr = dockEl.getBoundingClientRect();
    const inBand = e.clientY >= dr.top - 56 && e.clientX > dr.left - 90 && e.clientX < dr.right + 90;
    const canMerge = drag.entry.type === 'app';
    if (inBand) {
      drag.removing = false;
      const { count, hit } = pinnedIndexAt(e.clientX);
      if (hit && canMerge && hit !== drag.slot && !(hit.type === 'app' && hit.id === drag.entry.id)) {
        setMerge(hit);                          // hold here to stack
        if (!LD.state.dock.includes(drag.slot)) placeSlot(count);
      } else {
        setMerge(null);
        placeSlot(count);
      }
    } else {
      setMerge(null);
      const removable = drag.source === 'dock' || drag.source === 'stack';
      drag.removing = removable && e.clientY < dr.top - 80;
      if (drag.source !== 'dock' || drag.removing) removeSlot();
    }
    drag.ghost.classList.toggle('removing', drag.removing);
  }

  function flyGhost(toRect, done) {
    const g = drag.ghost;
    if (!toRect) { g.remove(); done && done(); return; }
    g.style.transition = `left ${LD.ms(300)}ms ${LD.springEasing}, top ${LD.ms(300)}ms ${LD.springEasing}, transform ${LD.ms(300)}ms ease`;
    g.style.left = (toRect.left + toRect.width / 2) + 'px';
    g.style.top = (toRect.top + toRect.height / 2) + 'px';
    g.style.transform = 'translate(-50%,-50%) scale(1)';
    setTimeout(() => { g.remove(); done && done(); }, LD.ms(300));
  }

  function poof(x, y) {
    for (let i = 0; i < 7; i++) {
      const p = document.createElement('div');
      p.className = 'poof';
      p.style.left = x + 'px'; p.style.top = y + 'px';
      document.body.appendChild(p);
      const a = (i / 7) * Math.PI * 2, r = 26 + Math.random() * 14;
      p.animate([{ transform: 'translate(-50%,-50%) scale(1.6)', opacity: .9 },
        { transform: `translate(calc(-50% + ${Math.cos(a) * r}px), calc(-50% + ${Math.sin(a) * r}px)) scale(.2)`, opacity: 0 }],
        { duration: 420, easing: 'cubic-bezier(.2,.8,.3,1)' }).onfinish = () => p.remove();
    }
  }

  function dragEnd(e) {
    if (!drag) return;
    const d = drag;
    clearTimeout(d.mergeT);
    const finish = () => {
      if (d.hideKey && els.get(d.hideKey)) els.get(d.hideKey).classList.remove('drag-src');
      drag = null;
      render();
      LD.save();
      maybeHide();
    };

    // 1) Drop onto an app/stack held long enough → make or extend a stack.
    if (d.merge && d.mergeReady) {
      const target = d.merge;
      const mEl = els.get(keyOf(target));
      mEl && mEl.classList.remove('merge-target');
      LD.state.dock = LD.state.dock.filter(x => x !== d.slot && !(d.source !== 'dock' && x.type === 'app' && x.id === d.entry.id && x !== target));
      let stackEntry;
      if (target.type === 'stack') {
        if (!target.apps.includes(d.entry.id)) target.apps.push(d.entry.id);
        stackEntry = target;
      } else {
        stackEntry = { type: 'stack', id: LD.uid('k'), name: 'New Stack', apps: [target.id, d.entry.id], view: 'grid' };
        LD.state.dock[LD.state.dock.indexOf(target)] = stackEntry;
      }
      if (d.source === 'stack') LD.stacks.removeApp(d.stackId, d.entry.id, true);
      drag = null;
      render();
      const sEl = els.get(keyOf(stackEntry));
      drag = d;
      flyGhost(sEl && sEl.getBoundingClientRect(), () => {
        drag = null; finish();
        if (target.type !== 'stack') setTimeout(() => LD.stacks.open(stackEntry, els.get(keyOf(stackEntry)), true), 120);
      });
      return;
    }

    // 2) Dropped into the dock.
    if (LD.state.dock.includes(d.slot)) {
      if (d.slot.type === 'ph') {
        const real = d.entry;
        LD.state.dock = LD.state.dock.filter(x => !(x.type === 'app' && x.id === real.id));
        LD.state.dock = LD.state.dock.map(x => (x === d.slot ? real : x));
        if (d.source === 'stack') LD.stacks.removeApp(d.stackId, real.id, true);
      }
      drag = null;
      render();
      drag = d;
      const el = els.get(keyOf(d.entry));
      if (el) { el.classList.add('drag-src'); }
      flyGhost(el && el.getBoundingClientRect(), () => { el && el.classList.remove('drag-src'); finish(); });
      return;
    }

    // 3) Pulled up and off the dock → remove.
    if (d.removing) {
      if (d.source === 'stack') LD.stacks.removeApp(d.stackId, d.entry.id, true);
      poof(e.clientX, e.clientY);
      d.ghost.animate([{ opacity: 1, transform: 'translate(-50%,-50%) scale(.92)' }, { opacity: 0, transform: 'translate(-50%,-50%) scale(.2)' }],
        { duration: 220, easing: 'ease-in' }).onfinish = () => d.ghost.remove();
      if (d.source === 'dock' && d.entry.type === 'app' && LD.wm.byApp(d.entry.id).length) {
        LD.toast(d.entry.id, 'Removed from Dock', 'It stays in the dock while it is running.');
      }
      finish();
      return;
    }

    // 4) Anything else → cancel and fly back.
    LD.state.dock = JSON.parse(d.snapshot);
    // Re-link: object identity changed, so rebuild keyed render from scratch for pinned items.
    flyGhost(d.from, finish);
  }

  /* ---------------- Context menus ---------------- */
  function itemMenu(it, e) {
    const x = e.clientX, y = (els.get(it.key) || dockEl).getBoundingClientRect().top - 8;
    const dockActions = [
      { label: S().magnify ? 'Turn Magnification Off' : 'Turn Magnification On', onClick: () => LD.set('magnify', !S().magnify) },
      { label: S().autoHide ? 'Turn Hiding Off' : 'Turn Hiding On', onClick: () => LD.set('autoHide', !S().autoHide) },
      { label: 'Dock Settings…', onClick: () => LD.openSettings('dock') },
    ];
    let items;
    if (it.type === 'app') {
      const wins = LD.wm.byApp(it.id);
      const app = LD.APPS[it.id];
      const idx = LD.state.dock.indexOf(it.ref);
      const stacks = LD.state.dock.filter(s => s.type === 'stack');
      items = [];
      if (wins.length) {
        items.push({ header: 'Windows' });
        wins.forEach(w => items.push({ label: w.title, checked: LD.wm.focusedId === w.id, onClick: () => (w.min ? LD.wm.restore(w.id) : LD.wm.focus(w.id)) }));
        items.push({ sep: true });
      }
      items.push({ label: 'New Window', shortcut: 'Shift+Click', onClick: () => launch(it.id) });
      if (wins.length > 1) items.push({ label: 'Show All Windows', onClick: () => LD.previews.showPinned(it.id, els.get(it.key)) });
      items.push({ sep: true });
      items.push({ label: 'Keep in Dock', checked: it.pinned, onClick: () => togglePin(it) });
      if (it.pinned) {
        items.push({ label: 'Add to Stack', submenu: [
          ...stacks.map(s => ({ label: s.name, onClick: () => { LD.state.dock = LD.state.dock.filter(x => x !== it.ref); if (!s.apps.includes(it.id)) s.apps.push(it.id); render(); LD.save(); } })),
          stacks.length ? { sep: true } : null,
          { label: 'New Stack', onClick: () => { const st = { type: 'stack', id: LD.uid('k'), name: 'New Stack', apps: [it.id], view: 'grid' }; LD.state.dock[idx] = st; render(); LD.save(); setTimeout(() => LD.stacks.open(st, els.get(keyOf(st)), true), 300); } },
        ] });
        items.push({ label: 'Add Separator After', onClick: () => { LD.state.dock.splice(idx + 1, 0, { type: 'sep', id: LD.uid('s') }); render(); LD.save(); } });
      }
      const lp = LD.state.lpPinned.includes(it.id);
      items.push({ label: lp ? 'Unpin from Launchpad' : 'Pin to Launchpad', onClick: () => LD.launchpad.togglePin(it.id) });
      items.push({ label: 'Run as Administrator', onClick: () => { LD.toast(it.id, app.name, 'Would start elevated (UAC prompt in the native app).'); launch(it.id); } });
      items.push({ label: 'Open File Location', onClick: () => LD.toast('files', 'Files', `Would open the folder containing ${app.name}.`) });
      if (wins.length) {
        items.push({ sep: true });
        items.push({ label: wins.length > 1 ? `Close All ${wins.length} Windows` : 'Close Window', danger: true, onClick: () => wins.forEach(w => LD.wm.close(w.id)) });
      }
    } else if (it.type === 'sep') {
      items = [
        { label: 'Remove Separator', onClick: () => { LD.state.dock = LD.state.dock.filter(x => x !== it.ref); render(); LD.save(); } },
        { sep: true }, ...dockActions,
      ];
      if (it.auto) items = dockActions;
    } else if (it.type === 'stack') {
      const st = it.ref;
      items = [
        { label: 'Open', onClick: () => LD.stacks.open(st, els.get(it.key)) },
        { sep: true },
        { label: 'View as Grid', checked: st.view !== 'fan', onClick: () => { st.view = 'grid'; LD.save(); } },
        { label: 'View as Fan', checked: st.view === 'fan', onClick: () => { st.view = 'fan'; LD.save(); } },
        { label: 'Sort by Name', onClick: () => { st.apps.sort((a, b) => LD.APPS[a].name.localeCompare(LD.APPS[b].name)); render(); LD.save(); } },
        { label: 'Rename…', onClick: () => LD.stacks.open(st, els.get(it.key), true) },
        { sep: true },
        { label: 'Ungroup Stack', onClick: () => LD.stacks.ungroup(st) },
        { label: 'Remove from Dock', danger: true, onClick: () => { LD.state.dock = LD.state.dock.filter(x => x !== st); render(); LD.save(); } },
      ];
    } else if (it.type === 'launch') {
      // Windows' power-user (Win+X) menu, Laminar style.
      items = [
        { label: 'Open Launchpad', shortcut: 'Ctrl+Space', onClick: () => LD.launchpad.open() },
        { label: 'Laminar Settings…', onClick: () => LD.openSettings() },
        { sep: true },
        { label: 'Task Manager', shortcut: 'Ctrl+Shift+Esc', onClick: () => launch('taskmgr') },
        { label: 'System Settings', onClick: () => launch('system') },
        { label: 'Files', onClick: () => launch('files') },
        { label: 'Terminal', onClick: () => launch('terminal') },
        { label: 'Terminal (Admin)', onClick: () => launch('terminal', { title: 'Administrator: Terminal' }) },
        { sep: true },
        { label: 'Shut down or sign out', submenu: [
          { label: 'Lock', onClick: () => LD.toast('laminar', 'Lock', 'Would lock the PC.') },
          { label: 'Sign out', onClick: () => LD.toast('laminar', 'Sign out', 'Would sign out.') },
          { label: 'Sleep', onClick: () => LD.toast('laminar', 'Sleep', 'Would put the PC to sleep.') },
          { label: 'Restart', onClick: () => LD.toast('laminar', 'Restart', 'Would restart.') },
          { label: 'Shut down', onClick: () => LD.toast('laminar', 'Shut down', 'Would shut down.') },
        ] },
        { label: 'Show Desktop', onClick: () => LD.wm.all().forEach(w => LD.wm.minimize(w.id)) },
      ];
    } else if (it.type === 'trash') {
      items = [
        { label: 'Open', onClick: () => activateApp('trash') },
        { label: 'Empty Recycle Bin', onClick: () => LD.toast('trash', 'Recycle Bin', 'Would permanently delete 12 items (after a confirmation).') },
        { sep: true }, ...dockActions,
      ];
    } else if (it.type === 'clock') {
      items = [
        { label: 'Adjust Date & Time…', onClick: () => launch('system', { title: 'Date & time — System Settings' }) },
        { label: 'Hide Clock from Dock', onClick: () => LD.set('showClock', false) },
        { sep: true }, ...dockActions,
      ];
    }
    LD.menu(x, y, items, { above: true, onClose: () => updateLabel() });
    updateLabel();
  }

  function togglePin(it) {
    if (it.pinned) {
      LD.state.dock = LD.state.dock.filter(x => x !== it.ref);
    } else {
      LD.state.dock.push({ type: 'app', id: it.id });
    }
    render({ flip: true });
    LD.save();
  }
  LD.pinToDock = function (appId, on) {
    const has = LD.state.dock.some(x => x.type === 'app' && x.id === appId);
    if (on && !has) LD.state.dock.push({ type: 'app', id: appId });
    if (!on && has) LD.state.dock = LD.state.dock.filter(x => !(x.type === 'app' && x.id === appId));
    render({ flip: true });
    LD.save();
  };
  LD.isPinned = appId => LD.state.dock.some(x => x.type === 'app' && x.id === appId);

  dockEl.addEventListener('contextmenu', e => {
    e.preventDefault();
    const el = e.target.closest('.dock-item');
    const it = el && itemByKey(el.dataset.key);
    if (it && it.type !== 'ph') { itemMenu(it, e); return; }
    LD.menu(e.clientX, dockEl.getBoundingClientRect().top - 8, [
      { label: S().magnify ? 'Turn Magnification Off' : 'Turn Magnification On', onClick: () => LD.set('magnify', !S().magnify) },
      { label: S().autoHide ? 'Turn Hiding Off' : 'Turn Hiding On', onClick: () => LD.set('autoHide', !S().autoHide) },
      { sep: true },
      { label: 'Add Separator', onClick: () => { LD.state.dock.push({ type: 'sep', id: LD.uid('s') }); render(); LD.save(); } },
      { label: 'New Stack', onClick: () => { const st = { type: 'stack', id: LD.uid('k'), name: 'New Stack', apps: [], view: 'grid' }; LD.state.dock.push(st); render(); LD.save(); } },
      { sep: true },
      { label: 'Dock Settings…', onClick: () => LD.openSettings('dock') },
    ], { above: true });
  });

  /* ---------------- Auto-hide ---------------- */
  const glow = document.getElementById('reveal-glow');
  let shown = true, revealT = 0, hideT = 0;
  function isBusy() {
    return !!drag || LD.menu.isOpen() || LD.previews.isOpen() || LD.stacks.isOpen() || (LD.launchpad && LD.launchpad.isOpen());
  }
  function reveal(now) {
    clearTimeout(hideT); hideT = 0;
    if (shown) return;
    clearTimeout(revealT);
    const go = () => { shown = true; wrap.classList.remove('tucked'); glow.classList.remove('on'); };
    if (now) go(); else revealT = setTimeout(go, S().revealDelay);
  }
  function hide() {
    if (!S().autoHide || isBusy()) return;
    shown = false;
    wrap.classList.add('tucked');
    pointerX = null;
    setHover(null);
    kick();
  }
  function maybeHide() {
    if (!S().autoHide) return;
    clearTimeout(hideT);
    hideT = setTimeout(() => {
      hideT = 0;
      if (isBusy()) { maybeHide(); return; }
      if (!dockEl.matches(':hover') && !document.getElementById('preview-pop').matches(':hover')) hide();
    }, S().hideDelay);
  }
  addEventListener('pointermove', e => {
    if (!S().autoHide) return;
    const nearEdge = e.clientY >= innerHeight - S().revealZone;
    if (!shown) {
      if (nearEdge) {
        glow.classList.add('on');
        if (!revealT) revealT = setTimeout(() => { revealT = 0; shown = true; wrap.classList.remove('tucked'); glow.classList.remove('on'); }, S().revealDelay);
      } else { glow.classList.remove('on'); clearTimeout(revealT); revealT = 0; }
      return;
    }
    const top = dockEl.getBoundingClientRect().top - (S().magnifySize - S().iconSize) - 30;
    if (e.clientY < top) { if (!hideT) maybeHide(); }
    else if (hideT) { clearTimeout(hideT); hideT = 0; }
  });
  document.addEventListener('pointerleave', () => { if (S().autoHide) maybeHide(); });
  function applyAutoHide() {
    if (S().autoHide) { hide(); if (isBusy()) reveal(true); }
    else { shown = true; wrap.classList.remove('tucked'); }
    LD.wm.relayout();
  }

  /* ---------------- Refraction map ---------------- */
  // A displacement map: pixels near the rounded edge are pushed along the edge
  // normal, so the wallpaper appears to bend through the glass rim.
  let mapKey = '';
  const feImg = document.getElementById('ld-refract-map');
  const feDisp = document.getElementById('ld-refract-disp');
  function buildMap(w, h, r, bezel) {
    const c = document.createElement('canvas');
    c.width = w; c.height = h;
    const ctx = c.getContext('2d');
    const img = ctx.createImageData(w, h);
    const d = img.data, hw = w / 2, hh = h / 2;
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const px = x + 0.5 - hw, py = y + 0.5 - hh;
        const qx = Math.abs(px) - (hw - r), qy = Math.abs(py) - (hh - r);
        let dist, nx, ny;
        if (qx > 0 && qy > 0) { const L = Math.hypot(qx, qy) || 1; dist = r - L; nx = (qx / L) * Math.sign(px); ny = (qy / L) * Math.sign(py); }
        else if (qx > qy) { dist = r - qx; nx = Math.sign(px); ny = 0; }
        else { dist = r - qy; nx = 0; ny = Math.sign(py); }
        const t = LD.clamp(1 - dist / bezel, 0, 1);
        const m = t * t * (3 - 2 * t);
        const i = (y * w + x) * 4;
        d[i] = 128 - nx * m * 127;
        d[i + 1] = 128 - ny * m * 127;
        d[i + 2] = 128;
        d[i + 3] = 255;
      }
    }
    ctx.putImageData(img, 0, 0);
    return c.toDataURL();
  }
  function syncRefract() {
    const s = S();
    dockEl.classList.toggle('refract', !!s.refraction);
    if (!s.refraction) return;
    const w = Math.round(dockEl.offsetWidth), h = Math.round(dockEl.offsetHeight);
    if (!w || !h) return;
    // Build the map once per rest size; stretch it while the dock magnifies.
    const key = [s.iconSize, s.dockPadding, s.dockRadius].join('|');
    if (key !== mapKey) {
      mapKey = key;
      const restW = Math.min(1400, Math.max(200, w));
      feImg.setAttribute('href', buildMap(restW, h, Math.min(s.dockRadius, h / 2), Math.min(h / 2, 20)));
    }
    feImg.setAttribute('width', w);
    feImg.setAttribute('height', h);
    feDisp.setAttribute('scale', (s.refractionStrength / 100 * 60).toFixed(1));
  }

  /* ---------------- Public API ---------------- */
  LD.dock = {
    render, kick, reveal, maybeHide, applyAutoHide,
    resetMap() { mapKey = ''; syncRefract(); },
    iconRect(appId) {
      const el = els.get('app:' + appId) || (appId === 'trash' ? els.get('trash') : null);
      if (el) return el.getBoundingClientRect();
      const r = dockEl.getBoundingClientRect();
      return { left: r.left + r.width / 2 - 20, top: r.top, width: 40, height: 40 };
    },
    elFor: key => els.get(key),
    keyOf,
    suppressLabel(on) { labelSuppressed = on; updateLabel(); },
    refreshLabel: updateLabel,
    isShown: () => shown,
    setStackEls() { render(); },
  };

  LD.on('windows', () => render());
  setInterval(() => { const c = els.get('clock'); if (c) c.innerHTML = clockHTML(); }, 15000);
})(window.LD);
