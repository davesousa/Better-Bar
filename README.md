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

| Phase | Goal | Done when | Status (2026-11-10) |
| --- | --- | --- | --- |
| 0. Prove the stack | Rust + Composition spike: glass dock, 10 icons, magnification, springs, blur, wallpaper refraction | Smooth at the display's refresh rate, under 40 MB, about 0% idle CPU, measured | ✅ (wallpaper refraction still to do) |
| 1. Real dock | Window tracking, indicators, activate/minimize, pins, reorder, separators, menus, hide the Explorer taskbar, crash safety net | Used as the daily taskbar on the test PC | ✅ built; awaiting Dave's daily use |
| 2. Dock depth | Live previews (DWM thumbnails), stacks, drag apps in, drop files on icons | Feature parity with this prototype's dock | ✅ built and agent-tested |
| 3. Launchpad: centre | App list (`shell:AppsFolder`), fuzzy search, Windows Search for files | Finds any installed app or file quickly | ✅ built and agent-tested |
| 4. Launchpad: right | Control center, tray icon hosting | All tray apps reachable without the Windows taskbar | ✅ built: controls, tray icons (Background Apps) and Windows Update status. ⚠️ Tray clicks need Dave's hand test. |
| 5. Launchpad: left, settings, polish | Widgets, Settings window, motion polish | Settings in this prototype all work natively | ✅ built: widgets, Settings, light theme, extra blur. ❌ Refraction: Windows refuses it (see below). |

## Overnight build status (2026-11-08)

This section records the overnight build Dave tested on 2026-11-08; later rounds follow below. "Agent-tested" means checked with screen captures and synthetic mouse input on the test PC; it is not the same as a person using it.

### How to use it

| Task | How |
| --- | --- |
| Start | `target\release\laminar.exe` (extra displays get a helper dock automatically) |
| Open Launchpad | Click the Laminar logo at the left of the dock, or press the Windows key on its own |
| Settings | Launchpad → gear button, or right-click the dock → **Dock Settings…**. Changes apply live and save to `%APPDATA%\Laminar\config.json`. |
| Reorder | Drag an icon along the dock. Drag a pinned icon up and away to remove it. |
| Pin from Launchpad | Drag an app tile out of Launchpad onto the dock, or right-click it → **Keep in Dock** |
| Pin from File Explorer | Drag a program, shortcut or folder onto the dock: the icons part, and it's pinned where you drop it. To pin a document, hold **Alt** while dragging. |
| Wi-Fi networks | Launchpad → click the **Wi-Fi** label (the round button only switches Wi-Fi on or off). Saved networks connect directly; **Other Networks…** opens Windows' list for new networks and passwords. |
| Make a stack | Drag an app and hold it over the middle of another pinned app for a moment |
| Add a separator | Right-click a pinned app → **Add Separator After** |
| Previews | Hover an open app; click a card to switch, × to close that window |
| Open files with an app | Drag documents from File Explorer onto an app's icon; drop on the Recycle Bin to delete (undoable) |
| Attention dots | When an app flashes its taskbar button (Slack, Teams, Outlook reminders and so on), a green dot with a slow breathing glow appears beside its running dot. It clears when you switch to that window or close it. If the dock is auto-hidden, it peeks up for about 3 seconds so you see it (not over full-screen games, videos or presentations). |
| Quit | Right-click the dock → **Quit Laminar**. The Windows taskbar comes back. |

### Built overnight

| Area | What it does |
| --- | --- |
| Auto-hide fix | The slide was clipped to 2–3 frames because the window region shrank before the animation finished. Now the region shrinks after the slide, the slide travels only the dock's height, hiding eases in and out (300 ms) and showing uses a spring (460 ms). |
| Menus | Glass context menus drawn by Laminar, with keyboard navigation, checkmarks and shortcuts |
| Drag and drop | Reorder with neighbours sliding aside, drag off to remove, drag in from Launchpad, hold-to-merge into stacks |
| Stacks | Pinned folders of apps, opening as a glass grid above the dock |
| Live previews | DWM live thumbnails of each window, with close buttons |
| File drops | Open files with classic apps (command line) or Store apps (Windows activation API, with a fallback for packaged desktop apps); Recycle Bin |
| Launchpad, left | Clock, date, weather (Open-Meteo, Toronto by default), CPU, memory, disk, network, battery |
| Launchpad, centre | Search across apps, files (Windows Search) and settings; pinned apps; collapsible All Apps |
| Launchpad, right | Volume and output device, media controls, Wi-Fi/Bluetooth toggles, dark mode, power actions |
| Settings view | Dock size, magnification and size, spacing, padding, corner radius, glass tint and edge highlight, auto-hide timings, taskbar, Windows key, displays |
| Windows key | A lone Windows key press opens Launchpad; Windows key shortcuts (Win+E and so on) still work |
| Multiple displays | One helper dock per extra display, sized for that display's scaling. Options: show on all displays; each display shows only its own windows. |
| System code | `native/laminar-sys/`: apps, search, files, audio, media, radios, theme, stats, weather, power |

### Measured (release build, 2026-11-08)

| Metric | Result | Target |
| --- | --- | --- |
| Startup to first frame | 87–92 ms | < 200 ms ✅ |
| Task Manager memory | 21.2 MB | < 40 MB ✅ |
| Private memory | 28.5–28.7 MB | < 40 MB ✅ |
| Idle CPU | 0% | ≈ 0% ✅ |
| CPU while sweeping the pointer | 2.5–3.1% of one core | low ✅ |
| Binary size | ≈ 1 MB | — |
| Compiler warnings | 0 | 0 ✅ |

### Agent-tested vs still needs a person

| Check | Result |
| --- | --- |
| Auto-hide slide renders as a multi-frame motion | ✅ captured mid-slide; **smoothness needs Dave's eyes** |
| Styled menu, drag reorder (saved order correct), drag off to remove | ✅ |
| Stack created by hold-merge; stack popover opens | ✅ |
| Live previews | ✅ |
| Launchpad: all three panels with live data, search, All Apps, Settings view, right-click menu | ✅ |
| Drag from Launchpad into the dock | ✅ |
| Drop a file on the Recycle Bin; drop a file on Paint (Store app) | ✅ |
| Helper dock on the 4K display at 150% | ✅ |
| Quit restores the taskbar; no processes left | ✅ |
| Windows key opens Launchpad | ✅ Dave uses it daily (the hook deliberately ignores synthetic key presses, so only a person can test it) |
| Dropping a file on a classic (non-Store) app | ⚠️ not yet tried |
| Switching the default audio output | ⚠️ not yet tried |

### Fixes from Dave's first test round (2026-11-08)

| Bug | Cause | Fix and check |
| --- | --- | --- |
| A third app added to a stack made the stack and other pins vanish | The code that finds the end of the pinned apps stopped at the first stack, so saving after any drag dropped the stack and everything after it | Stacks now count as pinned. ✅ Agent-tested: a stack grew to 4 apps, then reordering kept all pins. |
| Couldn't drag anything into the dock | Drops were only accepted on app icons and the Recycle Bin | Programs, shortcuts and folders open a gap and pin where dropped; Alt pins documents. ✅ Agent-tested: three drops pinned in the right places, a document still opened in Paint, cancelled drags changed nothing. |
| Couldn't select Wi-Fi networks | Launchpad only had an on/off switch; the network list was never shown | New Wi-Fi view with networks, signal, lock and connected marks. ✅ Layout checked with sample networks. ⚠️ Joining a network not tried (this PC's Wi-Fi is off). |
| (testing aid) | | `--demo lp-wifi` opens the Wi-Fi view; `--demo lp-wifi-sample` shows made-up networks for layout checks |

### Attention dots (2026-11-09)

| Part | How it works |
| --- | --- |
| Signal | Apps ask for attention with `FlashWindowEx`, which Windows reports to shell windows as an `HSHELL_FLASH` message. The dock registers for those with `RegisterShellHookWindow`, the same way taskbar replacements do. |
| Clearing | When the window comes to the front or closes. Windows also sends the flash message repeatedly while a button blinks, so only the first one counts. |
| Look | A green dot beside the running dot; the pair slides so it stays centred under the icon. A soft radial glow behind the dot swells and brightens on a 2.4-second eased loop. If Windows' **Animation effects** setting is off, the glow stays lit without moving. |
| Checked | ✅ Agent-tested with a test window that flashes without taking focus: the trace logged the flash, the dot appeared, and four captures 0.6 s apart showed the glow pulsing. ⚠️ Not yet tried: the auto-hide peek (the test copy ran beside Dave's dock, which keeps the test copy shown), clearing when the window is focused, and a real Slack message. |
| Not covered | Unread-count badges that apps draw on their taskbar icon (Slack's red number) go to Explorer's taskbar through its overlay-icon API, and Laminar doesn't receive them today. Only the flash is picked up. |

### Fixes from Dave's second test round (2026-11-10)

| Bug or request | Cause | Fix and check |
| --- | --- | --- |
| Dock stuck visible after dragging an app from Launchpad into it | The drag ran its own mouse loop, so the dock never heard the pointer leave | After the drop the dock re-checks where the pointer is and hides if it's elsewhere. ✅ Agent-tested (trace). |
| Dock stuck visible after right-click → **Dock Settings…**, then closing Launchpad | Windows sends one "pointer left" notice per hover; it arrived while the menu was open and was ignored. The menu's "Dock Settings…" item sits over the dock's window, so when the menu closed the dock still thought it was hovered and waited for a notice that never came | Closing a menu or Launchpad now re-checks where the pointer really is and restarts hover tracking. Found from Dave's dock trace log: closes after the menu route never started the hide timer; closes after the gear route did. ✅ Dave checked by hand. |
| Couldn't reorder apps inside a stack | Not built | Drag an app within the open stack. ✅ Agent-tested. |
| Couldn't rename a stack | Not built | Click the stack's title, or right-click the stack → **Rename Stack…**. Enter or clicking away saves; Esc cancels. ✅ Agent-tested. |
| Dock hid while a stack was open | The hide timer ignored the stack | The dock stays up until the stack closes. ✅ Agent-tested. |
| Dock stayed up after clicking just outside an open stack | The stack's window is bigger than its panel (room for the shadow), so a click just outside counted as a click inside | Clicks in that margin now close the stack. ✅ Dave checked by hand. |
| Auto-hidden dock popped up at random (switching windows, browser tabs, monitors) | WSL's hidden "RemoteApp" window (`msrdc.exe`) "flashes" every 1–15 s. Each flash peeked the dock, and every window change reset it so the next flash peeked again | Only visible windows the dock shows can ask for attention. ✅ Test logs showed the flashes and the window's identity; Dave saw no stray pop-ups in his first stretch of normal use. ⚠️ Worth a full day of use. Dave's dock logs to a trace file for now, so any remaining pop-up can be traced. |
| Preview of a minimized window said "Minimized" | The preview skipped minimized windows | It now asks Windows for the window's last picture (Windows reports a full-size one for minimized windows), with the app icon and "Minimized" underneath as a fallback. ✅ Dave checked by hand: minimized windows show their content. |

### Phase 4 and 5 completion (2026-11-10)

| Item | How it works | Checked |
| --- | --- | --- |
| Slack/Discord pins survive updates | These apps install each version in `app-x.y.z` and keep a launcher in the folder above. Running windows and saved pins now use that launcher; old pins convert automatically. | ✅ Running Slack and Discord now map to `slack\slack.exe` and `discord\discord.exe`; both apps had already downloaded their next version, so the old pins would have broken at their next restart. |
| Tray icons | Launchpad's right panel → **Background Apps**. Laminar runs a hidden window with the taskbar's class name (`Shell_TrayWnd`) placed in front of Explorer's, so apps' `Shell_NotifyIcon` calls reach it first. Every message is passed on to Explorer, so Explorer's tray stays complete for when Laminar quits. AppBar and other taskbar messages go only to Explorer. On start it broadcasts `TaskbarCreated`, so apps re-register their icons. Click = the app's left click (opens its window); right-click = its own menu. Hover shows the icon's name. The approach is the one Cairo and RetroBar use (ManagedShell). | ✅ Trace: 19 icons registered within 2 s of starting (PowerToys, Windows Security, Telegram, Windhawk, Lightshot, Tailscale, Phone Link and more); Laminar's window is first in line, both taskbar windows stay hidden. ✅ Dave checked by hand (after fixing blank pictures and a missed click message): most apps open their window; apps whose click opens a menu show it where you clicked, after Launchpad closes, the same as clicking in the Windows tray. Kept on purpose (see "Tray clicks that open a menu"). |
| Windows Update status | Launchpad's right panel. Reads the result of Windows' own last scan (no new scan, no download) on its own thread, at most every 20 minutes: restart needed, N updates waiting, or up to date. The row opens Settings → Windows Update; **Restart** opens the power menu. | ✅ The check returned "5 updates waiting" in 5.9 s on this PC. ⚠️ Compare the count with Windows Settings. |
| Light theme | Glass, text, cards, menus, previews, stacks and Launchpad follow Windows' light/dark app mode and switch live. Setting: **Glass → Match Windows light/dark mode** (off = always dark). `LAMINAR_THEME=light` forces it for testing. | ✅ Test dock captured in light: pale glass, dark dots and separators. ⚠️ Menus, previews, stacks and Launchpad in light need a look by hand. |
| Extra blur | **Glass → Extra blur** (0–40 px) adds a Gaussian blur on top of Windows' own background blur (Windows' blur is the minimum). | ✅ Windows accepted the effect and the dock rendered. ⚠️ Not yet judged by eye. |
| Refraction | Not possible on the live desktop. Windows' compositor refused both ways to bend its blurred background: a transform ("Backdrop brushes cannot be transformed") and Direct2D's displacement map ("Unsupported effect type"). Bending only the wallpaper image would show wallpaper over windows, so it was dropped. | ❌ Recorded in `effects.rs`. |

#### Tray clicks that open a menu (decision, 2026-11-10)

Launchpad closes on every tray click, so whatever the app opens (its window, a menu or a flyout) appears in front. An app's menu then appears where you clicked. Keeping Launchpad open behind menus was considered and rejected: Laminar can't know in advance whether an app will open a window, a menu or a Windows flyout, and windows or flyouts that open behind a full-screen Launchpad would look broken.

### Drag through the dock to a window (2026-11-11)

Dave's request: drag a file from one window into another without having both on screen first.

| Step | What happens |
| --- | --- |
| 1. Drag anything (files, text, images) over a running app in the dock and hold still | After 0.6 s its window tiles open, as when hovering. Dropping on the icon itself still opens the files with that app. |
| 2. Move up onto a tile and hold still | After 0.5 s that window comes forward and the tiles close; dropping on a tile does the same at once. |
| 3. Move into the window and let go | The window receives the drop as usual. |

How: the window tiles get their own drop target (the same OLE mechanism the dock uses). Windows only lets the app that sent the last input change the front window, and during a drag that's the app being dragged from, so Laminar first sends an empty mouse input (no movement or clicks), the step PowerToys uses. Checked: ✅ Dave tested it by hand on 2026-11-11 ("works well").

### Known gaps

- Tray icons (the apps normally beside the clock) aren't hosted yet, and there's no Windows Update status. *(Done 2026-11-10.)*
- Blur radius and wallpaper refraction aren't adjustable in the native app yet; there's no light theme. *(Light theme and extra blur done 2026-11-10; refraction isn't possible, see below.)*
- Pinning a running app by exe path breaks for apps whose folder changes on update (for example Discord). *(Fixed 2026-11-10.)*

## Phase 0 status (native spike, 2026-11-07, historical)

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

## Phase 1, slice 1 status (2026-11-07, historical)

The dock can now stand in for the Windows taskbar day to day.

### Using it

| Task | How |
| --- | --- |
| Start | `target\release\laminar.exe` (only one copy runs at a time) |
| Quit | Right-click the dock → **Quit Laminar**. The Windows taskbar comes back. |
| Settings | Right-click → **Edit settings file…** opens `%APPDATA%\Laminar\config.json` in Notepad. Saving applies the changes immediately; a file with a mistake is ignored until it's fixed. |
| Pin or unpin | Right-click an app → **Keep in Dock**. Or edit `pins` in the settings file (`"|"` = separator). |
| Auto-hide | Right-click → **Automatically hide the dock**, or set `"auto_hide": true`. Push the pointer into the bottom edge to bring it back. |
| Bring back the Windows taskbar | Right-click → untick **Hide the Windows taskbar** |
| Troubleshoot app matching | `laminar.exe --list-windows` prints what the dock sees for each window |

While Laminar hides the Windows taskbar, the **clock and tray icons aren't visible** (they arrive in the Launchpad's right panel in Phase 4). Start (Win key), Win+N and Win+A should still work.

### What's new

| Feature | Notes |
| --- | --- |
| Running-window tracking | Event-driven (WinEvent hooks + a 60 ms debounced rescan; no polling). Uses the same visibility rules as the taskbar. |
| App matching | Window AppUserModelID → packaged-app id → exe path. Pins match by id, exe, or the program the pin launches (cached in `targets.json`). |
| Running and focused indicators | Dot under running apps, wider pill under the app in front; animated by the compositor |
| Unpinned running apps | Shown after a separator, in the order they opened |
| Windows-style clicks | Closed → launch with bounce; behind → bring to front; in front → minimize (or cycle its windows) |
| Auto-hide | Spring slide; a thin bottom-edge band reveals it; maximized windows use the full height |
| Taskbar takeover | Hides Explorer's taskbar, switches it to auto-hide if needed, re-hides it if Explorer restarts or re-shows it, and reserves screen space as an AppBar when the dock is always visible |
| Safety net | `laminar.exe --watchdog <pid>` waits for the dock to exit (even a crash) and restores the taskbar. Quit, Windows sign-out and shutdown also restore it directly. |
| Icon cache | `%LOCALAPPDATA%\Laminar\icons\`: 256 px PNGs + `names.json` + `targets.json` |

### Measured (warm start, 22 icons, 11 open windows)

| Metric | Result | Target |
| --- | --- | --- |
| Startup to first frame | 77–80 ms | < 200 ms ✅ |
| Task Manager memory | 16.9 MB | < 40 MB ✅ |
| Private memory | 23.6–24.3 MB | < 40 MB ✅ |
| Idle CPU | 0.000% | ≈ 0% ✅ |
| CPU while sweeping the pointer | 1.3–3.1% of one core | low ✅ |

### Verified

| Check | Result |
| --- | --- |
| Dock shows pins, a separator, then open unpinned apps, with dots and the focused pill | ✅ screen capture |
| File Explorer windows match its pin (no duplicate) | ✅ after an explicit Explorer mapping |
| Forced kill (simulated crash) | ✅ taskbar visible again, full work area restored, watchdog exits |
| Live settings: `auto_hide` on → space released and dock tucked; off → space reserved again | ✅ |
| Timed exit | ✅ taskbar restored, no processes left |
| Clicking, reveal from the bottom edge, menu Quit | ⚠️ needs trying by hand |

Bugs found and fixed while testing: expression strings got "too long" with ~20 icons (now chained, constant length); a settings file saved with a byte-order mark was rejected; a bad settings file caused a reload loop; quitting from the menu skipped cleanup (the watchdog covered it); slow startup from shell lookups (now cached).

### Known gaps
- Pinning a running app by exe path breaks for apps whose folder changes on update (e.g. Discord, Slack `app-x.y.z`). Fix: pin through their Start-menu id instead.
- Drag to reorder, styled menus, previews and stacks are still to come (later Phase 1 slices and Phase 2). *(Done 2026-11-08.)*
- Only the primary display for now. *(Done 2026-11-08.)*

## Later / to-do

- Brightness control in the control center. Deferred because the test machine is a desktop. External monitors will need DDC/CI.
- Widget gallery and "Edit Controls" customization.
- Reordering pinned apps inside Launchpad by dragging.
- Focus / Do Not Disturb has no public Windows API; needs investigation.
