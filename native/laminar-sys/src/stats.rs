//! System stats for the dock's monitor widget.
//!
//! CPU: `GetSystemTimes` (idle/kernel/user) deltas between samples (kernel time includes
//! idle). Memory: `GlobalMemoryStatusEx`. Disk: `GetDiskFreeSpaceExW("C:\")`. Network:
//! `GetIfTable2` octet counters summed over interfaces that are up, not loopback, and not
//! NDIS filter/virtual-switch layers (those mirror a physical NIC's counters and would
//! double-count), with deltas between samples. All calls are cheap (<2 ms); no COM.
//! The first sample after `new()` measures from construction time.

use serde::{Deserialize, Serialize};
use std::time::Instant;
use windows::Win32::Foundation::{FILETIME, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_ROW2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

/// One stats sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// 0..100, averaged over all cores since the previous sample.
    pub cpu_percent: f32,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    /// 0..100 used space on C:.
    pub disk_c_used_percent: f32,
    /// Bytes per second received since the previous sample.
    pub net_down_bps: f64,
    /// Bytes per second sent since the previous sample.
    pub net_up_bps: f64,
}

/// Holds the previous counters so each `sample()` reports rates since the last call.
pub struct Sampler {
    last_cpu: Option<(u64, u64)>, // (idle, total)
    last_net: Option<(u64, u64, Instant)>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn cpu_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    Some((ft(idle), ft(kernel) + ft(user)))
}

fn net_octets() -> Option<(u64, u64)> {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table) != NO_ERROR || table.is_null() {
            return None;
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr() as *const MIB_IF_ROW2, n);
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in rows {
            let flags = r.InterfaceAndOperStatusFlags._bitfield;
            let filter = flags & 0x2 != 0; // FilterInterface
            let connector = flags & 0x4 != 0; // ConnectorPresent (real NIC)
            if r.OperStatus != IfOperStatusUp || r.Type == IF_TYPE_SOFTWARE_LOOPBACK || filter || !connector {
                continue;
            }
            rx += r.InOctets;
            tx += r.OutOctets;
        }
        FreeMibTable(table as *const _);
        Some((rx, tx))
    }
}

impl Sampler {
    /// Captures baseline counters.
    pub fn new() -> Self {
        Sampler { last_cpu: cpu_times(), last_net: net_octets().map(|(r, t)| (r, t, Instant::now())) }
    }

    /// Takes a sample; rates are relative to the previous `sample()`/`new()`.
    pub fn sample(&mut self) -> Stats {
        let mut s = Stats::default();

        if let Some((idle, total)) = cpu_times() {
            if let Some((li, lt)) = self.last_cpu {
                let (di, dt) = (idle.saturating_sub(li), total.saturating_sub(lt));
                if dt > 0 {
                    s.cpu_percent = ((1.0 - di as f64 / dt as f64) * 100.0).clamp(0.0, 100.0) as f32;
                }
            }
            self.last_cpu = Some((idle, total));
        }

        let mut ms = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if unsafe { GlobalMemoryStatusEx(&mut ms) }.is_ok() {
            const GB: f64 = 1024.0 * 1024.0 * 1024.0;
            s.mem_total_gb = (ms.ullTotalPhys as f64 / GB) as f32;
            s.mem_used_gb = ((ms.ullTotalPhys - ms.ullAvailPhys) as f64 / GB) as f32;
        }

        let (mut total, mut free) = (0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(windows::core::w!("C:\\"), None, Some(&mut total), Some(&mut free)) }.is_ok() && total > 0 {
            s.disk_c_used_percent = ((total - free) as f64 / total as f64 * 100.0) as f32;
        }

        if let Some((rx, tx)) = net_octets() {
            let now = Instant::now();
            if let Some((lr, lt, at)) = self.last_net {
                let secs = now.duration_since(at).as_secs_f64();
                if secs > 0.0 {
                    s.net_down_bps = rx.saturating_sub(lr) as f64 / secs;
                    s.net_up_bps = tx.saturating_sub(lt) as f64 / secs;
                }
            }
            self.last_net = Some((rx, tx, now));
        }
        s
    }
}
