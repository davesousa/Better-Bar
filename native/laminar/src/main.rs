//! Laminar Dock — native dock for Windows (Rust + Windows Composition).
//!
//! A Win32 tool window whose content is drawn by the Windows Composition
//! engine (the same GPU compositor the Windows shell uses).
//! Run with `--measure` to print startup time, memory and CPU usage.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod config;
mod displays;
mod dock;
mod effects;
mod filedrop;
mod gfx;
mod iconcache;
mod launchpad;
mod menu;
mod metrics;
mod preview;
mod services;
mod stack;
mod taskbar;
mod tracker;
mod tray;
mod ui;
mod winkey;

use windows::core::Result;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DispatcherQueueOptions, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};

/// Command-line options.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Measure startup, memory and CPU, print JSON, then exit.
    pub measure: bool,
    /// Hold the dock in its magnified state (for screenshots).
    pub demo_hover: bool,
    /// Quit automatically after this many seconds.
    pub exit_after: Option<u32>,
    /// Open a surface automatically for visual testing: menu, preview, stack, launchpad, settings.
    pub demo: Option<String>,
    /// Helper dock for an extra display (index into displays::all()).
    pub monitor: Option<usize>,
    /// The main dock's process id (helpers quit when it exits).
    pub parent: Option<u32>,
}

fn main() -> Result<()> {
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--watchdog <pid> [taskbar-state]`: tiny helper that restores the Windows taskbar
    // when the dock process exits, however it exits.
    if args.first().map(|a| a == "--watchdog").unwrap_or(false) {
        if let Some(pid) = args.get(1).and_then(|p| p.parse().ok()) {
            taskbar::run_watchdog(pid, args.get(2).and_then(|s| s.parse().ok()));
        }
        return Ok(());
    }

    // `--list-windows`: print what the dock sees (app identity, exe, title) for troubleshooting.
    if args.first().map(|a| a == "--list-windows").unwrap_or(false) {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        for w in tracker::Tracker::default().scan() {
            println!("{:<70} | {:<60} | {}", w.app, w.exe, w.title);
        }
        return Ok(());
    }

    // `--test-open-with <app id> <file>`: diagnostics for opening files with a dock app.
    if args.first().map(|a| a == "--test-open-with").unwrap_or(false) {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
        if let (Some(app), Some(file)) = (args.get(1), args.get(2)) {
            filedrop::open_with(app, None, &[file.clone()]);
            std::thread::sleep(std::time::Duration::from_secs(4));
        }
        return Ok(());
    }

    let mut opts = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--measure" => opts.measure = true,
            "--demo-hover" => opts.demo_hover = true,
            "--exit-after" => opts.exit_after = it.next().and_then(|v| v.parse().ok()),
            "--demo" => opts.demo = it.next().cloned(),
            "--monitor" => opts.monitor = it.next().and_then(|v| v.parse().ok()),
            "--parent" => opts.parent = it.next().and_then(|v| v.parse().ok()),
            _ => {}
        }
    }

    unsafe {
        // Release builds have no console; reuse the parent's so --measure can print.
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        // One real dock at a time (test modes may run alongside).
        if !(opts.measure || opts.demo_hover || opts.demo.is_some()) {
            let name = match opts.monitor {
                Some(i) => format!("Local\\LaminarDock-{i}"),
                None => "Local\\LaminarDock".to_string(),
            };
            let _mutex = CreateMutexW(None, false, &windows::core::HSTRING::from(name));
            if GetLastError() == ERROR_ALREADY_EXISTS {
                return Ok(());
            }
            std::mem::forget(_mutex);
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // OLE (COM single-threaded apartment + drag and drop support).
        windows::Win32::System::Ole::OleInitialize(None)?;
    }
    // Windows.UI.Composition needs a DispatcherQueue on the UI thread.
    let _queue = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?
    };
    dock::run(opts, started)
}
