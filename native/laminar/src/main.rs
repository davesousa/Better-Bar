//! Laminar Dock — Phase 0 native spike.
//!
//! Proves the native stack: a Win32 tool window whose content is drawn by the
//! Windows Composition engine (the same GPU compositor the Windows shell uses).
//! Run with `--measure` to print startup time, memory and CPU usage.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod dock;
mod gfx;
mod metrics;

use windows::core::Result;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
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
}

fn parse_options() -> Options {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--measure" => o.measure = true,
            "--demo-hover" => o.demo_hover = true,
            "--exit-after" => o.exit_after = args.next().and_then(|v| v.parse().ok()),
            _ => {}
        }
    }
    o
}

fn main() -> Result<()> {
    let started = std::time::Instant::now();
    let opts = parse_options();
    unsafe {
        // Release builds have no console; reuse the parent's so --measure can print.
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
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
