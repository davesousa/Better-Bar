/* Laminar Dock prototype — Laminar Settings.
   Built from a schema: each row maps to one key in LD.settings. Changing a
   control calls LD.set(key, value), and main.js applies it live. */
(function (LD) {
  'use strict';
  const ui = LD.ui;
  const pct = v => Math.round(v * 100) + '%';
  const NATIVE = '<span class="tag">native app</span>';

  const SECTIONS = [
    { id: 'appearance', name: 'Appearance', icon: 'sparkle', color: '#5E5CE6', lead: 'Theme, accent color and wallpaper.', groups: [[
      { key: 'theme', type: 'seg', label: 'Theme', hint: 'Auto matches the wallpaper (light on Dawn, dark otherwise).', options: [['auto', 'Auto'], ['light', 'Light'], ['dark', 'Dark']] },
      { key: 'accent', type: 'swatch', label: 'Accent color', options: ['#0A84FF', '#5E5CE6', '#BF5AF2', '#FF375F', '#FF9F0A', '#FFD60A', '#30D158', '#64D2FF', '#8E8E93'] },
      { key: 'wallpaper', type: 'wall', label: 'Wallpaper', options: [['dusk', 'Dusk'], ['dawn', 'Dawn'], ['graphite', 'Graphite']] },
    ]] },
    { id: 'glass', name: 'Glass', icon: 'contrast', color: '#32ADE6', preview: true, lead: 'The material every Laminar surface is made of.', groups: [[
      { key: 'glassTint', type: 'range', label: 'Tint', hint: 'Lower is more transparent.', min: 0, max: 0.8, step: 0.01, fmt: pct },
      { key: 'glassBlur', type: 'range', label: 'Blur', hint: 'Panels use about 1.8× this.', min: 0, max: 40, step: 1, fmt: v => v + ' px' },
      { key: 'glassSaturation', type: 'range', label: 'Vibrancy', hint: 'Saturation of what shows through.', min: 100, max: 260, step: 5, fmt: v => v + '%' },
      { key: 'edgeHighlight', type: 'range', label: 'Edge highlight', min: 0, max: 1, step: 0.01, fmt: pct },
      { key: 'shadowStrength', type: 'range', label: 'Shadow', min: 0, max: 1, step: 0.01, fmt: pct },
    ], [
      { key: 'refraction', type: 'switch', label: 'Refraction', hint: 'The dock bends the wallpaper at its rounded edges, like thick glass.' },
      { key: 'refractionStrength', type: 'range', label: 'Refraction strength', min: 0, max: 100, step: 1, fmt: v => v + '%', dep: 'refraction' },
    ]] },
    { id: 'dock', name: 'Dock', icon: 'grid', color: '#0A84FF', lead: 'Size, spacing, magnification and indicators.', groups: [[
      { key: 'iconSize', type: 'range', label: 'Icon size', min: 32, max: 96, step: 1, fmt: v => v + ' px' },
      { key: 'iconSpacing', type: 'range', label: 'Icon spacing', min: 0, max: 24, step: 1, fmt: v => v + ' px' },
      { key: 'dockPadding', type: 'range', label: 'Shelf padding', min: 4, max: 20, step: 1, fmt: v => v + ' px' },
      { key: 'dockRadius', type: 'range', label: 'Corner radius', min: 6, max: 40, step: 1, fmt: v => v + ' px' },
    ], [
      { key: 'magnify', type: 'switch', label: 'Magnification', hint: 'Icons grow as the pointer moves across the dock.' },
      { key: 'magnifySize', type: 'range', label: 'Magnified size', min: 40, max: 160, step: 1, fmt: v => v + ' px', dep: 'magnify' },
      { key: 'magnifyRange', type: 'range', label: 'Magnification spread', min: 1, max: 6, step: 0.1, fmt: v => v.toFixed(1) + ' icons', dep: 'magnify' },
    ], [
      { key: 'indicatorStyle', type: 'seg', label: 'Open-app indicator', options: [['dot', 'Dot'], ['bar', 'Bar']] },
      { key: 'showClock', type: 'switch', label: 'Show clock in the dock' },
      { key: 'bounceOnLaunch', type: 'switch', label: 'Bounce while apps open' },
      { key: 'minimizeEffect', type: 'seg', label: 'Minimize effect', options: [['genie', 'Genie'], ['scale', 'Scale']] },
    ]] },
    { id: 'behavior', name: 'Behavior', icon: 'sliders', color: '#30D158', lead: 'Hiding, clicking, keyboard and multiple displays.', groups: [[
      { key: 'autoHide', type: 'switch', label: 'Automatically hide and show the dock', hint: 'The dock slides off-screen. Push the pointer into the bottom edge to bring it back over your windows.' },
      { key: 'revealZone', type: 'range', label: 'Reveal zone height', hint: 'How close to the bottom edge the pointer must be.', min: 1, max: 40, step: 1, fmt: v => v + ' px', dep: 'autoHide' },
      { key: 'revealDelay', type: 'range', label: 'Reveal delay', min: 0, max: 1000, step: 10, fmt: v => v + ' ms', dep: 'autoHide' },
      { key: 'hideDelay', type: 'range', label: 'Hide delay', min: 0, max: 2000, step: 10, fmt: v => v + ' ms', dep: 'autoHide' },
    ], [
      { key: 'clickToMinimize', type: 'switch', label: 'Click the focused app to minimize it', hint: 'Windows taskbar behavior.' },
      { key: 'winKeyLaunchpad', type: 'switch', label: 'Windows key opens Launchpad', native: true, hint: 'In this prototype use Ctrl+Space or `.' },
    ], [
      { key: 'displays', type: 'select', label: 'Show the dock on', native: true, options: [['all', 'All displays'], ['main', 'Main display only'], ['pointer', 'The display with the pointer']] },
      { key: 'perDisplayApps', type: 'seg', label: 'Each dock shows', native: true, options: [['all', 'All apps'], ['own', 'That display’s windows']] },
    ]] },
    { id: 'launchpad', name: 'Launchpad', icon: 'grid', color: '#FF9F0A', lead: 'Panels, icon size and background.', groups: [[
      { key: 'showWidgets', type: 'switch', label: 'Show widgets panel' },
      { key: 'showControl', type: 'switch', label: 'Show control center panel' },
      { key: 'lpIconSize', type: 'range', label: 'Icon size', min: 44, max: 88, step: 1, fmt: v => v + ' px' },
      { key: 'lpBlur', type: 'range', label: 'Background blur', min: 0, max: 80, step: 1, fmt: v => v + ' px' },
    ]] },
    { id: 'motion', name: 'Motion', icon: 'activity', color: '#FF375F', lead: 'Animation speed and spring feel.', groups: [[
      { key: 'animSpeed', type: 'range', label: 'Animation duration', hint: 'Lower is faster.', min: 0.5, max: 2, step: 0.05, fmt: v => v.toFixed(2) + '×' },
      { key: 'bounciness', type: 'range', label: 'Spring bounciness', min: 0, max: 0.6, step: 0.01, fmt: pct },
      { key: 'reduceMotion', type: 'switch', label: 'Reduce motion', hint: 'Turns animations into quick fades.' },
    ]] },
    { id: 'about', name: 'About', icon: 'cpu', color: '#8E8E93' },
  ];

  let host = null, current = 'appearance', query = '';

  function rowHTML(r) {
    const v = LD.settings[r.key];
    let ctl = '';
    if (r.type === 'range') {
      const p = (v - r.min) / (r.max - r.min) * 100;
      ctl = `<div class="rng"><input class="slider" type="range" min="${r.min}" max="${r.max}" step="${r.step}" value="${v}" style="--p:${p}%"><span class="val">${r.fmt(v)}</span></div>`;
    } else if (r.type === 'switch') {
      ctl = `<button class="switch" role="switch" aria-checked="${!!v}" aria-label="${LD.esc(r.label)}"></button>`;
    } else if (r.type === 'seg') {
      ctl = `<div class="seg">${r.options.map(([k, l]) => `<button data-v="${k}" aria-pressed="${v === k}">${l}</button>`).join('')}</div>`;
    } else if (r.type === 'select') {
      ctl = `<select class="sel">${r.options.map(([k, l]) => `<option value="${k}" ${v === k ? 'selected' : ''}>${l}</option>`).join('')}</select>`;
    } else if (r.type === 'swatch') {
      ctl = `<div class="swatches">${r.options.map(c => `<button data-v="${c}" style="background:${c};color:${c}" aria-pressed="${v === c}" aria-label="${c}"></button>`).join('')}</div>`;
    } else if (r.type === 'wall') {
      ctl = `<div class="walls">${r.options.map(([k, l]) => `<button data-v="${k}" aria-pressed="${v === k}"><div class="wt">${LD.wallpaperSVG(k)}</div>${l}</button>`).join('')}</div>`;
    }
    const off = r.dep && !LD.settings[r.dep] ? ' off' : '';
    return `<div class="set-row${off}" data-key="${r.key}"><div class="lab">${LD.esc(r.label)}${r.native ? NATIVE : ''}${r.hint ? `<small>${LD.esc(r.hint)}</small>` : ''}</div><div class="ctl">${ctl}</div></div>`;
  }

  function glassDemo() {
    return `<div class="glass-demo"><div class="wpc">${LD.wallpaperSVG(LD.settings.wallpaper)}</div>
      <div class="card glass">${['files', 'browser', 'music', 'notes', 'code'].map(a => `<div class="mini">${LD.icon(a)}</div>`).join('')}</div>
      <div class="note">Live sample. The real dock below updates too.</div></div>`;
  }

  function aboutHTML() {
    const json = JSON.stringify({ settings: LD.settings, dock: LD.state.dock }, null, 2);
    return `<div class="about-hero"><div class="ai">${LD.icon('launch')}</div><div><b>Laminar Dock</b><span>Prototype 0.1 · HTML/CSS look-and-feel study</span></div></div>
      <div class="set-group"><table class="kv">
        <tr><td>Native stack</td><td>Rust · Windows Composition · DirectWrite</td></tr>
        <tr><td>Target memory (dock)</td><td>&lt; 40 MB</td></tr>
        <tr><td>Target idle CPU</td><td>≈ 0%</td></tr>
        <tr><td>Target launch time</td><td>&lt; 200 ms</td></tr>
        <tr><td>Target frame rate</td><td>Matches the display (60–240 Hz)</td></tr>
      </table></div>
      <div class="set-main-sub" style="font-size:12.5px;font-weight:600;color:var(--text-2);margin:4px 2px 8px">config.json (the shape the native app will read)</div>
      <div class="set-group"><pre class="json">${LD.esc(json)}</pre></div>
      <div class="set-group"><div class="btn-row">
        <button class="btn" data-reset="settings">${ui('restart', 14)} Reset settings</button>
        <button class="btn" data-reset="dock">${ui('grid', 14)} Reset dock layout</button>
        <button class="btn" data-reset="all">Reset everything</button>
      </div></div>`;
  }

  function render() {
    if (!host) return;
    const sec = SECTIONS.find(s => s.id === current) || SECTIONS[0];
    host.innerHTML = `<div class="set">
      <nav class="set-side">
        <label class="set-search">${ui('search', 14)}<input placeholder="Search settings" value="${LD.esc(query)}" spellcheck="false"></label>
        ${SECTIONS.map(s => `<div class="set-nav ${s.id === current && !query ? 'on' : ''}" data-sec="${s.id}"><span class="sq" style="background:${s.color}">${ui(s.icon, 14, 2)}</span>${s.name}</div>`).join('')}
      </nav>
      <main class="set-main"></main></div>`;
    const main = host.querySelector('.set-main');
    if (query) {
      const q = query.toLowerCase();
      const rows = [];
      SECTIONS.forEach(s => (s.groups || []).flat().forEach(r => { if ((r.label + ' ' + (r.hint || '') + ' ' + s.name).toLowerCase().includes(q)) rows.push(r); }));
      main.innerHTML = `<h2>Results</h2><p class="lead">${rows.length} matching ${rows.length === 1 ? 'setting' : 'settings'}</p>` +
        (rows.length ? `<div class="set-group">${rows.map(rowHTML).join('')}</div>` : '');
    } else if (sec.id === 'about') {
      main.innerHTML = `<h2>About</h2><p class="lead">What this prototype is, and what the native app aims for.</p>` + aboutHTML();
    } else {
      main.innerHTML = `<h2>${sec.name}</h2><p class="lead">${sec.lead}</p>` + (sec.preview ? glassDemo() : '') +
        sec.groups.map(g => `<div class="set-group">${g.map(rowHTML).join('')}</div>`).join('');
    }
    wire();
  }

  function rowDef(key) { for (const s of SECTIONS) for (const r of (s.groups || []).flat()) if (r.key === key) return r; return null; }

  function wire() {
    host.querySelectorAll('.set-nav').forEach(n => n.addEventListener('click', () => { current = n.dataset.sec; query = ''; render(); }));
    const si = host.querySelector('.set-search input');
    si.addEventListener('input', () => {
      query = si.value;
      const pos = si.selectionStart;
      render();
      const n = host.querySelector('.set-search input');
      n.focus(); n.setSelectionRange(pos, pos);
    });
    host.querySelectorAll('.set-row').forEach(row => {
      const r = rowDef(row.dataset.key);
      if (!r) return;
      if (r.type === 'range') {
        const inp = row.querySelector('input');
        inp.addEventListener('input', () => {
          const v = +inp.value;
          inp.style.setProperty('--p', ((v - r.min) / (r.max - r.min) * 100) + '%');
          row.querySelector('.val').textContent = r.fmt(v);
          LD.set(r.key, v);
        });
      } else if (r.type === 'switch') {
        const b = row.querySelector('.switch');
        b.addEventListener('click', () => LD.set(r.key, !LD.settings[r.key]));
      } else if (r.type === 'select') {
        const s = row.querySelector('select');
        s.addEventListener('change', () => LD.set(r.key, s.value));
      } else {
        row.querySelectorAll('[data-v]').forEach(b => b.addEventListener('click', () => LD.set(r.key, b.dataset.v)));
      }
    });
    host.querySelectorAll('[data-reset]').forEach(b => b.addEventListener('click', () => {
      const k = b.dataset.reset;
      if (k === 'settings') { Object.assign(LD.settings, LD.DEFAULTS); LD.save(); LD.emit('settings', '*'); }
      else if (k === 'dock') { LD.state.dock = JSON.parse(JSON.stringify(LD.DEFAULT_DOCK)); LD.save(); LD.dock.render({ flip: true }); render(); }
      else LD.resetAll();
    }));
  }

  /* Keep controls in sync when settings change elsewhere (dock menu, Control Center). */
  function sync(key) {
    if (!host || !host.isConnected) return;
    if (key === '*' || key === 'wallpaper' || key === 'theme') { render(); return; }
    host.querySelectorAll('.set-row').forEach(row => {
      const r = rowDef(row.dataset.key);
      if (!r) return;
      if (r.dep) row.classList.toggle('off', !LD.settings[r.dep]);
      if (r.key !== key) return;
      const v = LD.settings[key];
      if (r.type === 'switch') row.querySelector('.switch').setAttribute('aria-checked', !!v);
      else if (r.type === 'range') {
        const inp = row.querySelector('input');
        if (document.activeElement !== inp) { inp.value = v; inp.style.setProperty('--p', ((v - r.min) / (r.max - r.min) * 100) + '%'); row.querySelector('.val').textContent = r.fmt(v); }
      } else if (r.type === 'select') row.querySelector('select').value = v;
      else row.querySelectorAll('[data-v]').forEach(b => b.setAttribute('aria-pressed', b.dataset.v === String(v)));
    });
  }
  LD.on('settings', sync);

  LD.settingsUI = {
    mount(el) { host = el; render(); },
    show(sec) { if (sec) current = sec; query = ''; render(); },
  };

  LD.openSettings = function (section) {
    if (section) current = section;
    query = '';
    if (LD.launchpad.isOpen()) LD.launchpad.close();
    const w = LD.wm.mostRecent('laminar');
    if (w) {
      if (w.min) LD.wm.restore(w.id); else LD.wm.focus(w.id);
      render();
      return;
    }
    LD.launch('laminar');
  };
})(window.LD);
