//! Monitor brightness over DDC/CI (the "high-level monitor configuration" API in dxva2).
//!
//! `GetPhysicalMonitorsFromHMONITOR` turns a display (HMONITOR) into its physical
//! monitors; `GetMonitorBrightness`/`SetMonitorBrightness` then talk to each monitor over
//! the video cable. Each call is a round trip to the monitor's controller and can take
//! 50–200 ms, so call these from a worker thread. Many monitors (and TVs, docks and most
//! KVMs) don't implement DDC/CI, or have it switched off in their on-screen menu; laptop
//! panels are controlled by a different mechanism (WMI) that this module doesn't use.
//! In those cases the functions return `None`/`false`. No COM needed.

use windows::core::BOOL;
use windows::Win32::Devices::Display::{DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR};
use windows::Win32::Foundation::{HANDLE, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, MonitorFromPoint, HDC, HMONITOR, MONITOR_DEFAULTTOPRIMARY};

/// Physical monitors of one display; released on drop.
struct Physical(Vec<PHYSICAL_MONITOR>);

impl Physical {
    fn of(hmon: HMONITOR) -> Option<Physical> {
        let mut n = 0u32;
        unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut n) }.ok()?;
        if n == 0 {
            return None;
        }
        let mut v = vec![PHYSICAL_MONITOR::default(); n as usize];
        unsafe { GetPhysicalMonitorsFromHMONITOR(hmon, &mut v) }.ok()?;
        Some(Physical(v))
    }
}

impl Drop for Physical {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyPhysicalMonitors(&self.0);
        }
    }
}

/// (min, current, max) brightness as the monitor reports it (often 0..100).
fn read(h: HANDLE) -> Option<(u32, u32, u32)> {
    let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
    let ok = unsafe { GetMonitorBrightness(h, &mut min, &mut cur, &mut max) } != 0;
    (ok && max > min).then_some((min, cur.clamp(min, max), max))
}

/// Brightness of the primary display's first DDC/CI-capable monitor, 0..1.
pub fn brightness() -> Option<f32> {
    let hmon = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mons = Physical::of(hmon)?;
    mons.0.iter().find_map(|m| read(m.hPhysicalMonitor)).map(|(min, cur, max)| (cur - min) as f32 / (max - min) as f32)
}

unsafe extern "system" fn collect(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let v = &mut *(data.0 as *mut Vec<HMONITOR>);
    v.push(hmon);
    BOOL(1)
}

/// Sets every DDC/CI-capable monitor (on all displays) to `v` (0..1, mapped onto each
/// monitor's own range). Returns true if at least one monitor accepted it.
pub fn set_brightness(v: f32) -> bool {
    let v = v.clamp(0.0, 1.0);
    let mut hmons: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut hmons as *mut _ as isize));
    }
    let mut any = false;
    for hmon in hmons {
        let Some(mons) = Physical::of(hmon) else { continue };
        for m in &mons.0 {
            if let Some((min, _, max)) = read(m.hPhysicalMonitor) {
                let target = min + ((max - min) as f32 * v).round() as u32;
                any |= unsafe { SetMonitorBrightness(m.hPhysicalMonitor, target) } != 0;
            }
        }
    }
    any
}
