# Laminar Dock

Laminar Dock is a Windows taskbar replacement. It has a glassy dock and a three-panel Launchpad. It looks and feels like macOS but behaves like Windows. It is a new app that replaces the taskbar, not a restyle of it.

Status: **visual prototype** (HTML/CSS/JS in [`prototype/`](prototype/)). Native development has not started.

## Decisions so far (2026-11-07)

| Topic | Decision |
| --- | --- |
| Name | Laminar Dock |
| Native stack | Rust with Microsoft's `windows` crate. Rendering uses Windows Composition (DirectComposition / `Windows.UI.Composition`), and text uses DirectWrite. |
| Why that stack | Animations run on the GPU compositor, the real desktop behind can be blurred, live thumbnails come from DWM, and it uses very little memory and CPU. |
| Relationship to Explorer | Keep `explorer.exe` running, hide its taskbar, and reserve screen space as an AppBar. If Laminar exits or crashes, the Windows taskbar comes back. |
| Dock modes | Always visible (reserves space) **or** auto-hide. In auto-hide the dock slides off-screen and comes back over windows when the pointer enters a bottom-edge zone. The zone size and delays are settings. |
| Displays | Show on all displays by default. Settings: all / main only / display with the pointer, and whether each dock shows all apps or only that display's windows. |
| Windows key | Opens Launchpad (a toggle in settings). |
| Admin | Running elevated is acceptable. |
| Design language | Follows the principles of Apple's Human Interface Guidelines. No Apple assets: fonts are Segoe UI Variable, and icons are original or Fluent/Lucide-style. |
| Workflow | Lock the look and motion in the HTML prototype, then port it to native. |

## Run the prototype

You need Python (already installed: 3.12).

```bash
cd prototype && python -m http.server 5180 --bind 127.0.0.1
```

Then open <http://127.0.0.1:5180> in Edge or Chrome, full screen (F11) for the real effect. **Ctrl+Space** or **`** opens Launchpad. The Notes window that opens on start lists everything to try.

Settings and the dock layout are saved in the browser's local storage. To start fresh, open **Laminar Settings → About → Reset everything**.

## What the prototype shows

| Area | Included |
| --- | --- |
| Dock | Glass shelf with wallpaper refraction, magnification, labels, open/closed/focused indicators, badges, launch bounce, genie minimize, separators, stacks (grid and fan), clock |
| Interaction | Drag to reorder, drag off to remove, hold over an app to make a stack, drag from Launchpad or a stack into the dock, right-click menus everywhere, Windows-style click to focus/minimize, middle-click or Shift+click for a new window |
| Previews | Live window thumbnails on hover, close from the thumbnail, hover to "peek" at a window |
| Auto-hide | Slide away and reveal from a bottom-edge zone; maximized windows use the full height |
| Launchpad | Widgets (clock, weather, system monitor, calendar). Search across apps, files and settings with keyboard navigation. Pinned apps and collapsible All Apps. Control center (Wi-Fi, Bluetooth, media, focus, dark mode, sound with device and per-app mixer, background apps, Windows Update) |
| Settings | Appearance, Glass (tint, blur, vibrancy, refraction, edge, shadow), Dock, Behavior, Launchpad, Motion, About (with a `config.json` preview) |

All data is fake. Windows, files, networks, devices and tray apps are stand-ins.

## Prototype file map

| File | Purpose |
| --- | --- |
| `prototype/index.html` | Page skeleton and the refraction SVG filter |
| `prototype/css/base.css` | Design tokens, light/dark themes, the shared glass material, controls |
| `prototype/css/desktop.css` | Wallpaper, simulated windows, toasts |
| `prototype/css/dock.css` | Dock, labels, previews, stacks, drag ghost, menus |
| `prototype/css/launchpad.css` | Launchpad panels and control center |
| `prototype/css/settings.css` | Laminar Settings window |
| `prototype/js/state.js` | Settings defaults (same shape as the future `config.json`), app catalog, event bus, spring curve |
| `prototype/js/icons.js` | Original app icons (squircle SVG) and UI glyphs |
| `prototype/js/dock.js` | Dock layout, rendering, magnification, drag and drop, menus, auto-hide, refraction map |
| `prototype/js/windows.js` | Simulated window manager (focus, minimize, maximize, peek) |
| `prototype/js/launchpad.js` | Launchpad: widgets, search, apps, control center |
| `prototype/js/settings.js` | Schema-driven settings UI |
| `prototype/js/stacks.js`, `previews.js`, `menus.js`, `templates.js`, `main.js` | Stacks, hover previews, context menus, window contents, boot |

## Native roadmap

| Phase | Goal | Done when |
| --- | --- | --- |
| 0. Prove the stack | Rust + Composition spike: glass dock, 10 icons, magnification, springs, blur, wallpaper refraction | Smooth at the display's refresh rate, under 40 MB, about 0% idle CPU, measured |
| 1. Real dock | Window tracking, indicators, activate/minimize, pins, reorder, separators, menus, hide the Explorer taskbar, crash safety net | Used as the daily taskbar on the test PC |
| 2. Dock depth | Live previews (DWM thumbnails), stacks, drag apps in, drop files on icons | Feature parity with this prototype's dock |
| 3. Launchpad: centre | App list (`shell:AppsFolder`), fuzzy search, Windows Search for files | Finds any installed app or file quickly |
| 4. Launchpad: right | Control center, tray icon hosting | All tray apps reachable without the Windows taskbar |
| 5. Launchpad: left, settings, polish | Widgets, Settings window, motion polish | Settings in this prototype all work natively |

## Phase 0 status (native spike, 2026-11-07)

Code: [`native/laminar/`](native/laminar/), a Cargo workspace member. Build with `cargo build --release`, then run `target\release\laminar.exe`. Right-click the dock and choose **Quit Laminar** to exit.

| Flag | Purpose |
| --- | --- |
| `--measure` | Run for ~17 s, print startup/memory/CPU as JSON (also saved to `laminar-metrics.json`), then quit |
| `--demo-hover` | Hold the dock magnified at the 5th icon (for screenshots) |
| `--exit-after N` | Quit after N seconds |
| `LAMINAR_GPU=hardware` (env var) | Paint bitmaps with the GPU driver instead of WARP (for comparison) |

### Measured

| Metric | Result | Target |
| --- | --- | --- |
| Idle CPU (10 s) | 0.000% | ≈ 0% ✅ |
| CPU during simulated hover (63 moves/s) | 1.25% of one core | low ✅ |
| Task Manager memory (private working set) | 23.4 MB | < 40 MB ✅ |
| Private (committed) memory | 34.8 MB | < 40 MB ✅ |
| Startup to first frame | 662 ms | < 200 ms ❌ (≈ 610 ms is asking the shell for icons) |
| Binary size | 260 KB | — |

Memory per stage (private, hardware device): runtime 2.6 MB → shell icons +12.6 → compositor +0.8 → **GPU driver device +46.7** → surfaces +8.3. Painting with WARP (the software rasterizer) removes the driver cost. The GPU compositor still does all animation, so it's the default now.

### Verified vs not yet

| Item | Status |
| --- | --- |
| Win32 + Windows Composition from Rust | ✅ works |
| Live blur of the real desktop (`HostBackdropBrush`) | ✅ works. It needs `DWMWA_USE_HOSTBACKDROPBRUSH`; without it the glass is black |
| Real app icons from the shell (classic and Store apps) | ✅ 14 icons render sharply |
| Laminar logo drawn with Direct2D | ✅ |
| Magnification (expression animations), glass hover label, rim | ✅ Seen with the screen on. Note: while the monitor is asleep, Windows pauses composition animations (`GlobalPlaybackRate` reads 0), so test with the screen awake. |
| Launch bounce, click to launch, hover region, right-click quit | ⚠️ Dave trying by hand |

Fixed during verification: the rim nine-grid needs its source brush set to `Stretch = Fill`; otherwise it draws once, unstretched, in the middle of the dock.

Next fixes once animation is confirmed: an on-disk icon cache so startup skips the shell (target < 200 ms), a soft shelf shadow, custom blur radius, and wallpaper refraction.

## Later / to-do

- Brightness control in the control center. Deferred because the test machine is a desktop. External monitors will need DDC/CI.
- Widget gallery and "Edit Controls" customization.
- Reordering pinned apps inside Launchpad by dragging.
- Focus / Do Not Disturb has no public Windows API; needs investigation.
