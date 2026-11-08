/* Laminar Dock prototype — Launchpad.
   Left: widgets. Centre: search, pinned apps, collapsible All Apps.
   Right: control center (connectivity, media, focus, sound, background apps, updates). */
(function (LD) {
  'use strict';
  const root = document.getElementById('launchpad');
  const ui = LD.ui;
  let isOpen = false;
  let liveT = 0;

  /* ---------- Fake data for search ---------- */
  const FILES = [
    ['Roadmap.docx', 'Documents', 'file'], ['Budget 2026.xlsx', 'Documents\\Finance', 'file'], ['Laminar logo.png', 'Pictures\\Design', 'image'],
    ['Dock mockup.png', 'Pictures\\Design', 'image'], ['Meeting notes.txt', 'Documents', 'file'], ['WinDock', 'Documents\\Apps', 'folder'],
    ['Resume.pdf', 'Documents', 'file'], ['Vacation 2025', 'Pictures', 'folder'], ['Taxes', 'Documents\\Finance', 'folder'],
    ['Screenshot 2026-11-06.png', 'Pictures\\Screenshots', 'image'], ['Downloads', 'Home', 'folder'], ['Project brief.docx', 'Documents\\Work', 'file'],
  ].map(([name, path, kind]) => ({ name, path: 'C:\\Users\\Dave\\' + path, kind }));

  const SETTINGS = [
    ['Bluetooth & devices', 'System Settings', 'bluetooth'], ['Display', 'System Settings', 'monitor'], ['Sound', 'System Settings', 'speaker'],
    ['Wi-Fi & internet', 'System Settings', 'wifi'], ['Windows Update', 'System Settings', 'refresh'], ['Default apps', 'System Settings', 'grid'],
    ['Power & sleep', 'System Settings', 'power'], ['Personalization', 'System Settings', 'sparkle'], ['Accounts', 'System Settings', 'user'],
    ['Glass & blur', 'Laminar Settings', 'sliders', 'glass'], ['Dock size & magnification', 'Laminar Settings', 'sliders', 'dock'],
    ['Auto-hide the dock', 'Laminar Settings', 'sliders', 'behavior'], ['Animation speed', 'Laminar Settings', 'sliders', 'motion'],
    ['Theme & accent color', 'Laminar Settings', 'sliders', 'appearance'], ['Launchpad panels', 'Laminar Settings', 'sliders', 'launchpad'],
  ].map(([name, where, icon, section]) => ({ name, where, icon, section }));

  /* ---------- Control center state ---------- */
  const cc = {
    wifi: true, bt: true, share: false, focus: false, night: false, saver: false, mic: false, cast: false,
    volume: 64, output: 0, view: null,
    nets: [
      { name: 'Home Network', lock: true, s: 4, on: true }, { name: 'Laminar Studio', lock: true, s: 3 },
      { name: 'Coffee Bar Guest', lock: false, s: 2 }, { name: 'Upstairs-5G', lock: true, s: 2 }, { name: 'Printer-Direct', lock: true, s: 1 },
    ],
    devs: [
      { name: 'Headphones', icon: 'headphones', on: true, batt: 82 }, { name: 'Keyboard', icon: 'keyboard', on: true, batt: 64 },
      { name: 'Mouse', icon: 'mouse', on: true, batt: 41 }, { name: 'Phone', icon: 'phone', on: false }, { name: 'Living Room TV', icon: 'tv', on: false },
    ],
    outputs: ['Speakers (Realtek Audio)', 'Headphones (Bluetooth)', 'Monitor (HDMI)'],
    mix: { browser: 80, music: 64, chat: 45 },
  };
  const TRAY = [
    { id: 'cloud', icon: 'cloud', name: 'Cloud Drive', status: 'Syncing 3 files…', dot: '#0A84FF', spin: true },
    { id: 'vpn', icon: 'shield', name: 'Secure VPN', status: 'Connected · Toronto', dot: '#30D158' },
    { id: 'chat', icon: 'chat', name: 'Chat', status: '3 unread messages', dot: '#FF453A' },
    { id: 'games', icon: 'gamepad', name: 'Game Launcher', status: 'Up to date' },
    { id: 'gpu', icon: 'gpu', name: 'GPU Control Panel', status: 'Driver 566.14' },
    { id: 'sec', icon: 'shield', name: 'Security', status: 'No threats found', dot: '#30D158' },
    { id: 'clip', icon: 'clipboard', name: 'Clipboard History', status: '12 items' },
    { id: 'mon', icon: 'activity', name: 'Task Manager', status: 'CPU 14%' },
  ];

  /* ---------- Build ---------- */
  function mount() {
    root.innerHTML = `
      <div class="lp-backdrop"></div>
      <div class="lp-grid">
        <section class="lp-panel lp-left glass"><div class="lp-scroll" data-widgets></div></section>
        <section class="lp-panel lp-center glass">
          <label class="lp-search">${ui('search', 18)}<input type="text" placeholder="Search apps, files and settings" spellcheck="false" autocomplete="off" aria-label="Search"><kbd>Esc</kbd></label>
          <div class="lp-scroll">
            <div class="lp-body" data-home></div>
            <div class="lp-results" data-results hidden></div>
          </div>
          <footer class="lp-foot">
            <div class="lp-avatar">D</div>
            <div class="lp-user">Dave<span>Administrator</span></div>
            <button class="round-btn" data-act="settings" title="Laminar Settings">${ui('gear', 18)}</button>
            <button class="round-btn" data-act="power" title="Power">${ui('power', 18)}</button>
          </footer>
        </section>
        <section class="lp-panel lp-right glass"><div class="cc-views"><div class="cc-main"><div class="lp-scroll" data-cc></div></div><div class="cc-detail"><div class="lp-scroll" data-ccd></div></div></div></section>
      </div>`;

    root.addEventListener('pointerdown', e => {
      if (e.target === root || e.target.classList.contains('lp-backdrop') || e.target.classList.contains('lp-grid')) close();
    });
    root.querySelector('[data-act="settings"]').addEventListener('click', () => LD.openSettings());
    root.querySelector('[data-act="power"]').addEventListener('click', e => {
      const r = e.currentTarget.getBoundingClientRect();
      LD.menu(r.left, r.top - 6, [
        { label: 'Lock', onClick: () => LD.toast('laminar', 'Lock', 'Would lock the PC.') },
        { label: 'Sign out', onClick: () => LD.toast('laminar', 'Sign out', 'Would sign out.') },
        { sep: true },
        { label: 'Sleep', onClick: () => LD.toast('laminar', 'Sleep', 'Would put the PC to sleep.') },
        { label: 'Restart', onClick: () => LD.toast('laminar', 'Restart', 'Would restart.') },
        { label: 'Shut down', onClick: () => LD.toast('laminar', 'Shut down', 'Would shut down.') },
      ], { above: true });
    });

    const input = root.querySelector('.lp-search input');
    input.addEventListener('input', () => search(input.value));
    input.addEventListener('keydown', onSearchKey);

    renderHome();
    renderWidgets();
    renderCC();
    applyLayout();
  }

  function applyLayout() {
    const g = root.querySelector('.lp-grid');
    if (!g) return;
    g.classList.toggle('no-left', !LD.settings.showWidgets);
    g.classList.toggle('no-right', !LD.settings.showControl);
  }

  /* ---------- Centre: home ---------- */
  function appTile(id) {
    return `<div class="lp-app" data-app="${id}"><div class="ai">${LD.icon(id)}</div><div class="an">${LD.esc(LD.APPS[id].name)}</div></div>`;
  }
  let allOpen = false;
  function renderHome() {
    const home = root.querySelector('[data-home]');
    const all = Object.keys(LD.APPS).filter(a => !LD.APPS[a].hidden).sort((a, b) => LD.APPS[a].name.localeCompare(LD.APPS[b].name));
    const groups = {};
    all.forEach(a => { const L = LD.APPS[a].name[0].toUpperCase(); (groups[L] = groups[L] || []).push(a); });
    home.innerHTML = `
      <div class="lp-h">Pinned <button data-act="all">${allOpen ? 'Hide all apps' : 'All apps'}</button></div>
      <div class="lp-apps" data-pinned>${LD.state.lpPinned.filter(a => LD.APPS[a]).map(appTile).join('')}</div>
      <button class="lp-disclose" aria-expanded="${allOpen}">All Apps <span class="cnt">${all.length}</span><span class="chev">${ui('chevron', 14, 2.2)}</span></button>
      <div class="lp-all ${allOpen ? 'open' : ''}"><div>
        ${Object.keys(groups).sort().map(L => `<div class="lp-letter">${L}</div><div class="lp-list">${groups[L].map(a =>
          `<div class="lp-row" data-app="${a}"><div class="ri">${LD.icon(a)}</div>${LD.esc(LD.APPS[a].name)}</div>`).join('')}</div>`).join('')}
      </div></div>`;
    const toggleAll = () => {
      allOpen = !allOpen;
      home.querySelector('.lp-all').classList.toggle('open', allOpen);
      home.querySelector('.lp-disclose').setAttribute('aria-expanded', allOpen);
      home.querySelector('[data-act="all"]').textContent = allOpen ? 'Hide all apps' : 'All apps';
      if (allOpen) setTimeout(() => home.querySelector('.lp-disclose').scrollIntoView({ behavior: 'smooth', block: 'start' }), 80);
    };
    home.querySelector('.lp-disclose').addEventListener('click', toggleAll);
    home.querySelector('[data-act="all"]').addEventListener('click', toggleAll);
    home.querySelectorAll('[data-app]').forEach(wireApp);
  }

  function wireApp(el) {
    const appId = el.dataset.app;
    el.addEventListener('pointerdown', e => {
      if (e.button !== 0) return;
      const sx = e.clientX, sy = e.clientY;
      let started = false;
      const move = ev => {
        if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) > 6) {
          started = true;
          cleanup();
          const ic = el.querySelector('.ai, .ri');
          LD.dockDragIn(appId, ev, ic.getBoundingClientRect(), 'launchpad');
        }
      };
      const up = () => { cleanup(); if (!started) { close(); LD.openApp(appId); } };
      const cleanup = () => { removeEventListener('pointermove', move); removeEventListener('pointerup', up); };
      addEventListener('pointermove', move);
      addEventListener('pointerup', up);
    });
    el.addEventListener('contextmenu', e => { e.preventDefault(); appMenu(appId, e.clientX, e.clientY); });
  }

  function appMenu(appId, x, y) {
    const pinned = LD.state.lpPinned.includes(appId);
    const inDock = LD.isPinned(appId);
    LD.menu(x, y, [
      { label: 'Open', onClick: () => { close(); LD.openApp(appId); } },
      { label: 'Run as Administrator', onClick: () => { close(); LD.launch(appId); } },
      { sep: true },
      { label: pinned ? 'Unpin from Launchpad' : 'Pin to Launchpad', onClick: () => togglePin(appId) },
      { label: inDock ? 'Remove from Dock' : 'Add to Dock', onClick: () => LD.pinToDock(appId, !inDock) },
      { label: 'Open File Location', onClick: () => LD.toast('files', 'Files', `Would open the folder containing ${LD.APPS[appId].name}.`) },
      { sep: true },
      { label: 'Uninstall…', disabled: true },
    ]);
  }

  function togglePin(appId) {
    const i = LD.state.lpPinned.indexOf(appId);
    if (i >= 0) LD.state.lpPinned.splice(i, 1); else LD.state.lpPinned.push(appId);
    LD.save();
    if (root.innerHTML) renderHome();
  }

  /* ---------- Centre: search ---------- */
  function score(q, text) {
    const t = text.toLowerCase();
    if (t.startsWith(q)) return 100 - t.length * 0.1;
    const words = t.split(/[\s\-_.\\&]+/);
    const wi = words.findIndex(w => w.startsWith(q));
    if (wi >= 0) return 80 - wi;
    if (t.includes(q)) return 60;
    let ti = 0, s = 0, run = 0;
    for (const ch of q) {
      const idx = t.indexOf(ch, ti);
      if (idx < 0) return -1;
      run = idx === ti ? run + 1 : 0;
      s += 1 + run;
      ti = idx + 1;
    }
    return 10 + s - t.length * 0.05;
  }
  function hl(text, q) {
    const i = text.toLowerCase().indexOf(q);
    if (i < 0) return LD.esc(text);
    return LD.esc(text.slice(0, i)) + '<mark>' + LD.esc(text.slice(i, i + q.length)) + '</mark>' + LD.esc(text.slice(i + q.length));
  }

  let results = [], sel = 0;
  function search(raw) {
    const q = raw.trim().toLowerCase();
    const home = root.querySelector('[data-home]');
    const box = root.querySelector('[data-results]');
    if (!q) { home.hidden = false; box.hidden = true; results = []; return; }
    home.hidden = true; box.hidden = false;

    const apps = Object.keys(LD.APPS).filter(a => !LD.APPS[a].hidden)
      .map(a => ({ kind: 'app', id: a, name: LD.APPS[a].name, s: score(q, LD.APPS[a].name) }))
      .filter(r => r.s > 0).sort((a, b) => b.s - a.s).slice(0, 8);
    const files = FILES.map(f => ({ kind: 'file', f, name: f.name, s: score(q, f.name) - 8 })).filter(r => r.s > 0).sort((a, b) => b.s - a.s).slice(0, 5);
    const sets = SETTINGS.map(st => ({ kind: 'setting', st, name: st.name, s: score(q, st.name) - 6 })).filter(r => r.s > 0).sort((a, b) => b.s - a.s).slice(0, 5);

    const pool = [...apps, ...files, ...sets].sort((a, b) => b.s - a.s);
    const top = pool[0];
    results = [];
    let html = '';
    const row = (r, cls = '') => {
      results.push(r);
      const i = results.length - 1;
      let icon, title, sub;
      if (r.kind === 'app') { icon = `<div class="ri">${LD.icon(r.id)}</div>`; title = hl(r.name, q); sub = LD.isRunning(r.id) ? 'Application · Running' : 'Application'; }
      else if (r.kind === 'file') { icon = `<div class="rg">${ui(r.f.kind === 'folder' ? 'folder' : r.f.kind === 'image' ? 'image' : 'file', 16)}</div>`; title = hl(r.f.name, q); sub = r.f.path; }
      else if (r.kind === 'setting') { icon = `<div class="rg">${ui(r.st.icon, 16)}</div>`; title = hl(r.st.name, q); sub = r.st.where; }
      else { icon = `<div class="rg">${ui('globe', 16)}</div>`; title = `Search the web for “${LD.esc(raw.trim())}”`; sub = 'Opens your browser'; }
      return `<div class="res ${cls}" data-i="${i}">${icon}<div class="rt"><b>${title}</b><span>${LD.esc(sub)}</span></div><span class="rk">↵ Open</span></div>`;
    };
    if (top) { html += `<div class="res-h">Top Hit</div>` + row(top, 'top'); }
    const rest = arr => arr.filter(r => r !== top);
    if (rest(apps).length) html += `<div class="res-h">Apps</div>` + rest(apps).map(r => row(r)).join('');
    if (rest(files).length) html += `<div class="res-h">Files</div>` + rest(files).map(r => row(r)).join('');
    if (rest(sets).length) html += `<div class="res-h">Settings</div>` + rest(sets).map(r => row(r)).join('');
    html += `<div class="res-h">Web</div>` + row({ kind: 'web', q: raw.trim() });
    box.innerHTML = html;
    box.querySelectorAll('.res').forEach(el => {
      const r = results[+el.dataset.i];
      el.addEventListener('pointerenter', () => select(+el.dataset.i));
      el.addEventListener('click', () => runResult(r));
      if (r.kind === 'app') {
        el.dataset.app = r.id;
        el.addEventListener('contextmenu', e => { e.preventDefault(); appMenu(r.id, e.clientX, e.clientY); });
      }
    });
    select(0);
  }
  function select(i) {
    sel = LD.clamp(i, 0, results.length - 1);
    root.querySelectorAll('[data-results] .res').forEach(el => el.classList.toggle('sel', +el.dataset.i === sel));
    const el = root.querySelector(`[data-results] .res[data-i="${sel}"]`);
    el && el.scrollIntoView({ block: 'nearest' });
  }
  function runResult(r) {
    if (!r) return;
    if (r.kind === 'app') { close(); LD.openApp(r.id); }
    else if (r.kind === 'file') { close(); LD.toast(r.f.kind === 'image' ? 'photos' : 'files', r.f.name, 'Would open ' + r.f.path + '\\' + r.f.name); }
    else if (r.kind === 'setting') { close(); if (r.st.section) LD.openSettings(r.st.section); else LD.launch('system', { title: r.st.name + ' — System Settings' }); }
    else { close(); LD.launch('browser', { page: 0, title: `“${r.q}” — Search — Browser` }); }
  }
  function onSearchKey(e) {
    if (e.key === 'ArrowDown') { e.preventDefault(); select(sel + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); select(sel - 1); }
    else if (e.key === 'Enter') { e.preventDefault(); runResult(results[sel]); }
    else if (e.key === 'Escape') {
      e.preventDefault(); e.stopPropagation();
      if (e.target.value) { e.target.value = ''; search(''); } else close();
    }
  }

  /* ---------- Left: widgets ---------- */
  const sys = { cpu: 14, mem: 11.8, gpu: 7, disk: 61, down: 2.4, up: 0.3, hist: Array.from({ length: 40 }, () => 10 + Math.random() * 10) };
  function renderWidgets() {
    const w = root.querySelector('[data-widgets]');
    const now = new Date();
    const first = new Date(now.getFullYear(), now.getMonth(), 1);
    const start = new Date(first); start.setDate(1 - first.getDay());
    let cal = ['S', 'M', 'T', 'W', 'T', 'F', 'S'].map(d => `<div class="dow">${d}</div>`).join('');
    for (let i = 0; i < 42; i++) {
      const d = new Date(start); d.setDate(start.getDate() + i);
      const out = d.getMonth() !== now.getMonth();
      if (i >= 35 && out) break;
      const today = d.toDateString() === now.toDateString();
      cal += `<div><div class="d ${out ? 'out' : ''} ${today ? 'today' : ''}">${d.getDate()}</div></div>`;
    }
    const hours = ['Now', '+1h', '+2h', '+3h', '+4h', '+5h'].map((h, i) =>
      `<div>${h}${ui(i < 2 ? 'cloud' : i < 4 ? 'sun' : 'moon', 16)}<b>${[7, 8, 9, 8, 6, 4][i]}°</b></div>`).join('');
    w.innerHTML = `
      <div class="w-card"><div class="w-time" data-time></div><div class="w-date" data-date></div>
        <div class="w-world" data-world></div></div>
      <div class="w-card w-weather">
        <div class="top"><div><div class="city">Toronto</div><div class="deg">7°</div></div>
          <div class="cond">${ui('cloud', 26)}<div style="margin-top:4px">Partly Cloudy</div><div>H:9° L:2°</div></div></div>
        <div class="w-hours">${hours}</div>
      </div>
      <div class="w-card">
        <div class="w-title">${ui('activity', 15)} System</div>
        <svg class="w-spark" viewBox="0 0 200 34" preserveAspectRatio="none"><path data-spark fill="none" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round"/><path data-sparkf fill="var(--accent)" opacity=".14"/></svg>
        <div class="w-sys">
          <div class="w-row"><span>CPU</span><div class="w-bar"><i data-cpu></i></div><b data-cpuv></b></div>
          <div class="w-row"><span>Memory</span><div class="w-bar"><i data-mem></i></div><b data-memv></b></div>
          <div class="w-row"><span>GPU</span><div class="w-bar"><i data-gpu></i></div><b data-gpuv></b></div>
          <div class="w-row"><span>Disk C:</span><div class="w-bar"><i style="width:61%"></i></div><b>61%</b></div>
          <div class="w-row"><span>Network</span><span style="color:var(--text-2);font-size:11.5px" data-net></span><b></b></div>
        </div>
      </div>
      <div class="w-card">
        <div class="w-cal-head">${now.toLocaleDateString(undefined, { month: 'long' })}<span>${now.getFullYear()}</span></div>
        <div class="w-cal">${cal}</div>
        <div class="w-event"><b>Design review</b><span>Today · 2:00 – 2:45 PM</span></div>
      </div>
      <button class="pill-btn w-add" data-act="add">${'+ Add Widget'}</button>`;
    w.querySelector('[data-act="add"]').addEventListener('click', () => LD.toast('laminar', 'Widgets', 'The widget gallery comes in a later phase.'));
    updateLive();
  }

  function updateLive() {
    const w = root.querySelector('[data-widgets]');
    if (!w) return;
    const now = new Date();
    const t = now.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
    const m = t.match(/^(.*?)(\s?[AP]M)$/i);
    w.querySelector('[data-time]').innerHTML = m ? `${m[1]}<small>${m[2].trim()}</small>` : t;
    w.querySelector('[data-date]').textContent = now.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
    const zone = (tz, label) => {
      let v;
      try { v = now.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit', timeZone: tz }); } catch (e) { v = '--'; }
      return `<div><b>${v.replace(/\s?[AP]M/i, '')}</b>${label}</div>`;
    };
    w.querySelector('[data-world]').innerHTML = zone('Europe/London', 'London') + zone('Asia/Tokyo', 'Tokyo') + zone('America/Vancouver', 'Vancouver');

    // Random-walk system numbers (the native app reads real counters).
    sys.cpu = LD.clamp(sys.cpu + (Math.random() - 0.5) * 9, 3, 78);
    sys.mem = LD.clamp(sys.mem + (Math.random() - 0.5) * 0.3, 9, 20);
    sys.gpu = LD.clamp(sys.gpu + (Math.random() - 0.5) * 6, 1, 60);
    sys.down = Math.max(0.1, sys.down + (Math.random() - 0.5) * 1.4);
    sys.hist.push(sys.cpu); sys.hist.shift();
    const set = (k, pct, txt) => { w.querySelector(`[data-${k}]`).style.width = pct + '%'; w.querySelector(`[data-${k}v]`).textContent = txt; };
    set('cpu', sys.cpu, Math.round(sys.cpu) + '%');
    set('mem', sys.mem / 32 * 100, sys.mem.toFixed(1) + ' GB');
    set('gpu', sys.gpu, Math.round(sys.gpu) + '%');
    w.querySelector('[data-net]').textContent = `↓ ${sys.down.toFixed(1)} MB/s  ↑ ${sys.up.toFixed(1)} MB/s`;
    const pts = sys.hist.map((v, i) => `${(i / (sys.hist.length - 1) * 200).toFixed(1)},${(34 - v / 80 * 32).toFixed(1)}`);
    w.querySelector('[data-spark]').setAttribute('d', 'M' + pts.join('L'));
    w.querySelector('[data-sparkf]').setAttribute('d', 'M0,34L' + pts.join('L') + 'L200,34Z');
  }

  /* ---------- Right: control center ---------- */
  function tog(on, icon, extra = '') { return `<span class="cc-tog ${on ? 'on' : ''} ${extra}">${ui(icon, 16, 2)}</span>`; }

  function renderCC() {
    const el = root.querySelector('[data-cc]');
    const M = LD.media, t = M.track;
    const dark = document.documentElement.classList.contains('theme-dark');
    el.innerHTML = `
      <div class="cc-grid">
        <div class="cc-tile cc-2x2 cc-conn">
          <div class="cc-crow" data-k="wifi">${tog(cc.wifi, 'wifi')}<div class="lbl">Wi-Fi<span>${cc.wifi ? cc.nets.find(n => n.on).name : 'Off'}</span></div></div>
          <div class="cc-crow" data-k="bt">${tog(cc.bt, 'bluetooth')}<div class="lbl">Bluetooth<span>${cc.bt ? cc.devs.filter(d => d.on).length + ' connected' : 'Off'}</span></div></div>
          <div class="cc-crow" data-k="share">${tog(cc.share, 'share')}<div class="lbl">Nearby Share<span>${cc.share ? 'Everyone nearby' : 'Off'}</span></div></div>
        </div>
        <div class="cc-tile cc-2x2 cc-media">
          <div class="art">${LD.artSVG(t.c)}</div>
          <div class="meta">${LD.esc(t.title)}<span>${LD.esc(t.artist)}</span></div>
          <div class="prog"><i data-prog style="width:${M.pos / t.dur * 100}%"></i></div>
          <div class="ctrls"><button data-m="prev">${ui('prev', 15)}</button><button data-m="play">${ui(M.playing ? 'pause' : 'play', 18)}</button><button data-m="next">${ui('next', 15)}</button></div>
        </div>
        <button class="cc-tile cc-2x1 cc-wide" data-k="focus">${tog(cc.focus, 'moon', 'focus')}<div class="lbl">Focus<span>${cc.focus ? 'Do Not Disturb' : 'Off'}</span></div></button>
        <div class="cc-tile cc-small" title="Dark Mode"><button data-k="dark">${tog(dark, 'contrast')}</button></div>
        <div class="cc-tile cc-small" title="Night Light"><button data-k="night">${tog(cc.night, 'nightlight')}</button></div>
        <div class="cc-tile cc-small" title="Energy Saver"><button data-k="saver">${tog(cc.saver, 'leaf')}</button></div>
        <div class="cc-tile cc-small" title="Snip a region"><button data-k="snip">${tog(false, 'scissors')}</button></div>
        <div class="cc-tile cc-small" title="Cast to a display"><button data-k="cast">${tog(cc.cast, 'cast')}</button></div>
        <div class="cc-tile cc-small" title="Mute microphone"><button data-k="mic">${tog(cc.mic, 'mic')}</button></div>
        <div class="cc-tile cc-4x1 cc-slider">
          <div class="top">Sound <button data-k="sound">${LD.esc(cc.outputs[cc.output].split(' (')[0])} ${ui('chevron', 12, 2.2)}</button></div>
          <div class="row">${ui('speakerLow', 14)}<input class="big-slider" type="range" min="0" max="100" value="${cc.volume}" style="--p:${cc.volume}%" data-vol aria-label="Volume">${ui('speaker', 16)}</div>
        </div>
      </div>
      <div class="cc-sec">Background Apps <span style="font-weight:400;color:var(--text-3)">${TRAY.length}</span></div>
      <div class="tray">${TRAY.map(a => `<button data-tray="${a.id}" title="${LD.esc(a.name)} — ${LD.esc(a.status)}">${ui(a.icon, 19)}${a.dot ? `<i style="background:${a.dot}"></i>` : ''}</button>`).join('')}</div>
      <div class="cc-update"><div class="ui">${ui('download', 18)}</div><div class="tx">Windows Update<span>2 updates ready · restart required</span></div><button class="btn primary" data-k="restart">Restart</button></div>
      <div class="cc-foot"><button class="pill-btn" data-k="edit">Edit Controls</button></div>`;

    el.querySelectorAll('.cc-crow').forEach(row => {
      const k = row.dataset.k;
      row.querySelector('.cc-tog').addEventListener('click', e => { e.stopPropagation(); cc[k] = !cc[k]; renderCC(); });
      row.addEventListener('click', () => { if (k !== 'share') showDetail(k); });
    });
    el.querySelectorAll('[data-k]').forEach(b => {
      const k = b.dataset.k;
      if (b.classList.contains('cc-crow')) return;
      b.addEventListener('click', () => {
        if (k === 'dark') { LD.set('theme', dark ? 'light' : 'dark'); renderCC(); }
        else if (k === 'snip') { close(); LD.launch('snip'); }
        else if (k === 'sound') showDetail('sound');
        else if (k === 'restart') LD.toast('system', 'Windows Update', 'Would restart to install 2 updates (after saving your work).');
        else if (k === 'edit') LD.toast('laminar', 'Edit Controls', 'Choosing and arranging controls comes in a later phase.');
        else { cc[k] = !cc[k]; renderCC(); }
      });
    });
    el.querySelectorAll('[data-m]').forEach(b => b.addEventListener('click', () => {
      const a = b.dataset.m;
      if (a === 'play') LD.media.toggle(); else LD.media.next(a === 'next' ? 1 : -1);
    }));
    const vol = el.querySelector('[data-vol]');
    vol.addEventListener('input', () => { cc.volume = +vol.value; vol.style.setProperty('--p', vol.value + '%'); });
    el.querySelectorAll('[data-tray]').forEach(b => {
      const a = TRAY.find(x => x.id === b.dataset.tray);
      const menu = e => {
        e.preventDefault();
        const r = b.getBoundingClientRect();
        LD.menu(r.left, r.bottom + 6, [
          { header: a.name },
          { label: a.status, disabled: true },
          { sep: true },
          { label: 'Open ' + a.name, onClick: () => LD.toast('laminar', a.name, 'Would open the app’s window (tray click).') },
          { label: 'Settings…', onClick: () => LD.toast('laminar', a.name, 'Would open its settings.') },
          { sep: true },
          { label: 'Quit', danger: true, onClick: () => LD.toast('laminar', a.name, 'Would ask the app to exit.') },
        ]);
      };
      b.addEventListener('click', menu);
      b.addEventListener('contextmenu', menu);
    });
  }

  function bars(n) { return `<span class="bars">${[1, 2, 3, 4].map(i => `<i style="height:${i * 25}%" class="${i > n ? 'off' : ''}"></i>`).join('')}</span>`; }

  function showDetail(k) {
    cc.view = k;
    const d = root.querySelector('[data-ccd]');
    const head = (title, on) => `<div class="cc-dhead"><button class="round-btn" data-back>${ui('chevronLeft', 18)}</button><h3>${title}</h3>${on != null ? `<button class="switch" role="switch" aria-checked="${on}" data-sw></button>` : ''}</div>`;
    if (k === 'wifi') {
      d.innerHTML = head('Wi-Fi', cc.wifi) + `<div class="cc-list">${cc.nets.map((n, i) => `<div class="cc-li ${n.on ? 'on' : ''}" data-i="${i}">
          ${bars(n.s)}<div class="nm">${LD.esc(n.name)}${n.on ? '<span>Connected, secured</span>' : ''}</div>${n.lock ? ui('lock', 13) : ''}${n.on ? `<span class="ok">${ui('check', 16, 2.4)}</span>` : ''}</div>`).join('')}</div>
        <div class="cc-foot"><button class="pill-btn" data-more>Network & Internet Settings…</button></div>`;
      d.querySelectorAll('.cc-li').forEach(li => li.addEventListener('click', () => {
        cc.nets.forEach((n, i) => (n.on = i === +li.dataset.i)); cc.wifi = true; showDetail('wifi'); renderCC();
      }));
    } else if (k === 'bt') {
      d.innerHTML = head('Bluetooth', cc.bt) + `<div class="cc-list">${cc.devs.map((v, i) => `<div class="cc-li ${v.on ? 'on' : ''}" data-i="${i}">
          <span class="dev">${ui(v.icon, 15)}</span><div class="nm">${LD.esc(v.name)}<span>${v.on ? 'Connected' + (v.batt ? ' · ' + v.batt + '%' : '') : 'Not connected'}</span></div></div>`).join('')}</div>
        <div class="cc-foot"><button class="pill-btn" data-more>Bluetooth & Devices Settings…</button></div>`;
      d.querySelectorAll('.cc-li').forEach(li => li.addEventListener('click', () => {
        const v = cc.devs[+li.dataset.i]; v.on = !v.on; showDetail('bt'); renderCC();
      }));
    } else if (k === 'sound') {
      d.innerHTML = head('Sound') + `<div class="cc-list">${cc.outputs.map((o, i) => `<div class="cc-li ${i === cc.output ? 'on' : ''}" data-i="${i}">
          <span class="dev">${ui(i === 1 ? 'headphones' : i === 2 ? 'tv' : 'speaker', 15)}</span><div class="nm">${LD.esc(o)}</div>${i === cc.output ? `<span class="ok">${ui('check', 16, 2.4)}</span>` : ''}</div>`).join('')}</div>
        <div class="cc-sec">Volume Mixer</div>
        <div class="cc-list cc-mix">${Object.keys(cc.mix).map(a => `<div class="m"><span class="mi">${LD.icon(a)}</span><span>${LD.APPS[a].name}</span>
          <input class="slider" type="range" min="0" max="100" value="${cc.mix[a]}" style="--p:${cc.mix[a]}%" data-mix="${a}"><b>${cc.mix[a]}</b></div>`).join('')}</div>`;
      d.querySelectorAll('.cc-li').forEach(li => li.addEventListener('click', () => { cc.output = +li.dataset.i; showDetail('sound'); renderCC(); }));
      d.querySelectorAll('[data-mix]').forEach(s => s.addEventListener('input', () => {
        cc.mix[s.dataset.mix] = +s.value; s.style.setProperty('--p', s.value + '%'); s.nextElementSibling.textContent = s.value;
      }));
    }
    d.querySelector('[data-back]').addEventListener('click', hideDetail);
    const sw = d.querySelector('[data-sw]');
    sw && sw.addEventListener('click', () => { cc[k] = !cc[k]; sw.setAttribute('aria-checked', cc[k]); renderCC(); });
    const more = d.querySelector('[data-more]');
    more && more.addEventListener('click', () => { close(); LD.launch('system', { title: (k === 'wifi' ? 'Network & internet' : 'Bluetooth & devices') + ' — System Settings' }); });
    root.querySelector('.cc-views').classList.add('detail');
  }
  function hideDetail() { cc.view = null; root.querySelector('.cc-views').classList.remove('detail'); }

  LD.on('media', () => { if (isOpen) renderCC(); });
  LD.on('media-tick', () => {
    if (!isOpen) return;
    const p = root.querySelector('[data-prog]');
    if (p) p.style.width = (LD.media.pos / LD.media.track.dur * 100) + '%';
  });

  /* ---------- Open / close ---------- */
  function open() {
    if (isOpen) return;
    if (!root.innerHTML) mount();
    isOpen = true;
    LD.previews.hide(); LD.stacks.close(); LD.menu.close();
    LD.dock.reveal(true);
    root.classList.add('mounted');
    root.setAttribute('aria-hidden', 'false');
    document.body.classList.add('lp-open');
    const input = root.querySelector('.lp-search input');
    input.value = ''; search('');
    hideDetail();
    renderCC();
    requestAnimationFrame(() => requestAnimationFrame(() => root.classList.add('open')));
    setTimeout(() => input.focus({ preventScroll: true }), 60);
    // Live widgets only tick while Launchpad is visible (idle = no work).
    updateLive();
    clearInterval(liveT);
    liveT = setInterval(updateLive, 1000);
    LD.dock.render();
  }
  function close() {
    if (!isOpen) return;
    isOpen = false;
    clearInterval(liveT);
    root.classList.remove('open');
    root.setAttribute('aria-hidden', 'true');
    document.body.classList.remove('lp-open');
    root.querySelector('.lp-search input').blur();
    setTimeout(() => { if (!isOpen) root.classList.remove('mounted'); }, 380);
    LD.dock.render();
    LD.dock.maybeHide();
  }

  // Typing anywhere while Launchpad is open goes to search.
  document.addEventListener('keydown', e => {
    if (!isOpen || e.ctrlKey || e.metaKey || e.altKey) return;
    const input = root.querySelector('.lp-search input');
    if (document.activeElement === input) return;
    if (document.activeElement && document.activeElement.matches('input, textarea')) return;
    if (e.key === 'Escape') { if (!LD.menu.isOpen()) close(); return; }
    if (e.key.length === 1) { input.focus(); }
  });

  LD.isRunning = id => LD.wm.byApp(id).length > 0;
  LD.launchpad = {
    open, close, toggle: () => (isOpen ? close() : open()), isOpen: () => isOpen,
    togglePin, applyLayout, refresh() { if (root.innerHTML) { renderHome(); renderCC(); } },
  };
})(window.LD);
