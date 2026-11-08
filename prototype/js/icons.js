/* Laminar Dock prototype — original app icons.
   Each icon is an SVG: a superellipse ("squircle") tile, a gradient, a soft gloss,
   and a simple white glyph. No third-party or Apple artwork is used. */
(function (LD) {
  'use strict';

  /* Superellipse path in a 100x100 box (n≈5 is close to the iOS/macOS tile shape). */
  function squircle(n = 5, steps = 160) {
    const r = 50, pts = [];
    for (let i = 0; i < steps; i++) {
      const t = (i / steps) * Math.PI * 2;
      const c = Math.cos(t), s = Math.sin(t);
      const x = r + r * Math.sign(c) * Math.pow(Math.abs(c), 2 / n);
      const y = r + r * Math.sign(s) * Math.pow(Math.abs(s), 2 / n);
      pts.push(x.toFixed(2) + ' ' + y.toFixed(2));
    }
    return 'M' + pts.join('L') + 'Z';
  }
  const SQ = squircle();
  LD.SQUIRCLE = SQ;

  const W = 'rgba(255,255,255,.96)';
  /* Glyphs drawn on a 24x24 grid (stroke-based unless they set their own fill). */
  const G = {
    folder: `<path d="M3 7.6A2.4 2.4 0 0 1 5.4 5.2h3.7l2.1 2.2h7.4A2.4 2.4 0 0 1 21 9.8v7.6a2.4 2.4 0 0 1-2.4 2.4H5.4A2.4 2.4 0 0 1 3 17.4z" fill="${W}" stroke="none"/><path d="M3.2 10.2h17.6" stroke="rgba(25,95,215,.35)" stroke-width="1.2"/>`,
    compass: `<circle cx="12" cy="12" r="8.6"/><path d="M15.6 8.4l-2.3 4.9-4.9 2.3 2.3-4.9z" fill="${W}" stroke-width="1.2"/><circle cx="12" cy="12" r="1" fill="rgba(40,90,240,1)" stroke="none"/>`,
    mail: `<rect x="3" y="5.6" width="18" height="12.8" rx="2.6" fill="${W}" stroke="none"/><path d="M3.8 7.2L12 12.9l8.2-5.7" stroke="rgba(26,108,240,.75)" stroke-width="1.5"/>`,
    music: `<path d="M9 17.6V6.2l10.5-2v11.2"/><circle cx="6.6" cy="17.6" r="2.5" fill="${W}" stroke="none"/><circle cx="17.1" cy="15.4" r="2.5" fill="${W}" stroke="none"/>`,
    terminal: `<path d="M5.5 8l4.2 4-4.2 4" stroke="#7CF2A5"/><path d="M12 16.6h6.5"/>`,
    code: `<path d="M8.4 7L3.6 12l4.8 5M15.6 7l4.8 5-4.8 5M13.6 4.8l-3.2 14.4"/>`,
    chat: `<path d="M12 3.8c4.8 0 8.7 3.2 8.7 7.2s-3.9 7.2-8.7 7.2c-1.1 0-2.1-.15-3.1-.45L4.6 19.6l1.1-3.6C4.1 14.7 3.3 12.9 3.3 11c0-4 3.9-7.2 8.7-7.2z" fill="${W}" stroke="none"/>`,
    palette: `<path d="M12 3.3a8.7 8.7 0 1 0 0 17.4c1 0 1.6-.7 1.6-1.5 0-.5-.2-.9-.5-1.2-.3-.3-.5-.7-.5-1.2 0-.9.7-1.6 1.6-1.6h1.9a4.8 4.8 0 0 0 4.6-4.8C20.7 6.6 16.8 3.3 12 3.3z" fill="${W}" stroke="none"/><circle cx="7.6" cy="11" r="1.4" fill="#FF5A82" stroke="none"/><circle cx="10" cy="7.3" r="1.4" fill="#FFB020" stroke="none"/><circle cx="14.4" cy="7.2" r="1.4" fill="#2BB673" stroke="none"/><circle cx="17" cy="10.4" r="1.4" fill="#3A7BFF" stroke="none"/>`,
    camera: `<path d="M4.2 8h3l1.5-2.4h6.6L16.8 8h3a1.2 1.2 0 0 1 1.2 1.2v8.6a1.2 1.2 0 0 1-1.2 1.2H4.2A1.2 1.2 0 0 1 3 17.8V9.2A1.2 1.2 0 0 1 4.2 8z" fill="${W}" stroke="none"/><circle cx="12" cy="13.3" r="3.3" fill="none" stroke="#4B5563" stroke-width="1.6"/>`,
    play: `<circle cx="12" cy="12" r="8.6"/><path d="M10 8.4v7.2l6-3.6z" fill="${W}" stroke="none"/>`,
    wave: `<path d="M4 10.5v3M7 8v8M10 5.5v13M13 8.5v7M16 6.5v11M19 10v4"/>`,
    calc: `<rect x="5.5" y="3.2" width="13" height="17.6" rx="2.4" fill="${W}" stroke="none"/><rect x="7.7" y="5.4" width="8.6" height="3.4" rx=".8" fill="#F2741A" stroke="none"/><g fill="#9A5A20" stroke="none"><circle cx="8.9" cy="11.8" r="1"/><circle cx="12" cy="11.8" r="1"/><circle cx="15.1" cy="11.8" r="1"/><circle cx="8.9" cy="14.9" r="1"/><circle cx="12" cy="14.9" r="1"/><circle cx="15.1" cy="14.9" r="1"/><circle cx="8.9" cy="18" r="1"/><circle cx="12" cy="18" r="1"/><circle cx="15.1" cy="18" r="1"/></g>`,
    bag: `<path d="M5.2 8.2h13.6l-1 11.6H6.2z" fill="${W}" stroke="none"/><path d="M9 8.2V6.8a3 3 0 0 1 6 0v1.4"/>`,
    gamepad: `<path d="M7.2 8h9.6a4 4 0 0 1 4 4.4l-.6 3.4a2.3 2.3 0 0 1-4 1L14.8 15H9.2l-1.4 1.8a2.3 2.3 0 0 1-4-1l-.6-3.4A4 4 0 0 1 7.2 8z" fill="${W}" stroke="none"/><path d="M7.4 10.6v3M5.9 12.1h3" stroke="#16A34A" stroke-width="1.4"/><circle cx="16" cy="11.2" r=".9" fill="#16A34A" stroke="none"/><circle cx="17.6" cy="13" r=".9" fill="#16A34A" stroke="none"/>`,
    weather: `<circle cx="9" cy="9" r="3.6" fill="#FFD54A" stroke="none"/><path d="M8.4 19.4h8.8a3.6 3.6 0 0 0 .2-7.2 5 5 0 0 0-9.4 1.6 2.8 2.8 0 0 0 .4 5.6z" fill="${W}" stroke="none"/>`,
    pin: `<path d="M12 21s-6.4-5.6-6.4-11a6.4 6.4 0 0 1 12.8 0c0 5.4-6.4 11-6.4 11z" fill="${W}" stroke="none"/><circle cx="12" cy="10" r="2.4" fill="#0FA678" stroke="none"/>`,
    clock: `<circle cx="12" cy="12" r="8.6" fill="${W}" stroke="none"/><path d="M12 7.2V12l3.2 2" stroke="#17171B" stroke-width="1.6"/><circle cx="12" cy="12" r=".9" fill="#FF8A00" stroke="none"/>`,
    doc: `<path d="M7 3.2h6.6l4.4 4.4v13.2H7z" fill="${W}" stroke="none"/><path d="M9.4 11.4h6.2M9.4 14.2h6.2M9.4 17h4" stroke="#1D4ED8" stroke-width="1.3"/>`,
    grid: `<rect x="4" y="4.5" width="16" height="15" rx="2" fill="${W}" stroke="none"/><path d="M4 9.6h16M4 14.6h16M9.6 4.5v15" stroke="#15803D" stroke-width="1.2"/>`,
    chart: `<rect x="3.6" y="4.6" width="16.8" height="11.6" rx="1.6" fill="${W}" stroke="none"/><path d="M12 16.2v3.2M8.6 19.6h6.8"/><path d="M8 13v-2.4M11 13V8.6M14 13v-3.2M17 13V7.6" stroke="#EA580C" stroke-width="1.5"/>`,
    podcast: `<circle cx="12" cy="10.4" r="2.6" fill="${W}" stroke="none"/><path d="M8.2 14.6a5.4 5.4 0 1 1 7.6 0M5.6 17.2a9 9 0 1 1 12.8 0M12 13.6v6.6"/>`,
    book: `<path d="M4 5.6c2-1 5-1 8 .6 3-1.6 6-1.6 8-.6v13.2c-2-1-5-1-8 .6-3-1.6-6-1.6-8-.6z" fill="${W}" stroke="none"/><path d="M12 6.2v13.2" stroke="#F2620F" stroke-width="1.2"/>`,
    shield: `<path d="M12 3l7.4 3v5.4c0 4.5-3.1 8.2-7.4 9.6-4.3-1.4-7.4-5.1-7.4-9.6V6z" fill="${W}" stroke="none"/><path d="M8.6 12l2.4 2.4 4.4-4.8" stroke="#0886AD" stroke-width="1.7"/>`,
    cloud: `<path d="M7.4 18.6h10a4.2 4.2 0 0 0 .3-8.4 5.8 5.8 0 0 0-11 1.9 3.3 3.3 0 0 0 .7 6.5z" fill="${W}" stroke="none"/>`,
    gear: `<circle cx="12" cy="12" r="7.4" fill="none" stroke="${W}" stroke-width="2.8" stroke-dasharray="2.55 3.26"/><circle cx="12" cy="12" r="5.8" fill="${W}" stroke="none"/><circle cx="12" cy="12" r="2.2" fill="#6E6E78" stroke="none"/>`,
    activity: `<path d="M3 12.4h3.8l2.6-6.6 4.4 12.6 2.8-6h4.4"/>`,
    scissors: `<circle cx="6.6" cy="7" r="2.6"/><circle cx="6.6" cy="17" r="2.6"/><path d="M8.8 8.6L20 16.6M8.8 15.4L20 7.4"/>`,
    user: `<circle cx="12" cy="9" r="3.6" fill="${W}" stroke="none"/><path d="M5 20.2c1-3.6 3.8-5.6 7-5.6s6 2 7 5.6" fill="${W}" stroke="none"/>`,
    monitor: `<rect x="3" y="4.4" width="18" height="12.2" rx="2" fill="${W}" stroke="none"/><path d="M9 20h6M12 16.6V20"/>`,
    sliders: `<path d="M5 7.2h14M5 12h14M5 16.8h14"/><circle cx="9.4" cy="7.2" r="2" fill="${W}" stroke="none"/><circle cx="15" cy="12" r="2" fill="${W}" stroke="none"/><circle cx="8.2" cy="16.8" r="2" fill="${W}" stroke="none"/>`,
    trashGlyph: `<path d="M6.4 7.4h11.2l-1 12.2a1.4 1.4 0 0 1-1.4 1.3H8.8a1.4 1.4 0 0 1-1.4-1.3z" fill="${W}" stroke="none"/><path d="M4.8 6.4h14.4M10 4.2h4"/>`,
  };

  let gid = 0;

  function frame(defs, body, opts = {}) {
    const id = 'i' + (++gid);
    const gloss = opts.noGloss ? '' :
      `<path d="${SQ}" fill="url(#${id}gl)"/>`;
    return `<svg viewBox="0 0 100 100" aria-hidden="true">
      <defs>
        ${defs.replace(/\$ID/g, id)}
        <linearGradient id="${id}gl" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#fff" stop-opacity="${opts.gloss ?? 0.26}"/>
          <stop offset=".5" stop-color="#fff" stop-opacity="0"/>
        </linearGradient>
        <clipPath id="${id}cp"><path d="${SQ}"/></clipPath>
      </defs>
      <g clip-path="url(#${id}cp)">${body.replace(/\$ID/g, id)}${gloss}</g>
      <path d="${SQ}" fill="none" stroke="rgba(255,255,255,${opts.rim ?? 0.24})" stroke-width="1.4"/>
    </svg>`;
  }

  function glyphIcon(c, glyph) {
    const defs = `<linearGradient id="$IDbg" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="${c[0]}"/><stop offset="1" stop-color="${c[1]}"/></linearGradient>`;
    const body = `<rect width="100" height="100" fill="url(#$IDbg)"/>
      <g transform="translate(20 20) scale(2.5)" fill="none" stroke="${W}" stroke-width="1.7"
         stroke-linecap="round" stroke-linejoin="round"
         style="filter:drop-shadow(0 .4px .5px rgba(0,0,0,.18))">${G[glyph] || G.gear}</g>`;
    return frame(defs, body);
  }

  const SPECIAL = {
    calendar() {
      const d = new Date();
      const day = d.toLocaleDateString(undefined, { weekday: 'short' }).toUpperCase();
      return frame('', `<rect width="100" height="100" fill="#FDFDFD"/>
        <text x="50" y="30" text-anchor="middle" font-family="Segoe UI Variable Display, Segoe UI, sans-serif"
          font-size="15" font-weight="600" fill="#FF3B30" letter-spacing=".5">${day}</text>
        <text x="50" y="79" text-anchor="middle" font-family="Segoe UI Variable Display, Segoe UI, sans-serif"
          font-size="50" font-weight="300" fill="#1D1D1F">${d.getDate()}</text>`, { gloss: 0.1, rim: 0.5 });
    },
    photos() {
      const cols = ['#FFB020', '#FF7A1A', '#FF3B6B', '#C04BF2', '#5B5BFF', '#18A0FB', '#22C38E', '#9BD42A'];
      const petals = cols.map((c, i) =>
        `<ellipse cx="50" cy="31" rx="10" ry="17" fill="${c}" opacity=".82" transform="rotate(${i * 45} 50 50)" style="mix-blend-mode:multiply"/>`).join('');
      return frame('', `<rect width="100" height="100" fill="#FDFDFD"/>${petals}`, { gloss: 0.1, rim: 0.5 });
    },
    notes() {
      return frame('', `<rect width="100" height="100" fill="#FFFDF5"/>
        <rect width="100" height="30" fill="#FFD54A"/>
        <path d="M0 30h100" stroke="#E2B623" stroke-width="1.5"/>
        <g stroke="#D6D3CC" stroke-width="2"><path d="M18 46h64M18 60h64M18 74h48"/></g>`, { gloss: 0.12, rim: 0.5 });
    },
    launch() {
      /* The Laminar mark: three smooth parallel flow lines in a glassy tile. */
      const defs = `<linearGradient id="$IDbg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="#7FA8FF"/><stop offset=".5" stop-color="#6A4DF5"/><stop offset="1" stop-color="#E05ACF"/></linearGradient>
        <radialGradient id="$IDsh" cx=".3" cy=".2" r=".8"><stop offset="0" stop-color="#fff" stop-opacity=".45"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient>`;
      const body = `<rect width="100" height="100" fill="url(#$IDbg)"/>
        <rect width="100" height="100" fill="url(#$IDsh)"/>
        <g fill="none" stroke="#fff" stroke-linecap="round" stroke-width="5.2">
          <path d="M20 40c12-10 22 8 34-2s20-8 26-4" opacity=".98"/>
          <path d="M20 54c12-10 22 8 34-2s20-8 26-4" opacity=".78"/>
          <path d="M20 68c12-10 22 8 34-2s20-8 26-4" opacity=".55"/>
        </g>`;
      return frame(defs, body, { gloss: 0.22 });
    },
    trash() {
      /* Translucent bin, so it reads as part of the glass rather than an app. */
      return `<svg viewBox="0 0 100 100" aria-hidden="true">
        <defs><linearGradient id="tb${++gid}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#fff" stop-opacity=".55"/><stop offset="1" stop-color="#fff" stop-opacity=".22"/></linearGradient></defs>
        <path d="M24 30h52l-4.5 56a6 6 0 0 1-6 5.5h-31a6 6 0 0 1-6-5.5z" fill="url(#tb${gid})" stroke="rgba(255,255,255,.85)" stroke-width="2"/>
        <rect x="19" y="21" width="62" height="10" rx="5" fill="rgba(255,255,255,.7)" stroke="rgba(255,255,255,.9)" stroke-width="1.5"/>
        <g stroke="rgba(255,255,255,.65)" stroke-width="2.4" stroke-linecap="round"><path d="M38 42l1.5 40M50 42v40M62 42l-1.5 40"/></g>
      </svg>`;
    },
  };

  /** Returns SVG markup for an app id, or for 'launch' / 'trash'. */
  LD.icon = function (id) {
    if (SPECIAL[id]) return SPECIAL[id]();
    const app = LD.APPS[id];
    if (!app) return glyphIcon(['#999', '#555'], 'gear');
    if (app.special && SPECIAL[app.special]) return SPECIAL[app.special]();
    return glyphIcon(app.c, app.glyph);
  };

  /** Small monochrome UI glyphs (Lucide-style, drawn here) used across the chrome. */
  const UI = {
    search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.6-3.6"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-1-1.5 1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.5-1 1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H9a1.6 1.6 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V9a1.6 1.6 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z"/>',
    power: '<path d="M12 3v8"/><path d="M6.4 6.6a8 8 0 1 0 11.2 0"/>',
    wifi: '<path d="M2.5 8.8a14 14 0 0 1 19 0"/><path d="M5.6 12.3a9.4 9.4 0 0 1 12.8 0"/><path d="M8.8 15.7a4.8 4.8 0 0 1 6.4 0"/><circle cx="12" cy="19" r="1" fill="currentColor"/>',
    bluetooth: '<path d="M7 7l10 10-5 4V3l5 4L7 17"/>',
    share: '<circle cx="12" cy="12" r="2.4"/><path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M5 5a10 10 0 0 0 0 14M19 5a10 10 0 0 1 0 14"/>',
    moon: '<path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z"/>',
    contrast: '<circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor"/>',
    sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
    nightlight: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9z"/><path d="M17 3v3M15.5 4.5h3"/>',
    leaf: '<path d="M11 20A7 7 0 0 1 9.8 6.1C15.5 5 17 4.5 19 2c1 2 2 4.2 2 8 0 5.5-4.8 10-10 10z"/><path d="M2 21c0-3 1.9-5.4 5.2-6"/>',
    scissors: '<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M20 4L8.1 15.9M14.5 14.5L20 20M8.1 8.1L12 12"/>',
    cast: '<path d="M2 8V6a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-6"/><path d="M2 12a9 9 0 0 1 8 8M2 16a5 5 0 0 1 4 4"/><circle cx="2.5" cy="20" r=".6" fill="currentColor"/>',
    mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>',
    speaker: '<path d="M11 5L6 9H3v6h3l5 4z" fill="currentColor"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M18.4 5.6a9 9 0 0 1 0 12.8"/>',
    speakerLow: '<path d="M11 5L6 9H3v6h3l5 4z" fill="currentColor"/>',
    chevron: '<path d="M9 6l6 6-6 6"/>',
    chevronLeft: '<path d="M15 6l-6 6 6 6"/>',
    chevronDown: '<path d="M6 9l6 6 6-6"/>',
    play: '<path d="M7 4.5v15l12-7.5z" fill="currentColor" stroke="none"/>',
    pause: '<rect x="6" y="4.5" width="4" height="15" rx="1" fill="currentColor" stroke="none"/><rect x="14" y="4.5" width="4" height="15" rx="1" fill="currentColor" stroke="none"/>',
    next: '<path d="M4 5.5v13l8-6.5zM12 5.5v13l8-6.5z" fill="currentColor" stroke="none"/>',
    prev: '<path d="M20 5.5v13l-8-6.5zM12 5.5v13l-8-6.5z" fill="currentColor" stroke="none"/>',
    lock: '<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
    check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    x: '<path d="M6 6l12 12M18 6L6 18"/>',
    plane: '<path d="M17.8 19.2L16 11l3.5-3.5C21 6 21.5 4 21 3c-1-.5-3 0-4.5 1.5L13 8 4.8 6.2c-.5-.1-.9.1-1.1.5l-.3.5c-.2.5-.1 1 .3 1.3L9 12l-2 3H4l-1 1 3 2 2 3 1-1v-3l3-2 3.5 5.3c.3.4.8.5 1.3.3l.5-.2c.4-.3.6-.7.5-1.2z"/>',
    refresh: '<path d="M21 12a9 9 0 1 1-2.6-6.4L21 8"/><path d="M21 3v5h-5"/>',
    download: '<path d="M12 3v12M7 10l5 5 5-5M5 21h14"/>',
    cpu: '<rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>',
    sparkle: '<path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
    image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="M21 16l-5-5L5 20"/>',
    globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
    sliders: '<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>',
    user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
    headphones: '<path d="M3 18v-6a9 9 0 0 1 18 0v6"/><rect x="3" y="14" width="4" height="7" rx="1.5"/><rect x="17" y="14" width="4" height="7" rx="1.5"/>',
    keyboard: '<rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/>',
    mouse: '<rect x="6" y="3" width="12" height="18" rx="6"/><path d="M12 7v4"/>',
    phone: '<rect x="7" y="2" width="10" height="20" rx="2.5"/><path d="M11 18h2"/>',
    tv: '<rect x="2" y="5" width="20" height="13" rx="2"/><path d="M8 21h8"/>',
    shield: '<path d="M12 3l7.5 3v5.5c0 4.5-3.2 8.2-7.5 9.5-4.3-1.3-7.5-5-7.5-9.5V6z"/>',
    cloud: '<path d="M7 18h10a4 4 0 0 0 .5-8 6 6 0 0 0-11.4 2A3 3 0 0 0 7 18z"/>',
    clipboard: '<rect x="6" y="4" width="12" height="17" rx="2"/><path d="M9 4V3h6v1"/>',
    gpu: '<rect x="2" y="7" width="20" height="10" rx="2"/><circle cx="9" cy="12" r="2.5"/><path d="M15 10h4M15 14h4"/>',
    gamepad: '<path d="M6 11h4M8 9v4"/><circle cx="15.5" cy="10.5" r=".8" fill="currentColor"/><circle cx="17.5" cy="12.5" r=".8" fill="currentColor"/><path d="M7 6h10a5 5 0 0 1 5 5.5l-.6 3.5a2.5 2.5 0 0 1-4.4 1.1L15.5 14h-7L7 16.1a2.5 2.5 0 0 1-4.4-1.1L2 11.5A5 5 0 0 1 7 6z"/>',
    chat: '<path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12z"/>',
    battery: '<rect x="2" y="7" width="17" height="10" rx="2.5"/><path d="M22 11v2"/><rect x="4.5" y="9.5" width="9" height="5" rx="1" fill="currentColor" stroke="none"/>',
    edit: '<path d="M4 20h4L19 9l-4-4L4 16z"/>',
    grid: '<rect x="4" y="4" width="6" height="6" rx="1.5"/><rect x="14" y="4" width="6" height="6" rx="1.5"/><rect x="4" y="14" width="6" height="6" rx="1.5"/><rect x="14" y="14" width="6" height="6" rx="1.5"/>',
    signOut: '<path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 17l5-5-5-5M15 12H3"/>',
    restart: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/>',
    sleep: '<path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z"/>',
    pin: '<path d="M12 17v5M9 3h6l-1 6 3 3H7l3-3z"/>',
    activity: '<path d="M3 12h4l3-8 4 16 3-8h4"/>',
    memory: '<rect x="3" y="7" width="18" height="10" rx="1.5"/><path d="M7 7v10M11 7v10M15 7v10M3 20h18"/>',
    disk: '<rect x="3" y="5" width="18" height="14" rx="2.5"/><circle cx="16.5" cy="15" r="1" fill="currentColor"/><path d="M6 15h6"/>',
    net: '<path d="M7 4v16M3 16l4 4 4-4M17 20V4M13 8l4-4 4 4"/>',
  };
  LD.ui = function (name, size = 16, sw = 1.8) {
    return `<svg class="ui-ico" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${UI[name] || ''}</svg>`;
  };
})(window.LD);
