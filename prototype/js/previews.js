/* Laminar Dock prototype — live window previews on hover.
   The native app will use DWM thumbnails (GPU-composited live window images);
   here we clone the simulated window and refresh it every second. */
(function (LD) {
  'use strict';
  const pop = document.getElementById('preview-pop');
  let appId = null, anchor = null, showT = 0, hideT = 0, refreshT = 0, peekT = 0;
  let open = false;

  function hoverIcon(id, el) {
    clearTimeout(hideT);
    if (open) { if (id !== appId) show(id, el); return; }   // already open: swap instantly
    clearTimeout(showT);
    showT = setTimeout(() => show(id, el), 380);
  }
  function leaveIcon() {
    clearTimeout(showT);
    if (open) scheduleHide();
  }
  function scheduleHide() {
    clearTimeout(hideT);
    hideT = setTimeout(hide, 280);
  }

  pop.addEventListener('pointerenter', () => clearTimeout(hideT));
  pop.addEventListener('pointerleave', () => { endPeek(); scheduleHide(); });

  function build() {
    const wins = LD.wm.byApp(appId);
    if (!wins.length) { hide(); return; }
    pop.innerHTML = '';
    wins.forEach(w => {
      const TW = 224;
      const ar = LD.clamp(w.h / w.w, 0.45, 0.8);
      const card = document.createElement('div');
      card.className = 'thumb' + (w.min ? ' is-min' : '');
      card.innerHTML = `<div class="thumb-head"><span class="mi">${LD.icon(w.app)}</span><span class="tt">${LD.esc(w.title)}</span>
          <button class="tx" title="Close window">${LD.ui('x', 11, 2.4)}</button></div>
        <div class="shot" style="height:${Math.round(TW * ar) - 12}px"></div>`;
      fillShot(card.querySelector('.shot'), w, TW - 12);
      card.addEventListener('click', e => {
        if (e.target.closest('.tx')) return;
        endPeek(); hide();
        if (w.min) LD.wm.restore(w.id); else LD.wm.focus(w.id);
      });
      card.querySelector('.tx').addEventListener('click', e => {
        e.stopPropagation();
        endPeek();
        LD.wm.close(w.id);
        card.animate([{ opacity: 1, transform: 'none' }, { opacity: 0, transform: 'scale(.9)' }], { duration: 180 }).onfinish = () => {
          if (LD.wm.byApp(appId).length) { build(); position(); } else hide();
        };
      });
      card.addEventListener('pointerenter', () => { clearTimeout(peekT); peekT = setTimeout(() => LD.wm.peek(w.id), 320); });
      card.addEventListener('pointerleave', () => { clearTimeout(peekT); LD.wm.unpeek(); });
      card._win = w;
      pop.appendChild(card);
    });
  }

  function fillShot(shot, w, width) {
    const clone = w.el.cloneNode(true);
    clone.classList.remove('minimized', 'peek-target', 'geo-anim');
    clone.removeAttribute('data-id');
    clone.querySelectorAll('[id]').forEach(n => n.removeAttribute('id'));
    const ww = w.max ? innerWidth : w.w;
    const hh = w.max ? LD.wm.workArea().h : w.h;
    clone.style.width = ww + 'px';
    clone.style.height = hh + 'px';
    clone.style.transform = `scale(${width / ww})`;
    clone.style.zIndex = 0;
    shot.innerHTML = '';
    shot.appendChild(clone);
  }

  function refresh() {
    pop.querySelectorAll('.thumb').forEach(card => {
      if (card._win && card._win.el.isConnected) fillShot(card.querySelector('.shot'), card._win, 212);
    });
  }

  function position() {
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    const dockTop = document.getElementById('dock').getBoundingClientRect().top;
    const w = pop.offsetWidth;
    const left = LD.clamp(r.left + r.width / 2 - w / 2, 8, innerWidth - w - 8);
    pop.style.left = left + 'px';
    pop.style.top = 'auto';
    pop.style.bottom = (innerHeight - Math.min(dockTop, r.top) + 12) + 'px';
    pop.style.transformOrigin = `${r.left + r.width / 2 - left}px 100%`;
  }

  function show(id, el) {
    if (LD.stacks.isOpen() || LD.menu.isOpen()) return;
    appId = id; anchor = el;
    build();
    if (!LD.wm.byApp(id).length) return;
    position();
    open = true;
    pop.classList.add('show');
    LD.dock.suppressLabel(true);
    clearInterval(refreshT);
    refreshT = setInterval(refresh, 1000);
  }

  function hide() {
    clearTimeout(showT); clearTimeout(hideT);
    if (!open) return;
    open = false;
    endPeek();
    clearInterval(refreshT);
    pop.classList.remove('show');
    LD.dock.suppressLabel(false);
    LD.dock.maybeHide();
  }

  function endPeek() { clearTimeout(peekT); LD.wm.unpeek(); }

  LD.on('windows', () => { if (open) { build(); position(); } });

  LD.previews = {
    hoverIcon, leaveIcon, hide,
    showPinned(id, el) { clearTimeout(showT); show(id, el); },
    isOpen: () => open,
  };
})(window.LD);
