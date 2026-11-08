/* Laminar Dock prototype — stacks (folders of apps in the dock).
   Grid view (default) or Fan view; rename inline; drag apps out to the dock. */
(function (LD) {
  'use strict';
  const pop = document.getElementById('stack-pop');
  let cur = null, anchor = null;

  const find = id => LD.state.dock.find(x => x.type === 'stack' && x.id === id);

  function open(st, anchorEl, rename) {
    if (!st || !anchorEl) return;
    LD.previews.hide();
    LD.menu.close();
    cur = st; anchor = anchorEl;
    LD.dock.suppressLabel(true);
    LD.dock.reveal(true);
    pop.classList.remove('show');
    if (st.view === 'fan' && !rename && st.apps.length) renderFan(st); else renderGrid(st, rename);
    requestAnimationFrame(() => requestAnimationFrame(() => pop.classList.add('show')));
  }

  function close() {
    if (!cur) return;
    cur = null;
    pop.classList.remove('show');
    LD.dock.suppressLabel(false);
    setTimeout(() => { if (!cur) pop.innerHTML = ''; }, 300);
    LD.dock.maybeHide();
  }

  function toggle(st, el) { if (cur === st) close(); else open(st, el); }

  function renderGrid(st, rename) {
    const r = anchor.getBoundingClientRect();
    const W = 372;
    const left = LD.clamp(r.left + r.width / 2 - W / 2, 8, innerWidth - W - 8);
    const dockTop = document.getElementById('dock').getBoundingClientRect().top;
    pop.style.left = left + 'px';
    pop.style.top = 'auto';
    pop.style.bottom = (innerHeight - dockTop + 12) + 'px';
    pop.innerHTML = `<div class="stack-grid glass strong" style="transform-origin:${r.left + r.width / 2 - left}px 100%">
        <div class="stack-title">
          <input value="${LD.esc(st.name)}" spellcheck="false" aria-label="Stack name">
          <span class="cnt">${st.apps.length} ${st.apps.length === 1 ? 'app' : 'apps'}</span>
        </div>
        <div class="stack-apps">
          ${st.apps.length ? st.apps.map(a => `<div class="stack-app" data-app="${a}"><div class="si">${LD.icon(a)}</div>${LD.esc(LD.APPS[a].name)}</div>`).join('')
            : `<div style="grid-column:1/-1;padding:18px;text-align:center;color:var(--text-2)">Drag apps onto this stack to add them.</div>`}
        </div>
        <div class="stack-foot"><button data-act="lp">Add apps from Launchpad…</button></div>
      </div>`;
    const input = pop.querySelector('input');
    input.addEventListener('input', () => { st.name = input.value || 'Stack'; LD.save(); });
    input.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === 'Escape') { e.stopPropagation(); input.blur(); } });
    if (rename) setTimeout(() => { input.focus(); input.select(); }, 60);
    pop.querySelector('[data-act="lp"]').addEventListener('click', () => { close(); LD.launchpad.open(); });
    wireApps(st, '.stack-app');
  }

  function renderFan(st) {
    const r = anchor.getBoundingClientRect();
    pop.style.left = (r.left + r.width / 2) + 'px';
    pop.style.top = 'auto';
    pop.style.bottom = (innerHeight - r.top + 8) + 'px';
    const items = st.apps.slice(0, 10);
    pop.innerHTML = `<div class="stack-fan">${items.map((a, i) => {
      const fy = -(i * 62) + 'px';
      const fx = (i * i * 2.4) + 'px';
      const fr = (i * 1.4) + 'deg';
      return `<div class="fan-item" data-app="${a}" style="--i:${i};--fx:${fx};--fy:${fy};--fr:${fr}">
          <div class="si">${LD.icon(a)}</div><span class="fl glass strong">${LD.esc(LD.APPS[a].name)}</span></div>`;
    }).join('')}</div>`;
    wireApps(st, '.fan-item');
  }

  function wireApps(st, sel) {
    pop.querySelectorAll(sel).forEach(el => {
      const appId = el.dataset.app;
      el.addEventListener('pointerdown', e => {
        if (e.button !== 0) return;
        const sx = e.clientX, sy = e.clientY;
        let started = false;
        const move = ev => {
          if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) > 6) {
            started = true;
            cleanup();
            const rect = el.querySelector('.si').getBoundingClientRect();
            close();
            LD.dockDragIn(appId, ev, rect, 'stack', { stackId: st.id });
          }
        };
        const up = () => { cleanup(); if (!started) { close(); LD.openApp(appId); } };
        const cleanup = () => { removeEventListener('pointermove', move); removeEventListener('pointerup', up); };
        addEventListener('pointermove', move);
        addEventListener('pointerup', up);
      });
      el.addEventListener('contextmenu', e => {
        e.preventDefault();
        e.stopPropagation();
        LD.menu(e.clientX, e.clientY, [
          { label: 'Open', onClick: () => { close(); LD.openApp(appId); } },
          { sep: true },
          { label: 'Move to Dock', onClick: () => { removeApp(st.id, appId, true); LD.pinToDock(appId, true); close(); } },
          { label: 'Remove from Stack', danger: true, onClick: () => { removeApp(st.id, appId); if (cur) open(st, LD.dock.elFor('stack:' + st.id)); } },
        ]);
      });
    });
  }

  /** Remove an app from a stack. A stack left with one app becomes that app; an empty one disappears. */
  function removeApp(stackId, appId, silent) {
    const st = find(stackId);
    if (!st) return;
    st.apps = st.apps.filter(a => a !== appId);
    const i = LD.state.dock.indexOf(st);
    if (st.apps.length === 1) LD.state.dock[i] = { type: 'app', id: st.apps[0] };
    else if (st.apps.length === 0) LD.state.dock.splice(i, 1);
    if (!silent) { LD.dock.render(); LD.save(); }
  }

  function ungroup(st) {
    const i = LD.state.dock.indexOf(st);
    if (i < 0) return;
    const apps = st.apps.filter(a => !LD.isPinned(a)).map(a => ({ type: 'app', id: a }));
    LD.state.dock.splice(i, 1, ...apps);
    close();
    LD.dock.render();
    LD.save();
  }

  document.addEventListener('pointerdown', e => {
    if (!cur) return;
    if (e.target.closest('#stack-pop') || e.target.closest('.menu')) return;
    if (anchor && anchor.contains(e.target)) return;   // the dock click handler toggles
    close();
  }, true);
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && cur && !LD.menu.isOpen()) close(); });

  LD.stacks = { open, close, toggle, isOpen: () => !!cur, removeApp, ungroup };
})(window.LD);
