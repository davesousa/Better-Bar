/* Laminar Dock prototype — shared state, settings, app catalog, event bus.
   Everything lives on window.LD so the plain <script> files can share it
   without a build step. */
window.LD = window.LD || {};
(function (LD) {
  'use strict';

  const STORE_KEY = 'laminar-dock-proto-v1';

  /* ---------- Settings (the same shape the native app's config.json will use) ---------- */
  LD.DEFAULTS = {
    // Appearance
    theme: 'auto',            // auto (match wallpaper) | light | dark
    accent: '#0A84FF',
    wallpaper: 'dusk',        // dusk | dawn | graphite

    // Glass material
    glassTint: 0.24,          // 0 – 0.8 opacity of the tint layer
    glassBlur: 12,            // px (panels use ~1.8x this)
    glassSaturation: 180,     // %
    refraction: true,         // dock bends the wallpaper behind it
    refractionStrength: 45,   // 0 – 100
    edgeHighlight: 0.55,      // 0 – 1 specular rim
    shadowStrength: 0.35,     // 0 – 1

    // Dock
    iconSize: 54,             // px
    magnify: true,
    magnifySize: 88,          // px
    magnifyRange: 2.6,        // icons either side
    iconSpacing: 8,           // px
    dockPadding: 8,           // px
    dockRadius: 24,           // px
    indicatorStyle: 'dot',    // dot | bar
    bounceOnLaunch: true,
    showClock: true,
    minimizeEffect: 'genie',  // genie | scale

    // Behaviour
    autoHide: false,
    revealZone: 6,            // px band at the bottom edge
    revealDelay: 120,         // ms
    hideDelay: 450,           // ms
    displays: 'all',          // all | main | pointer
    perDisplayApps: 'all',    // all | own
    winKeyLaunchpad: true,
    clickToMinimize: true,

    // Launchpad
    lpIconSize: 62,
    lpBlur: 36,
    showWidgets: true,
    showControl: true,

    // Motion
    animSpeed: 1,             // duration multiplier (lower = faster)
    bounciness: 0.22,         // 0 – 0.6
    reduceMotion: false,
  };

  /* ---------- App catalog (original icons, drawn in icons.js) ---------- */
  LD.APPS = {
    files:    { name: 'Files',           glyph: 'folder',   c: ['#63C1FF', '#1F6FE5'] },
    browser:  { name: 'Browser',         glyph: 'compass',  c: ['#5ED8FF', '#2B54F0'] },
    mail:     { name: 'Mail',            glyph: 'mail',     c: ['#6BBEFF', '#1A6CF0'] },
    calendar: { name: 'Calendar',        special: 'calendar' },
    photos:   { name: 'Photos',          special: 'photos' },
    music:    { name: 'Music',           glyph: 'music',    c: ['#FF7896', '#EE2350'] },
    notes:    { name: 'Notes',           special: 'notes' },
    terminal: { name: 'Terminal',        glyph: 'terminal', c: ['#40404A', '#111115'] },
    code:     { name: 'Code',            glyph: 'code',     c: ['#45AEFF', '#1449C4'] },
    chat:     { name: 'Chat',            glyph: 'chat',     c: ['#74EBA6', '#16B157'] },
    paint:    { name: 'Paint',           glyph: 'palette',  c: ['#FFC078', '#FF5A82'] },
    camera:   { name: 'Camera',          glyph: 'camera',   c: ['#A5ADB8', '#4B5563'] },
    video:    { name: 'Video',           glyph: 'play',     c: ['#9686FF', '#5131E0'] },
    recorder: { name: 'Recorder',        glyph: 'wave',     c: ['#FF8577', '#E3303A'] },
    calc:     { name: 'Calculator',      glyph: 'calc',     c: ['#FFBE55', '#F2741A'] },
    store:    { name: 'Store',           glyph: 'bag',      c: ['#62CCFF', '#3A63FF'] },
    games:    { name: 'Games',           glyph: 'gamepad',  c: ['#B2EC45', '#16A34A'] },
    weather:  { name: 'Weather',         glyph: 'weather',  c: ['#86D8FF', '#2563EB'] },
    maps:     { name: 'Maps',            glyph: 'pin',      c: ['#8EF0B0', '#0FA678'] },
    clock:    { name: 'Clock',           glyph: 'clock',    c: ['#44444C', '#17171B'] },
    docs:     { name: 'Docs',            glyph: 'doc',      c: ['#6AABFF', '#1D4ED8'] },
    sheets:   { name: 'Sheets',          glyph: 'grid',     c: ['#55E08A', '#15803D'] },
    slides:   { name: 'Slides',          glyph: 'chart',    c: ['#FFC285', '#EA580C'] },
    podcasts: { name: 'Podcasts',        glyph: 'podcast',  c: ['#DDBBFF', '#9333EA'] },
    books:    { name: 'Books',           glyph: 'book',     c: ['#FFC382', '#F2620F'] },
    vpn:      { name: 'Secure VPN',      glyph: 'shield',   c: ['#70EAFA', '#0886AD'] },
    cloud:    { name: 'Cloud Drive',     glyph: 'cloud',    c: ['#C2EAFF', '#3BA9F2'] },
    system:   { name: 'System Settings', glyph: 'gear',     c: ['#AFAFB8', '#55555E'] },
    taskmgr:  { name: 'Task Manager',    glyph: 'activity', c: ['#3EDBA4', '#0F766E'] },
    snip:     { name: 'Snipping Tool',   glyph: 'scissors', c: ['#FBB2DA', '#D61F73'] },
    contacts: { name: 'Contacts',        glyph: 'user',     c: ['#DDD9D6', '#7A736E'] },
    remote:   { name: 'Remote Desktop',  glyph: 'monitor',  c: ['#9DCBFF', '#3B7BF6'] },
    laminar:  { name: 'Laminar Settings', glyph: 'sliders', c: ['#8C9BFF', '#4A2FE0'] },
    trash:    { name: 'Recycle Bin',     glyph: 'trashGlyph', c: ['#9AA0AA', '#5A616C'], hidden: true },
  };

  /* Default dock: launch button and trash are fixed; this is the user-arranged middle. */
  LD.DEFAULT_DOCK = [
    { type: 'app', id: 'files' },
    { type: 'app', id: 'browser' },
    { type: 'app', id: 'mail' },
    { type: 'app', id: 'calendar' },
    { type: 'app', id: 'photos' },
    { type: 'app', id: 'music' },
    { type: 'sep', id: 's1' },
    { type: 'app', id: 'notes' },
    { type: 'app', id: 'terminal' },
    { type: 'app', id: 'code' },
    { type: 'app', id: 'chat' },
    { type: 'stack', id: 'k1', name: 'Creative', apps: ['paint', 'camera', 'video', 'recorder'], view: 'grid' },
  ];

  LD.DEFAULT_LP_PINNED = ['files', 'browser', 'mail', 'calendar', 'photos', 'music',
    'notes', 'code', 'terminal', 'chat', 'store', 'system', 'weather', 'maps', 'calc', 'laminar'];

  /* ---------- Persistence (prototype only: browser localStorage) ---------- */
  function load() {
    let saved = null;
    try { saved = JSON.parse(localStorage.getItem(STORE_KEY) || 'null'); } catch (e) { saved = null; }
    LD.settings = Object.assign({}, LD.DEFAULTS, saved && saved.settings);
    LD.state = {
      dock: (saved && Array.isArray(saved.dock)) ? saved.dock : JSON.parse(JSON.stringify(LD.DEFAULT_DOCK)),
      lpPinned: (saved && Array.isArray(saved.lpPinned)) ? saved.lpPinned : LD.DEFAULT_LP_PINNED.slice(),
    };
  }
  let saveTimer = 0;
  LD.save = function () {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      try {
        localStorage.setItem(STORE_KEY, JSON.stringify({
          settings: LD.settings, dock: LD.state.dock.filter(d => d.type !== 'ph'), lpPinned: LD.state.lpPinned,
        }));
      } catch (e) { /* storage unavailable: prototype keeps working in memory */ }
    }, 200);
  };
  LD.resetAll = function () {
    try { localStorage.removeItem(STORE_KEY); } catch (e) { /* ignore */ }
    location.reload();
  };
  load();

  /* ---------- Tiny event bus ---------- */
  const handlers = {};
  LD.on = (evt, fn) => { (handlers[evt] = handlers[evt] || []).push(fn); };
  LD.emit = (evt, data) => { (handlers[evt] || []).forEach(fn => fn(data)); };

  LD.set = function (key, value) {
    LD.settings[key] = value;
    LD.save();
    LD.emit('settings', key);
  };

  /* ---------- Helpers ---------- */
  let n = 0;
  LD.uid = (p = 'id') => p + Date.now().toString(36) + (n++).toString(36);
  LD.clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  LD.esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  LD.ms = base => LD.settings.reduceMotion ? 1 : Math.round(base * LD.settings.animSpeed);

  /* A damped-spring curve expressed as a CSS linear() easing, so CSS transitions
     move like physical springs (the native app will use Composition's NaturalMotion). */
  LD.springCurve = function (bounce) {
    const zeta = Math.max(0.36, 1 - bounce);
    const w = 6.2 / zeta;
    const N = 40, pts = [];
    for (let i = 0; i <= N; i++) {
      const t = i / N;
      let x;
      if (zeta < 1) {
        const wd = w * Math.sqrt(1 - zeta * zeta);
        x = 1 - Math.exp(-zeta * w * t) * (Math.cos(wd * t) + (zeta * w / wd) * Math.sin(wd * t));
      } else {
        x = 1 - Math.exp(-w * t) * (1 + w * t);
      }
      pts.push(+x.toFixed(4));
    }
    pts[N] = 1;
    return 'linear(' + pts.join(', ') + ')';
  };

  /* ---------- Shared media session (Music window + Control Center) ---------- */
  LD.media = {
    playing: true, idx: 0, pos: 63,
    tracks: [
      { title: 'Midnight Current', artist: 'Lumen Tides', album: 'Laminar', dur: 214, c: ['#FF7EB3', '#7A5CFF'] },
      { title: 'Glass Horizon', artist: 'Northbound', album: 'Refraction', dur: 188, c: ['#5ED8FF', '#2B54F0'] },
      { title: 'Slow Tides', artist: 'Aria Vale', album: 'Undertow', dur: 241, c: ['#FFC078', '#FF5A82'] },
    ],
    get track() { return this.tracks[this.idx]; },
    toggle() { this.playing = !this.playing; LD.emit('media'); },
    next(d = 1) { this.idx = (this.idx + d + this.tracks.length) % this.tracks.length; this.pos = 0; LD.emit('media'); },
  };
  setInterval(() => {
    if (!LD.media.playing) return;
    LD.media.pos += 1;
    if (LD.media.pos >= LD.media.track.dur) LD.media.next();
    else LD.emit('media-tick');
  }, 1000);
})(window.LD);
