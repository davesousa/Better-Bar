/* Laminar Dock prototype — context menus (with submenus and keyboard navigation).
   Item shape: { label, onClick, shortcut, checked, disabled, danger, submenu: [...] }
   or { sep: true } or { header: 'Text' }. */
(function (LD) {
  'use strict';
  const layer = document.getElementById('menu-layer');
  let stack = [];          // open menu elements (root first)
  let onCloseCb = null;

  function build(items, level) {
    const m = document.createElement('div');
    m.className = 'menu glass strong';
    m.setAttribute('role', 'menu');
    m._items = [];
    items.forEach(it => {
      if (!it) return;
      if (it.sep) { const s = document.createElement('div'); s.className = 'msep'; m.appendChild(s); return; }
      if (it.header) { const h = document.createElement('div'); h.className = 'mh'; h.textContent = it.header; m.appendChild(h); return; }
      const row = document.createElement('div');
      row.className = 'mi' + (it.disabled ? ' dis' : '') + (it.danger ? ' danger' : '');
      row.setAttribute('role', 'menuitem');
      row.innerHTML = `<span class="ck">${it.checked ? LD.ui('check', 13, 2.4) : ''}</span>
        <span class="lbl">${LD.esc(it.label)}</span>
        ${it.shortcut ? `<span class="sc">${LD.esc(it.shortcut)}</span>` : ''}
        ${it.submenu ? `<span class="sub">${LD.ui('chevron', 12, 2.2)}</span>` : ''}`;
      row._item = it;
      row.addEventListener('pointerenter', () => highlight(m, row, level));
      row.addEventListener('click', e => { e.stopPropagation(); activate(row, level); });
      m.appendChild(row);
      m._items.push(row);
    });
    m.addEventListener('contextmenu', e => e.preventDefault());
    return m;
  }

  function highlight(m, row, level) {
    m._items.forEach(r => r.classList.toggle('on', r === row));
    m._active = row;
    // close deeper submenus, open this one's
    closeFrom(level + 1);
    clearTimeout(m._subT);
    if (row && row._item.submenu && !row._item.disabled) {
      m._subT = setTimeout(() => openSub(row, level), 120);
    }
  }

  function openSub(row, level) {
    closeFrom(level + 1);
    const sub = build(row._item.submenu, level + 1);
    layer.appendChild(sub);
    const r = row.getBoundingClientRect();
    place(sub, r.right + 2, r.top - 5, { altX: r.left - 2 });
    stack.push(sub);
  }

  function activate(row, level) {
    const it = row._item;
    if (it.disabled) return;
    if (it.submenu) { openSub(row, level); return; }
    // Brief flash like macOS, then run.
    row.classList.remove('on');
    setTimeout(() => {
      row.classList.add('on');
      setTimeout(() => { close(); it.onClick && it.onClick(); }, 70);
    }, 50);
  }

  function place(m, x, y, o = {}) {
    m.style.left = '0px'; m.style.top = '0px';
    const w = m.offsetWidth, h = m.offsetHeight;
    let left = x, top = y;
    if (o.above) top = y - h;
    if (left + w > innerWidth - 6) left = (o.altX != null ? o.altX - w : innerWidth - w - 6);
    if (top + h > innerHeight - 6) top = Math.max(6, innerHeight - h - 6);
    left = Math.max(6, left); top = Math.max(6, top);
    m.style.left = left + 'px';
    m.style.top = top + 'px';
    m.style.transformOrigin = `${x - left}px ${o.above ? '100%' : (y - top) + 'px'}`;
    requestAnimationFrame(() => m.classList.add('show'));
  }

  function closeFrom(level) {
    while (stack.length > level) { const m = stack.pop(); m.remove(); }
  }

  function close() {
    if (!stack.length) return;
    stack.forEach(m => {
      m.classList.remove('show');
      m.style.pointerEvents = 'none';
      setTimeout(() => m.remove(), 150);
    });
    stack = [];
    const cb = onCloseCb; onCloseCb = null;
    cb && cb();
  }

  /** Show a menu at (x, y). opts.above opens it upward (used by the dock). */
  LD.menu = function (x, y, items, opts = {}) {
    close();
    const m = build(items, 0);
    layer.appendChild(m);
    place(m, x, y, opts);
    stack = [m];
    onCloseCb = opts.onClose || null;
  };
  LD.menu.close = close;
  LD.menu.isOpen = () => stack.length > 0;

  document.addEventListener('pointerdown', e => {
    if (stack.length && !e.target.closest('.menu')) close();
  }, true);
  window.addEventListener('blur', close);
  window.addEventListener('resize', close);

  document.addEventListener('keydown', e => {
    if (!stack.length) return;
    const m = stack[stack.length - 1];
    const rows = m._items.filter(r => !r._item.disabled);
    const i = rows.indexOf(m._active);
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); if (stack.length > 1) closeFrom(stack.length - 1); else close(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); highlight(m, rows[(i + 1) % rows.length], stack.length - 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); highlight(m, rows[(i - 1 + rows.length) % rows.length], stack.length - 1); }
    else if (e.key === 'ArrowRight' && m._active && m._active._item.submenu) { e.preventDefault(); openSub(m._active, stack.length - 1); const s = stack[stack.length - 1]; highlight(s, s._items[0], stack.length - 1); }
    else if (e.key === 'ArrowLeft' && stack.length > 1) { e.preventDefault(); closeFrom(stack.length - 1); }
    else if (e.key === 'Enter' && m._active) { e.preventDefault(); activate(m._active, stack.length - 1); }
  }, true);
})(window.LD);
