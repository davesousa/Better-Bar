# Laminar Dock

Laminar Dock is a Windows taskbar replacement. It has a glassy dock and a three-panel Launchpad. It looks and feels like macOS but behaves like Windows. It is a new app that replaces the taskbar, not a restyle of it.

Status: **visual prototype** (HTML/CSS/JS in [`prototype/`](../prototype/)). Native development has not started.

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
| 5. Launchpad: left, settings, polish | Widgets, Settings window, motion polish | Settings in this prototype all work natively | ✅ built: widgets, Settings, light theme, extra blur. ❌ Refraction: Windows refuses it (see below). 2026-11-15: widget gallery (106 widgets, connectors, dock widgets, drag to reorder). |

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

### Installer and updates (Velopack, 2026-11-11)

Decision (Dave, 2026-11-11): use [Velopack](https://docs.velopack.io/) with updates hosted on this repo's GitHub Releases.

| Part | How it works |
| --- | --- |
| Install | `LaminarDock-win-Setup.exe` from the GitHub Release installs per user in `%LocalAppData%\LaminarDock` (no admin), adds Start menu and desktop shortcuts, and starts Laminar. Settings stay in `%AppData%\Laminar`, so updates and reinstalls keep them. |
| Start at sign-in | Installed copies add a `Laminar Dock` shortcut to the Startup folder (`shell:startup`), created only when missing and removed when the setting is off. Since 0.2.2; earlier versions used `HKCU\…\Run\Laminar Dock`, which 0.2.2 removes (see the Avira note below). Setting: **Settings → About → Start Laminar when you sign in**. |
| Update check | Installed copies check GitHub 20 s after starting, then every 6 hours, and download a newer version quietly. Setting: **Check for updates automatically**; **Check for updates** button on About, and **Check for Updates…** in the dock's right-click menu. |
| Update icon | A green download tile appears before the Recycle Bin. Click: **Restart and Update**, **What's New** (the release page), **Remind Me Tomorrow**, **Remind Me Next Week**, **Skip This Version**. Nothing installs until Restart and Update. |
| Installing an update | Laminar starts Velopack's updater, quits cleanly (Windows' taskbar comes back), the updater swaps the files and starts the new version. |
| Uninstall | Windows Settings → Apps. Laminar's uninstall step quits the running dock (restoring the taskbar) and removes the Startup shortcut. |
| Developer builds | A copy not installed by Setup.exe (like `target
elease`) shows "developer build", never checks for updates and never touches the Startup shortcut. |
| Releasing | Set `version` in `native/laminar/Cargo.toml`, commit, push a matching tag (`git tag v0.1.1`, `git push origin v0.1.1`). `.github/workflows/release.yml` builds, packages with `vpk` 1.2.161 (including a delta from the previous release) and publishes the GitHub Release. |
| Toolchain | Velopack needs Rust 1.88+, so `rust-toolchain.toml` pins Rust 1.99.1 for this repo (Dave's default toolchain is unchanged); the release builder uses the same. |

Checked on this PC (2026-11-11): builds with no warnings; `vpk pack` made Setup.exe (10.8 MB), the full package and the feed; the update icon draws in a test dock; the installed-update check against GitHub runs (`cargo run --release -p laminar --example update_feed -- 0.0.1`). Not installed here because an installed copy would share the one-dock lock with Dave's running dock, and uninstalling would close it. ✅ Install, update and uninstall need the test on Dave's other machine. *(Done 2026-11-16: the laptop installed from GitHub and updated in-app without problems.)* Unsigned for now, so Windows shows "Windows protected your PC" (More info → Run anyway); signing is a later decision.

First release (2026-11-12): [v0.1.0](https://github.com/davesousa/Better-Bar/releases/tag/v0.1.0), published by the workflow on its first run. ✅ The release has Setup.exe, the full package and `releases.win.json`; the update check run as an installed 0.0.1 is offered 0.1.0, and as 0.1.0 is told it's up to date.

Avira false alarm and the 0.2.2 fix (2026-11-15): Avira Security (Defender is off on Dave's PC) flagged the 0.2.0 `Setup.exe` as `Drop.Win64.RegStartupSelfCopy` at 20:46, five hours after Dave installed it and two seconds after the dock downloaded the 0.2.1 update. Its clean-up finished at 20:47:05, the same second `Update.exe` disappeared from `%LocalAppData%\LaminarDock`, which left the install on 0.2.0 with no updater. Evidence: `C:\ProgramData\Avira\Security\Logs\Elevated\Avira.Spotlight.Service_*.log` ("Malware detected event"), `%LocalAppData%elopackelopack_LaminarDock.log`. Likely cause (inference from the detection name): an unsigned app that copies itself into AppData and writes itself into `HKCU\…\Run`, which the dock also rewrote on every start. 0.2.2 uses a Startup-folder shortcut instead and only deletes the old `Run` value. *(Superseded 2026-11-16: Avira then flagged the Startup shortcut as Drop.Win64.Startup; since 0.2.6 the dock never creates it on its own, see below. Reporting false positives to Avira and signing remain open decisions.)*

### Widgets and the new Start button (2026-11-15)

Dave's request: rebuild the widgets after a reference gallery of ~110 widgets in 14 categories, add connectors to outside services, let widgets be reordered by dragging and added to the dock, and replace the Start icon with a calmer Windows-style mark that has a subtle hover animation.

| Task | How |
| --- | --- |
| Add a widget | Launchpad → **+ Add** (top of the left column). Pick a category, then **+ Launchpad** or **+ Dock**. Previews show your real data where no setup is needed, sample data otherwise. |
| Reorder | Launchpad: drag a widget card up or down. Dock: drag a widget along the dock, like an app. |
| Remove, move, connect | Right-click a widget card in Launchpad (Connect…, Move Up/Down, Add to Dock, Remove) or a widget in the dock (Connect…, Show in Launchpad, Remove from Dock). Dragging a dock widget up and off removes it. |
| Connect a service | **Connect…** on the widget, or click a widget that says it needs setup. Paste the key (Ctrl+V), Enter saves. **Where do I get this?** opens the service's key page. |
| Timer length | Right-click the Timer widget → 1–60 min. Pomodoro alternates 25-minute focus and 5-minute breaks; Stopwatch resets from its menu. |

| Part | How it works |
| --- | --- |
| Catalog | 106 widgets: AI (8), Productivity (9), Finance (13), System (22), Media & Audio (6), Analytics (4), Developer (4), Online & Mail (6), Social (4), Apps & Files (7), Weather (6), Stocks (3), Clocks (12), plus a Highlights tab. Ids are saved in config.json, so they're fixed. Service marks are original letter or glyph tiles in each service's colours, not their logos. |
| Drawing | Each widget is one Direct2D bitmap with a full Launchpad layout and a compact dock layout (one icon tall, not magnified). Clocks, timers and live stats redraw in place each second; others redraw when their data arrives. Light and dark themes both work. |
| Connectors | `laminar-sys/src/connectors/`: 30 web services and 9 local sources, all returning one shape (`feed.rs`). Web calls use WinHTTP (`http.rs`) on worker threads; each refreshes on its own schedule (2 s to 6 h). |
| Keys | API keys, tokens and private calendar links are stored in **Windows Credential Manager** (`LaminarDock:<service>:<field>`), never in config.json, and are only sent to their own service. Plain settings (usernames, site ids, symbols) go in config.json under `widget_settings`. |
| Settings | config.json: `launchpad_widgets` (left column, top to bottom) and dock pins `{ "widget": "clock" }`. The four old Launchpad widgets are now catalog widgets (Clock, Today's Forecast, Resource Monitor, Calendar) and are the default column. |
| Screen time | Counted by the dock (the app in front, only while you've used the mouse or keyboard in the last 2 minutes), saved to `%APPDATA%\Laminar\screentime.json`. Starts from zero on this version. |
| Start button | Four rounded panes on a quiet frosted tile (no colour gradient), drawn as vector shapes, so it stays crisp when magnified. It follows the light/dark theme. On hover the panes ease apart and shrink slightly, one after another (45 ms apart, 420 ms). Nothing moves if Windows' **Animation effects** is off. |

| Connector status | Services |
| --- | --- |
| ✅ Working live on this PC (2026-11-15) | Weather (Open-Meteo), stocks (Yahoo Finance), crypto (CoinGecko), GitHub (public contribution graph), LeetCode, Bing wallpaper, calendar (tested with a public Google iCal link), and all local sources: top CPU/memory, storage, Claude Code usage (from `~\.claude` logs; the cost is an estimate from list prices), keyboard language, uptime, microphone, monitor brightness (DDC/CI) |
| ⚠️ Built and unit-tested on sample responses; needs your keys to try for real | Anthropic and OpenAI admin cost APIs, OpenRouter (needs a management key), Cursor, Stripe, Lemon Squeezy, Paddle, Shopify, Gumroad, Polar, RevenueCat, Plausible, Umami, Search Console and Gmail (Google OAuth client + refresh token), Linear, Resend, Supabase, Vercel, X (paid API tier), YouTube, TikTok, Instagram |
| ⚠️ Not confirmed against current docs | OpenAI costs (docs page refused), Umami response shape, Supabase `interval` values, RevenueCat metric names, Lemon Squeezy sort order, Gumroad Bearer auth, Instagram host split |
| ⚠️ Local, no data here | Codex CLI (format written from documentation; this PC's only log is empty), Battery (this PC has none) |
| No public API, so the widget does something honest instead | WhatsApp (shows a dot when WhatsApp flashes for attention, like the dock does; no unread count), Focus/Do Not Disturb (opens Windows' settings) |

Checked (agent, 2026-11-15, debug build beside Dave's dock with a copied settings folder, so Dave's config.json was never written): builds with no warnings; 61 tests pass; Launchpad column with live data; all 14 gallery categories; the connect form; dragging a card from 5th to 2nd (lifted card follows the pointer, others make room, order saved); clicking the dock Pomodoro starts it; dragging a dock widget reorders the pins; Start hover; light theme. ⚠️ Not yet tried by hand: saving a real key through the form, typing and pasting in the form with a real keyboard, every widget's buttons, dock widgets on the 4K helper display, and the release build (Dave's running dock locks `target\release\laminar.exe`; quit it first, then `cargo build --release`).

### Widgets, round two (2026-11-15)

Dave's feedback on 0.2.0: drag widgets from the gallery to any place in Launchpad or the dock; resize widgets (icon-sized ones start small); a square analog clock; dragging felt laggy, with no lift until moving and no sign cards can be dragged; dock widgets looked broken when small and didn't magnify; a cleaner Start icon with a reflection or stroke on hover; icons off-centre in tiles; shadow settings; Control Center pieces as widgets placeable on the right too, with Background Apps, Windows Update and Windows Settings always on unless switched off; a Widgets page in Settings with one connection per service.

| Change | How it works |
| --- | --- |
| Two widget columns | Launchpad's right panel is now a widget column too. By default it holds what Control Center had, as widgets: Wi-Fi & Bluetooth, Now Playing, Quick Controls (dark mode, mute, snip, lock), Audio Output, Background Apps, Windows Update and Windows Settings. The Wi-Fi network list still opens in that panel. |
| Always-on widgets | Background Apps, Windows Update and Windows Settings come back on their own unless switched off in **Settings → Widgets**. They can be moved and resized, not removed from their menu, and don't go in the dock. |
| Drag anywhere | Drag a gallery preview, or any card, into either column: the cards part and a dashed outline shows where it will land. Drag it over the dock and the dock opens a gap at that spot (anywhere among the pinned items). Moving a card from a column to the dock moves it. |
| Pick up | Pressing a card dips it slightly. Holding still for 0.28 s, or moving, lifts it: a little larger with a deeper shadow. A grip (three dots) shows on the card under the pointer. Launchpad no longer rebuilds every second (stats, controls and update status redraw the affected cards in place), which was the cause of the stutter. Nothing redraws mid-drag. |
| Sizes | Right-click → Small, Medium or Large, in Launchpad and the dock. Small = half a column (two pair up side by side); Large = twice as tall, showing more (a chart, rows, the week, a month grid, a bigger ring). In the dock: one icon, the widget's own width, or wider. Icon-like widgets (analog clocks, day page, tiles) start small. Saved as `{ "id": "analog", "size": "small" }` in config.json; plain ids still work. |
| Layouts at every size | Every widget now has five layouts: one dock icon, the dock wider, small, medium and large. Compact versions drop or move parts instead of squeezing them (e.g. the dock clock puts the date beside the time when there's room). Text that doesn't fit shrinks to a minimum, then trims. |
| Dock magnification | Dock widgets now grow on hover like icons (half as much, since they're wider), counting the pointer as "on" a widget anywhere across its width. They're painted at their magnified size, so they stay sharp. |
| New widgets | Analog Square (the analog face in a rounded square, numerals when there's room) and the six Control Center widgets. |
| Start button | The Windows mark itself, nothing behind it: four panes with one blue gradient running across all of them, a faint highlight on each pane's top edge. On hover, a soft reflection sweeps once across the glass, top-left to bottom-right (0.85 s). It stays still if Windows' **Animation effects** is off. |
| Icon centring | Icons (Segoe Fluent glyphs) are centred on their drawn shape, not their text box, in widgets and across Launchpad. Tiles were redrawn: a rounded-square plate with the name beside or under it. Keep Awake uses a cup glyph instead of an emoji. |
| Shadows | **Settings → Glass → Panel shadow** (under the three panels) and **Widget shadow** (under every widget and tile). |
| Settings → Widgets | Switches for the always-on widgets; widget settings (weather location, looked up by name; world clock cities; revenue goal); and every connection, one row per service, with its status and the widgets that use it. One connection serves every widget for that service. |
| Testing aid | `laminar.exe --widget-sheet out.png [light] [filter]` paints every widget at every size into one image with sample data. Used to review all 109 widgets × 6 sizes. |

Checked (agent, 2026-11-15): builds with no warnings; 65 tests pass (4 new: small-card pairing, old and new config entries, always-on widgets, unique ids); every widget at every size reviewed on the widget sheet; the test dock starts and lays out widgets of each size. ⚠️ Not yet checked on screen (the PC locked mid-session): the two columns live, dragging from the gallery and between columns, dragging into the dock, press-and-hold lift, dock widget magnification, the Start reflection, and Settings → Widgets.

### Overnight test and polish run (2026-11-15/16, released in 0.2.3)

Dave asked for a full test and polish pass while away ("widgets still don't magnify correctly"). The PC locked at 22:23, so testing used a **test driver** instead of the real screen: the test copy runs with `LAMINAR_TESTCTL=1` (fake pointer via a `WM_APP+40` message, animations jump to their end, a plain colour behind the glass) and is captured with `PrintWindow`. The driver and helpers (`Drv.cs`, `drv.ps1`, `rt.ps1`, `restart.ps1`) were scratch files and aren't in the repo. Locked-screen captures show stale frames through the glass unless the backdrop is replaced, which is why test mode swaps it.

| Fixed | Cause and fix |
| --- | --- |
| Dock widgets didn't magnify correctly | Widget slots scaled from their top-left corner (no centre point set), so they grew down out of the shelf instead of rising. Now they scale from the bottom-left like icons; small widgets magnify as much as icons, wider ones 75%. The shelf-shift expression now counts widgets too, so clicks match what's drawn. |
| Widgets had no hover label | Dock widgets now show their name above them, centred on their width. |
| Dark band inside every glass panel and card | The drop shadow was a filled shape under translucent glass. Shadows are now hollow (the panel's own area is cut out), so they only fall outside. |
| Launchpad's bottom bar unclickable from the dock | While hovered, the dock's window claims the headroom above the shelf, which covers Launchpad's account / Settings / Power row. With Launchpad open, headroom that isn't on a magnified item is now see-through (`WM_NCHITTEST` → `HTTRANSPARENT`). |
| Changing the weather city didn't save | The lookup result was checked before finished jobs were collected, so it waited for an unrelated later update. Now collected first; the form says "Looking up…", closes when found, shows an error if not, and ignores answers for a cancelled or older lookup. |
| Local widget settings hard to reach | Right-click menus offer "Change Location…", "Choose Cities…" or "Set Goal…" on the widgets that use them. |
| Lifted card see-through | A dragged card gets an opaque backing while held. |
| Settings tab order | About is now last. |
| Widget review (two reviewers over all 109 × 6 renders) | Fixed the broken or wrong ones: Flip Clock (colon, darker flaps, hours over minutes when small), Now Playing Disc (record instead of a hole over a note), Day Timeline small (title overlapped the total), GitHub Overview dock (showed contributions labelled "Repositories"), Sunrise & Sunset dock (showed the weather), Clock & Weather (PM overlapped the icon; PM beside the time on small), small-card sparklines (crossed the caption; now beside the badge), dock captions ("Last 30 da…" → "30 days"), one medium dock width, tiny ring numbers dropped, list widgets' dock figure matches its label, Quick Controls large (2×2, not tall bars), "today" red everywhere, weekly highs/lows told apart, uptime/battery as "2d 4h" / "4h 10m left", storage rounding, RevenueCat subscribers on the caption line, Mail large duplicate subject, Notes empty state, "Up next" casing, timer ring padding, light-theme emoji (faint silhouette so white clouds show). |

Checked (agent, overnight): builds with no warnings; 66 tests pass (new: Startup shortcut); on the locked screen through the driver: magnification sweep across every dock item, widget hover labels, Launchpad open, gallery, press-hold lift, drag to reorder, gallery → dock drop into the gap (saved), right-click menus in Launchpad and dock, resize in the dock, Change Location (typed, saved, forecast switched to Ottawa), Settings (Glass sliders, Widgets page), Widget shadow slider at 100% and back, Settings gear reached from the dock, light theme; an independent code review of the night's diff (its two bugs — hover flicker in the see-through headroom, and a stale weather lookup — fixed).

⚠️ Still needs a real screen: animations (the Start reflection, hover fades, lift) can't run on a locked screen; the frosted glass itself; the see-through headroom with a real mouse (the driver can't reproduce Windows' leave tracking); the reviewers' remaining minor items (several Large cards leave empty space at the bottom, e.g. social counts, storage, RevenueCat; small-card truncation on Linear, Connectivity, Screenshot; dock S legibility of month/week).

### Updates in place in Settings, and Launchpad closing reliably (2026-11-16, released in 0.2.8)

| Problem (Dave) | Fix |
| --- | --- |
| Settings → About → **Check for updates** opened its result as a menu wherever the pointer was | The result shows in place. While checking, the button reads **Checking…**. When a version is ready: an accent **Restart and Update** button, **What's New**, and links for Remind me tomorrow · Remind me next week · Skip this version · Check again. The status line says when a reminder or skip is set. The dock's update icon keeps its menu (it opens beside the icon). |
| Clicking something in the dock while Launchpad was open switched apps behind it, but Launchpad stayed | Launchpad closed only when its own window lost focus, and Windows doesn't always give it focus (and clicking the dock never takes it). Now clicking an app, the Recycle Bin or a stack in the dock closes Launchpad first, and Launchpad closes whenever another program's window comes to the front (the dock's existing foreground-change hook). |

Checked: builds, 94 tests pass. On screen (test copy; `LAMINAR_DEMO_UPDATE=<version>|checking|idle` fakes the update state because test copies aren't installed): the ready state's buttons and links, and the Checking… button. ⚠️ Not checked: the Launchpad closing fix (the test driver turns off focus rules), and clicking the update buttons in an installed copy. Built in a separate worktree because another session's unfinished widget-style work didn't compile in the shared checkout.
### Update checks: daily by default, with a frequency choice (2026-11-16, released in 0.2.7)

A user on X pointed out that checking GitHub "every few hours" is unkind to metered connections. Dave replied that the default would become once a day, with a frequency option and an on/off switch.

| Before | Now |
| --- | --- |
| Checked 20 s after every start, then every 6 hours | Settings → About → **How often to check**: At startup, **Daily** (default) or Weekly. The time of the last automatic check is saved (`last_update_check`), so restarting Laminar doesn't check again. |
| On/off switch | Unchanged (**Check for updates automatically**); the **Check for updates** button works any time |
| A version downloaded earlier showed its dock icon after the next check | Shown at start without going online (`updater::show_pending`) |

Checked: builds, all tests pass (new: `update_checks_follow_the_chosen_frequency` covers daily, weekly, at startup, off, and a clock moved back). On screen in a test copy: the three-way control shows under the switch with Daily selected; clicking Weekly highlights it and saves `update_check_hours: 168`. ⚠️ Not checked in an installed copy (the test copy isn't installed, so the timer code is skipped). Possible follow-up: skip automatic checks on metered connections.
### Start at sign-in only when the user turns it on (2026-11-16, released in 0.2.6)

Avira's behaviour check flagged the installed `laminar.exe` as `Drop.Win64.Startup.3410` (2:46 PM, first launch after a reinstall): on launch the dock had created its own Startup shortcut, and an unsigned program adding itself to Startup on first run is what malware droppers do. The dock never creates that shortcut on its own now.

| Situation | Before | Now |
| --- | --- | --- |
| New install | Shortcut created on first launch | Off; Settings → Behavior → "Start Laminar when you sign in" creates it |
| Reinstall (setting says on, shortcut gone) | Shortcut recreated silently | Setting switches to off to match; turn it on again in Settings |
| Existing install with the shortcut | Kept | Kept |

Checked: builds, 93 tests pass, release build succeeded. ✅ Dave's desktop updated 0.2.5 → 0.2.6 in-app at 15:42 with no "file in use" retries (Slack started a minute later, so the apps-open case is still unproven), its existing shortcut and setting kept, and no Avira detection. ⚠️ Not yet seen: a fresh install starting with the switch off, and the reinstall case. Avira may still flag when the switch is turned on; code signing (Azure Artifact Signing, US$9.99/month, individuals in the US or Canada) is the longer-term fix.
### Update and panel-shadow fixes (2026-11-16, released in 0.2.5)

| Problem | Cause (evidence) | Fix |
| --- | --- | --- |
| In-dock update closes, "installs", and reopens on the old version (desktop only; the laptop updated fine) | Velopack backs up by renaming `LaminarDock\current`. Apps opened from the dock inherited the dock's working folder, which was that folder (Velopack starts the dock there). Windows won't rename a folder some process is "in". Velopack log: 10 s of "file in use" (os error 32), then "running processes prevented it". Seven Slack processes had `...\LaminarDock\current\` as their working folder. | The dock switches its working folder to the user folder (`%USERPROFILE%`) at startup, so nothing it opens holds the install folder. Checked: a test copy started from its own folder reports `C:\Users\davej\`. |
| The dock kept offering 0.2.3 after 0.2.4 shipped | Once a version was downloaded, the dock stopped asking GitHub. | It always asks GitHub. An older download is used only when it's the latest, or when offline. |
| Panel shadow slider seems to do nothing | Launchpad's panels are drawn when it opens. Settings opens inside Launchpad, so their shadows never redrew while dragging. | The panel shadows redraw whenever the setting changes. Checked on screen: dragging 0% to 100% shows the shadow appear live. |

To get past an install folder that's already held (any version before this fix), quit apps opened from the dock first (Slack fully, from its tray icon), then update.
### Smarter search (2026-11-16, released in 0.2.4)

Dave: "if I search for a particular windows setting it doesn't appear … there is no smart context aware search". Search only knew 19 hand-picked Settings pages, matched by name. Phases 1 and 2 of the plan (closer to Spotlight / Raycast):

| Try typing | You get |
| --- | --- |
| `night light`, `vpn`, `mouse speed`, `resolution`, `startup apps`, `proxy` | The exact Windows setting (about 1,350 Settings entries, with Windows' own synonyms: `blue light` finds Night light) |
| `environment variables`, `device manager`, `power plan` | Classic Control Panel tasks (about 140) |
| `12*7+3`, `15% of 80`, `sqrt(2)`, `200 + 10%` | Calculator answer; Enter copies it |
| `5 km in miles`, `70 f to c`, `2.5 GB in MB`, `10 kg` | Unit conversion (length, mass, volume, speed, data, time, area, temperature); Enter copies |
| `time in tokyo`, `london time`, `time` | The time there, the day, and how far ahead or behind |
| `weather` | Current conditions from the weather widget's location |
| `bluetooth`, `wifi`, `dark mode`, `mute`, `volume 30`, `lock`, `screenshot`, `empty trash`, `sleep`, `restart` | Commands; toggles show On/Off and stay open so you see the change; Restart, Shut Down and Sign Out need a second Enter |
| part of a window's title (`budget`) | Switch to that open window |
| `g rust`, `yt lofi`, `gh laminar`, `w toronto`, `maps cafe`, `ddg …`, `amazon …` | Search that site; `github.com` opens the address |

How it works:

| Part | Where | Notes |
| --- | --- | --- |
| Settings index | `laminar-sys/src/settings_index.rs` | Reads `C:\Windows\ImmersiveControlPanel\Settings\AllSystemSettings_*.xml` (what Windows' own search uses). Names and synonyms are resource references resolved with `SHLoadIndirectString` (your display language). Each Settings page's `ms-settings:` link comes from the same files (`<PolicyIds>`), so links are Windows' own, not guessed; a short fallback table covers 8 pages and the category pages. Control Panel entries launch their `<DeepLink>` (`control.exe /name …`, or the program); entries whose program or COM class is missing are skipped. Built on a background thread at start (1.8 s with COM up; 8.6 s without, so the thread initialises COM) and cached in `%LOCALAPPDATA%\Laminar\settings-index.json`, rebuilt when Windows updates the files or the language changes. |
| Ranking | `laminar-sys/src/search.rs`, `laminar/src/launchpad_search.rs` | One list across apps, commands, windows, settings and files. Settings need a real match (name contains the words, or a synonym starts with them; no loose letter matches). Typing exactly a command's word makes it the Top Hit ("bluetooth" → the toggle, settings right below). A short `INTENTS` table fixes words where Windows' synonyms point elsewhere first ("uninstall" → Installed apps, not "Uninstall updates"). Sections are ordered by their best result; at most two rows per Settings page. |
| Learning | `laminar-sys/src/history.rs` | Each pick is stored as (what you typed, which result). Typing the same or the start of it again lifts that result (picked VPN after `vpn` → `v` shows VPN first). Older picks count less (half after 30 days); 400 kept. Saved in `%APPDATA%\Laminar\search-history.json`; the `Clear Search History` command forgets it. |
| Answers | `laminar-sys/src/calc.rs`, `worldtime.rs` | Calculator and converter only answer when the whole query is a calculation (a lone number or a word never is). City times use Windows' time-zone list (city names in its display names) plus ~70 aliases for regions Windows names by zone ("Eastern Time"). |

Checked (agent): 93 tests pass. They include 31 real searches run against this PC's installed apps and Windows index, asserting the Top Hit for each (the table above), and a learning test ("v" → VPN after one pick). Typical search time is 1–2 ms. The first time search is used it takes about 70 ms while it reads the time zones, and those are now loaded at start-up. Release build has no warnings.

An independent review of the search code found six problems, all now fixed:

| Problem | Fix |
| --- | --- |
| Holding Enter, or double-clicking a row, could confirm Restart / Shut Down / Sign Out (auto-repeat and the second click counted as the second press) | Enter auto-repeat is ignored. A confirm has to come at least a double-click interval after arming, and only for the armed command. |
| Sleep ran on one press, and learning could make "s" + Enter sleep the PC | Sleep asks again too, and search never learns any of these four commands. |
| An instant answer could hide an app the query names ("google chrome" searched Google; "clock" copied the time) | An app or window whose name starts with the whole query is the Top Hit; the answer moves to second. |
| Arming changed the row's score, so the selection could move | The row keeps its score, and the selection follows it. |
| The clipboard could reject the copy and leak the memory | The copy now names the dock's window as the clipboard's owner, and frees the memory if it fails. |
| "3x4" didn't calculate; "wifi" missed "Wi-Fi" names in the quick filter | Both fixed. Each setting's lower-case search text is now built once, not on every keystroke. |

Checked on screen (agent, 2026-11-16, test copy with its own settings): `night light`, `bluetooth` (On pill, state only, not toggled), `12*7+3`, `google chrome`, `time in tokyo`, `5 km in miles`, `vpn` and `g rust traits` all showed the expected Top Hit or answer, the section headers, and the Enter hint. That check found two problems, both fixed and rechecked: a sum also listed unrelated files (Windows reads `*` as a wildcard), so sums no longer show files; and a command waiting for its second Enter now says "Confirm" instead of "Run".

⚠️ Rule for testing: never type or arm power commands (restart, shut down, sign out, sleep) in the test copy. It runs them for real, so one stray Enter reboots the PC. Check the confirm step with unit tests only.

Not checked at all:
- That every `ms-settings:` link opens the right page. They come from Windows' own files, and opening each one would flood the screen.
- Windows Security entries. They need the Security app's package resources, which didn't resolve, so they're absent.
- Currency conversion. It needs exchange rates from the internet and wasn't built.

### Known gaps

- Tray icons (the apps normally beside the clock) aren't hosted yet, and there's no Windows Update status. *(Done 2026-11-10.)*
- Blur radius and wallpaper refraction aren't adjustable in the native app yet; there's no light theme. *(Light theme and extra blur done 2026-11-10; refraction isn't possible, see below.)*
- Pinning a running app by exe path breaks for apps whose folder changes on update (for example Discord). *(Fixed 2026-11-10.)*

## Phase 0 status (native spike, 2026-11-07, historical)

Code: [`native/laminar/`](../native/laminar/), a Cargo workspace member. Build with `cargo build --release`, then run `target\release\laminar.exe`. Right-click the dock and choose **Quit Laminar** to exit.

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
- "Edit Controls" customization of the right panel. *(Widget gallery done 2026-11-15.)*
- Reordering pinned apps inside Launchpad by dragging.
- Focus / Do Not Disturb has no public Windows API; needs investigation.

### Showcase README and media (2026-11-16)

Dave asked for a showcase README with screenshots and animation that "sells" the free, open-source app, states the performance, and asks beta users for feedback and pull requests. Decisions (Dave, 2026-11-16): MIT licence (`LICENSE`); this development log moved from `README.md` to `docs/DEVELOPMENT.md` (add new sections here, not to the README); blur the media title, artist and art, and the Windows username, in screenshots.

| Part | How it was made |
| --- | --- |
| Numbers | `laminar.exe --measure` (0.2.5 release build), three runs: idle CPU 0.000% of one core every run; Task Manager memory 17.0–17.8 MB; start-up 70–87 ms; sweep CPU 0.9–2.5%. The installed dock in daily use, about 45 min after start: main process 26.3 MB private working set, helper dock on the second display 10.8 MB, watchdog 1.2 MB. |
| Clean demo data | Test docks ran with `APPDATA` pointed at a scratch folder holding a hand-written `config.json` (common Windows apps, a "Creative" stack, a clock widget), so none of Dave's pins or settings appear. |
| Clean background | A borderless, non-activating Windows PowerShell window showed a background image over the main display while the test docks ran. Since 2026-11-16 (re-captured on 0.2.8) it is Dave's chosen [Gradient Lab](https://backgrounds.supply/gradient-lab) "Aura" gradient, exported as a 1920×1080 PNG (free for commercial use, no attribution needed) (`--measure`, `--demo launchpad`, `lp-gallery`, `lp-settings`, `lp-search`, `stack`, `menu`, `--demo-hover`, with and without `LAMINAR_THEME=light`). |
| Capture | `ffmpeg -f gdigrab` of the main display at 30 fps. The magnification GIFs are exactly one 2.5 s sweep period, so they loop seamlessly. GIFs use `palettegen stats_mode=diff` and `paletteuse diff_mode=rectangle` (Launchpad GIF went from 15.9 MB to 0.9 MB). |
| Privacy | Media card art, title and artist are box-blurred from the frame the media loads (frame 40); the username is blurred; the search image is cropped above the Files results (they showed local paths). |
| Animated SVGs | `docs/media/banner.svg` (drifting gradient, icon wave) and `stats.svg` use CSS animations that GitHub plays inside `<img>`; both stop under reduced motion. Icon wave checked in a browser: neighbouring icons never overlap (worst 0.6 px at the rounded corners). |

Found while capturing (not fixed): a running app that isn't pinned can show its file name as the hover label (`slack.exe` instead of Slack).

⚠️ Not checked on github.com itself until pushed: rendering was previewed locally with GitHub's stylesheet, marked and Mermaid. Badges were live (v0.2.5, download count).
