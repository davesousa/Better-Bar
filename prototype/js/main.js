/* Laminar Dock prototype — boot: wallpaper, live settings, theme, keyboard, toasts. */
(function (LD) {
  'use strict';
  const html = document.documentElement;

  /* ---------- Wallpapers (original "laminar flow" ribbons, generated as SVG) ---------- */
  const PALETTES = {
    dusk: { bg: ['#090D2C', '#22185C', '#5A2775'], ga: '#FF5FA2', gb: '#4D7CFF', r1: ['#5B4CFF', '#FF6FB1'], r2: ['#4F6BFF', '#D77BFF'], r3: ['#FF9A8B', '#FF5FA2'], edge: .75 },
    dawn: { bg: ['#C9DAFF', '#E7DDFF', '#FFE2EC'], ga: '#FFC2D9', gb: '#9FC0FF', r1: ['#B3C2FF', '#FFD1E3'], r2: ['#97AEFF', '#EFC4FF'], r3: ['#FFD5BF', '#FFB0CC'], edge: .55 },
    graphite: { bg: ['#0C0E12', '#191D25', '#272D38'], ga: '#3A4A66', gb: '#56688C', r1: ['#2C3647', '#485670'], r2: ['#3A4864', '#7889AE'], r3: ['#55637F', '#2A3242'], edge: .45 },
  };
  let wpn = 0;
  LD.wallpaperSVG = function (kind) {
    const p = PALETTES[kind] || PALETTES.dusk;
    const id = 'wp' + (++wpn);
    const main = 'M-200 700 C 220 470, 640 860, 1040 610 S 1600 250, 1900 300';
    return `<svg viewBox="0 0 1600 1000" preserveAspectRatio="xMidYMid slice" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <linearGradient id="${id}bg" x1="0" y1="0" x2=".35" y2="1"><stop offset="0" stop-color="${p.bg[0]}"/><stop offset=".55" stop-color="${p.bg[1]}"/><stop offset="1" stop-color="${p.bg[2]}"/></linearGradient>
        <radialGradient id="${id}ga" cx=".18" cy=".9" r=".7"><stop offset="0" stop-color="${p.ga}" stop-opacity=".85"/><stop offset="1" stop-color="${p.ga}" stop-opacity="0"/></radialGradient>
        <radialGradient id="${id}gb" cx=".88" cy=".12" r=".6"><stop offset="0" stop-color="${p.gb}" stop-opacity=".75"/><stop offset="1" stop-color="${p.gb}" stop-opacity="0"/></radialGradient>
        <linearGradient id="${id}r1" x1="0" y1="1" x2="1" y2="0"><stop offset="0" stop-color="${p.r1[0]}"/><stop offset="1" stop-color="${p.r1[1]}"/></linearGradient>
        <linearGradient id="${id}r2" x1="0" y1=".7" x2="1" y2=".2"><stop offset="0" stop-color="${p.r2[0]}"/><stop offset="1" stop-color="${p.r2[1]}"/></linearGradient>
        <linearGradient id="${id}r3" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${p.r3[0]}"/><stop offset="1" stop-color="${p.r3[1]}"/></linearGradient>
        <linearGradient id="${id}e" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#FF6EC7"/><stop offset=".3" stop-color="#FFD36E"/><stop offset=".55" stop-color="#7EFFC4"/><stop offset=".8" stop-color="#6EC8FF"/><stop offset="1" stop-color="#B06EFF"/></linearGradient>
        <filter id="${id}s" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="46"/></filter>
        <filter id="${id}s2"><feGaussianBlur stdDeviation="5"/></filter>
      </defs>
      <rect width="1600" height="1000" fill="url(#${id}bg)"/>
      <rect width="1600" height="1000" fill="url(#${id}ga)"/>
      <rect width="1600" height="1000" fill="url(#${id}gb)"/>
      <path d="M-200 820 C 250 560, 620 980, 1050 700 S 1650 360, 1900 420 L 1900 1100 L -200 1100 Z" fill="url(#${id}r1)" opacity=".6" filter="url(#${id}s)"/>
      <path d="M-100 300 C 300 420, 700 140, 1100 360 S 1500 760, 1800 640 L1800 770 C 1480 910, 1250 570, 1000 490 S 300 480, -100 430 Z" fill="url(#${id}r3)" opacity=".5"/>
      <path d="${main} L1900 540 C 1560 490, 1240 780, 960 880 S 260 840, -200 1000 Z" fill="url(#${id}r2)" opacity=".92"/>
      <path d="M-200 760 C 260 560, 640 920, 1040 690 S 1600 360, 1900 420" stroke="#fff" stroke-opacity=".18" stroke-width="2" fill="none"/>
      <path d="${main}" stroke="url(#${id}e)" stroke-width="6" fill="none" opacity="${p.edge * .7}" filter="url(#${id}s2)"/>
      <path d="${main}" stroke="url(#${id}e)" stroke-width="1.6" fill="none" opacity="${p.edge}"/>
      <path d="M-100 300 C 300 420, 700 140, 1100 360 S 1500 760, 1800 640" stroke="#fff" stroke-opacity=".28" stroke-width="1.2" fill="none"/>
    </svg>`;
  };

  let currentWall = null;
  function renderWallpaper() {
    const kind = LD.settings.wallpaper;
    if (kind === currentWall) return;
    currentWall = kind;
    const host = document.getElementById('wallpaper');
    const layer = document.createElement('div');
    layer.className = 'wp';
    layer.style.opacity = host.children.length ? '0' : '1';
    layer.innerHTML = LD.wallpaperSVG(kind);
    host.appendChild(layer);
    requestAnimationFrame(() => { layer.style.opacity = '1'; });
    const old = [...host.children].slice(0, -1);
    setTimeout(() => old.forEach(o => o.remove()), 900);
  }

  /* ---------- Apply settings to CSS tokens and components ---------- */
  function apply(key) {
    const s = LD.settings;
    const st = html.style;
    st.setProperty('--icon', s.iconSize + 'px');
    st.setProperty('--mag', Math.max(s.magnifySize, s.iconSize) + 'px');
    st.setProperty('--gap', s.iconSpacing + 'px');
    st.setProperty('--pad', s.dockPadding + 'px');
    st.setProperty('--dock-radius', Math.min(s.dockRadius, (s.iconSize + s.dockPadding * 2) / 2) + 'px');
    st.setProperty('--glass-tint', s.glassTint);
    st.setProperty('--glass-blur', s.glassBlur + 'px');
    st.setProperty('--glass-sat', s.glassSaturation + '%');
    st.setProperty('--edge', s.edgeHighlight);
    st.setProperty('--shadow', s.shadowStrength);
    st.setProperty('--accent', s.accent);
    st.setProperty('--lp-icon', s.lpIconSize + 'px');
    st.setProperty('--lp-blur', s.lpBlur + 'px');
    st.setProperty('--dur', LD.ms(520) + 'ms');
    LD.springEasing = s.reduceMotion ? 'linear' : LD.springCurve(s.bounciness);
    st.setProperty('--spring', LD.springEasing);

    const theme = s.theme === 'auto' ? (s.wallpaper === 'dawn' ? 'light' : 'dark') : s.theme;
    html.classList.toggle('theme-dark', theme === 'dark');
    html.classList.toggle('theme-light', theme === 'light');

    renderWallpaper();
    if (!LD.dock) return;
    if (['iconSize', 'dockPadding', 'dockRadius', '*'].includes(key)) LD.dock.resetMap();
    LD.dock.render();
    LD.dock.kick();
    if (['autoHide', 'iconSize', 'dockPadding', '*'].includes(key)) LD.dock.applyAutoHide();
    if (['showWidgets', 'showControl', '*'].includes(key)) LD.launchpad.applyLayout();
    if (['theme', 'wallpaper', '*'].includes(key)) LD.launchpad.refresh();
  }
  LD.on('settings', apply);

  /* ---------- Toasts ---------- */
  LD.toast = function (appId, title, body) {
    const box = document.getElementById('toasts');
    const t = document.createElement('div');
    t.className = 'toast glass strong';
    t.innerHTML = `<div class="t-ico">${LD.icon(appId)}</div><div><b>${LD.esc(title)}</b><span>${LD.esc(body)}</span></div>`;
    box.appendChild(t);
    const kill = () => { t.classList.add('out'); setTimeout(() => t.remove(), 300); };
    t.addEventListener('click', kill);
    setTimeout(kill, 3600);
  };

  /* ---------- Keyboard ---------- */
  let metaAlone = false;
  document.addEventListener('keydown', e => {
    const typing = document.activeElement && document.activeElement.matches('input, textarea, select');
    if ((e.ctrlKey && e.code === 'Space') || (e.key === '`' && !typing)) {
      e.preventDefault();
      LD.launchpad.toggle();
      return;
    }
    metaAlone = e.key === 'Meta';
  });
  document.addEventListener('keyup', e => {
    // Best effort: Windows usually opens its own Start menu first in a browser.
    if (e.key === 'Meta' && metaAlone && LD.settings.winKeyLaunchpad) LD.launchpad.toggle();
    metaAlone = false;
  });

  /* ---------- Prototype hint ---------- */
  const hint = document.getElementById('proto-hint');
  let hintGone = false;
  try { hintGone = sessionStorage.getItem('ld-hint') === '1'; } catch (e) { /* ignore */ }
  if (hintGone) hint.classList.add('gone');
  document.getElementById('proto-hint-close').addEventListener('click', () => {
    hint.classList.add('gone');
    try { sessionStorage.setItem('ld-hint', '1'); } catch (e) { /* ignore */ }
  });

  /* ---------- Boot ---------- */
  apply('*');
  const W = innerWidth, wa = LD.wm.workArea();
  LD.wm.open('files', { x: Math.round(W * 0.05), y: Math.round(wa.h * 0.1), silent: true });
  LD.wm.open('browser', { page: 0, x: Math.round(W * 0.22), y: Math.round(wa.h * 0.16), silent: true });
  LD.wm.open('browser', { page: 1, minimized: true });
  LD.wm.open('music', { minimized: true });
  LD.wm.open('chat', { minimized: true });
  LD.wm.open('notes', { x: Math.round(W - Math.min(740, W - 60) - W * 0.05), y: Math.round(wa.h * 0.07), silent: true });
  LD.dock.render({ instant: true });
  addEventListener('resize', () => { LD.dock.kick(); });
})(window.LD);
