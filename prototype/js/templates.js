/* Laminar Dock prototype — content for the simulated app windows.
   These stand in for real Windows apps so the dock has something to manage. */
(function (LD) {
  'use strict';
  const ui = (n, s) => LD.ui(n, s);
  const fmt = s => Math.floor(s / 60) + ':' + String(Math.floor(s % 60)).padStart(2, '0');

  function artSVG(c) {
    return `<svg class="art-lines" viewBox="0 0 100 100" preserveAspectRatio="none">
      <defs><linearGradient id="ag${c[0].slice(1)}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${c[0]}"/><stop offset="1" stop-color="${c[1]}"/></linearGradient></defs>
      <rect width="100" height="100" fill="url(#ag${c[0].slice(1)})"/>
      <g fill="none" stroke="#fff" stroke-linecap="round" stroke-width="3">
        <path d="M10 45c14-12 26 8 40-2s24-8 40-4" opacity=".9"/><path d="M10 58c14-12 26 8 40-2s24-8 40-4" opacity=".6"/><path d="M10 71c14-12 26 8 40-2s24-8 40-4" opacity=".35"/>
      </g></svg>`;
  }
  LD.artSVG = artSVG;

  LD.templates = {
    notes: {
      html: () => `<div class="notes">
        <div class="notes-side">
          <div class="n on"><b>Try the prototype</b><span>Today · Laminar Dock</span></div>
          <div class="n"><b>Native build plan</b><span>Rust + Composition</span></div>
          <div class="n"><b>Later</b><span>Brightness, widgets…</span></div>
        </div>
        <div class="notes-doc">
          <h2>Things to try</h2>
          <div class="sub">A clickable look-and-feel prototype of Laminar Dock.</div>
          <ul>
            <li><i></i><span><b>Hover the dock.</b> Icons magnify and the icon under the pointer stays put.</span></li>
            <li><i></i><span><b>Click apps.</b> Closed apps launch with a bounce. Running apps focus. Clicking the focused app minimizes it, like Windows.</span></li>
            <li><i></i><span><b>Hover a running app.</b> Browser has two windows. Hover a preview to peek at that window.</span></li>
            <li><i></i><span><b>Drag icons</b> to reorder them. Drag one up off the dock to remove it.</span></li>
            <li><i></i><span><b>Hold a dragged app over another app</b> to make a stack. Click the Creative stack to open it.</span></li>
            <li><i></i><span><b>Right-click everything:</b> icons, separators, stacks, the Laminar button and empty dock space.</span></li>
            <li><i></i><span><b>Open Launchpad</b> with the Laminar button (far left) or <b>Ctrl+Space</b>. Type to search, then drag apps from it into the dock.</span></li>
            <li><i></i><span><b>Open Laminar Settings</b> (the gear in Launchpad) and change the glass, sizes and motion live.</span></li>
            <li><i></i><span><b>Turn on Auto-hide</b>, then push the pointer against the bottom edge of the screen.</span></li>
          </ul>
        </div>
      </div>`,
    },

    files: {
      html: () => {
        const items = [['folder', 'Desktop'], ['folder', 'Documents'], ['folder', 'Downloads'], ['folder', 'Pictures'], ['folder', 'Projects'], ['folder', 'WinDock'],
          ['doc', 'Roadmap.docx'], ['doc', 'Budget.xlsx'], ['img', 'Wallpaper.png'], ['img', 'Mockup.png'], ['doc', 'Notes.txt'], ['folder', 'Music']];
        return `<div class="files">
          <div class="files-side">
            ${[['Home', 1], ['Desktop'], ['Documents'], ['Downloads'], ['Pictures'], ['Music'], ['This PC']].map(([n, on]) =>
              `<div class="${on ? 'on' : ''}">${ui(n === 'This PC' ? 'monitor' : 'folder', 15)}${n}</div>`).join('')}
          </div>
          <div class="files-main">
            <div class="files-path">${ui('folder', 14)} Home ${ui('chevron', 12)} Dave</div>
            <div class="files-grid">${items.map(([k, n]) => `<figure><div class="fi ${k}"></div>${n}</figure>`).join('')}</div>
          </div></div>`;
      },
    },

    browser: {
      html: (w, o) => {
        const p = o.page || 0;
        const pages = [
          { tab: 'Laminar Dock', url: 'laminar.local/design', h: 'Laminar Dock', s: 'A fast, glassy dock for Windows.', g: 'linear-gradient(135deg,#5770FF,#B05BFF 55%,#FF7EB3)' },
          { tab: 'Weather · Toronto', url: 'weather.example/toronto', h: '7° Partly Cloudy', s: 'Toronto · High 9° Low 2°', g: 'linear-gradient(160deg,#4FA3FF,#2457D6)' },
        ][p];
        return `<div class="browser">
          <div class="browser-tabs"><span class="on">${pages.tab}</span><span>New Tab</span></div>
          <div class="browser-url">${ui('lock', 13)} ${pages.url}</div>
          <div class="browser-page">
            <div class="browser-hero" style="background:${pages.g}"><h1>${pages.h}</h1><p>${pages.s}</p></div>
            <div class="browser-cards"><div></div><div></div><div></div></div>
          </div></div>`;
      },
      title: (o) => ['Laminar Dock — Browser', 'Weather · Toronto — Browser'][o.page || 0],
    },

    music: {
      html: () => `<div class="music">
          <div class="music-art"><div class="art" data-art></div></div>
          <div class="music-info">
            <h2 data-title></h2><div class="artist" data-artist></div>
            <div class="music-progress"><i data-bar></i></div>
            <div class="music-times"><span data-pos></span><span data-dur></span></div>
            <div class="music-ctrls">
              <button data-act="prev">${ui('prev', 22)}</button>
              <button data-act="play">${ui('pause', 30)}</button>
              <button data-act="next">${ui('next', 22)}</button>
            </div>
            <div class="music-list" data-list></div>
          </div></div>`,
      mount(body) {
        const M = LD.media;
        const $ = s => body.querySelector(s);
        body.querySelectorAll('[data-act]').forEach(b => b.addEventListener('click', () => {
          const a = b.dataset.act;
          if (a === 'play') M.toggle(); else M.next(a === 'next' ? 1 : -1);
        }));
        function full() {
          if (!body.isConnected) return;
          const t = M.track;
          $('[data-art]').innerHTML = artSVG(t.c);
          $('[data-title]').textContent = t.title;
          $('[data-artist]').textContent = t.artist + ' — ' + t.album;
          $('[data-dur]').textContent = fmt(t.dur);
          $('[data-act="play"]').innerHTML = ui(M.playing ? 'pause' : 'play', 30);
          $('[data-list]').innerHTML = M.tracks.map((x, i) => `<div class="${i === M.idx ? 'on' : ''}"><span>${x.title}</span><span>${fmt(x.dur)}</span></div>`).join('');
          tick();
        }
        function tick() {
          if (!body.isConnected) return;
          $('[data-pos]').textContent = fmt(M.pos);
          $('[data-bar]').style.width = (M.pos / M.track.dur * 100) + '%';
        }
        LD.on('media', full); LD.on('media-tick', tick);
        full();
      },
    },

    chat: {
      html: () => `<div class="chat">
        <div class="chat-side">
          ${[['Design Team', 'Dock looks amazing 🔥', '#FF7EB3,#7A5CFF', 1], ['Alex', 'See you at 2', '#5ED8FF,#2B54F0'], ['Sam', 'Sent the mockups', '#FFC078,#FF5A82']]
            .map(([n, m, g, on]) => `<div class="c ${on ? 'on' : ''}"><div class="av" style="background:linear-gradient(135deg,${g})"></div><div><b>${n}</b><span>${m}</span></div></div>`).join('')}
        </div>
        <div class="chat-main">
          <div class="bubble in">Has anyone tried the new dock build?</div>
          <div class="bubble out">Yes! The magnification feels just like it should.</div>
          <div class="bubble in">The glass refraction over the wallpaper is wild.</div>
          <div class="bubble in">Dock looks amazing 🔥</div>
        </div></div>`,
    },

    laminar: {
      html: () => `<div class="settings-host"></div>`,
      mount(body) { LD.settingsUI.mount(body.querySelector('.settings-host')); },
    },

    generic: {
      html: (w) => `<div class="generic">
          <div class="gi">${LD.icon(w.app)}</div>
          <h3>${LD.esc(LD.APPS[w.app] ? LD.APPS[w.app].name : 'App')}</h3>
          <p>A stand-in window. In the native app this is the real program; Laminar only manages its window.</p>
        </div>`,
    },
  };
})(window.LD);
