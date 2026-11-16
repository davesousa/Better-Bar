//! Local Windows sources for widgets. Nothing here touches the network.
//!
//! | Connector      | Source                                                                  |
//! |----------------|-------------------------------------------------------------------------|
//! | `top_cpu`      | Toolhelp process list + `GetProcessTimes` deltas between calls          |
//! | `top_memory`   | Toolhelp process list + `GetProcessMemoryInfo` (private working set)    |
//! | `battery`      | `GetSystemPowerStatus`                                                  |
//! | `storage`      | `GetLogicalDrives` + `GetDriveTypeW` + `GetDiskFreeSpaceExW`            |
//! | `claude_code`  | Claude Code transcripts (`~/.claude/projects/**/*.jsonl`)               |
//! | `codex`        | Codex CLI rollouts (`~/.codex/sessions/**/*.jsonl`)                     |
//! | `recent_files` | `%APPDATA%\Microsoft\Windows\Recent\*.lnk` resolved with `IShellLinkW`  |
//! | `language`     | Foreground thread's keyboard layout → locale names                      |
//! | `uptime`       | `GetTickCount64`                                                        |
//!
//! The two AI-log readers only extract token counts, model ids and timestamps; message
//! text is never kept. Parsed results are cached per file (path, size, mtime) and files
//! that only grew are read from where the last read stopped, so repeat calls are cheap.

use crate::feed::*;
use crate::util::{file_description, from_wide, process_image_path, wide};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, SYSTEMTIME};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

// ---------------------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------------------

/// Bytes as Explorer shows them (binary units): "640 MB", "1.2 GB", "123 GB", "1.8 TB".
fn fmt_bytes(b: u64) -> String {
    let mb = b as f64 / (1024.0 * 1024.0);
    let gb = b as f64 / GIB;
    let tb = gb / 1024.0;
    if gb < 1.0 {
        format!("{:.0} MB", mb)
    } else if gb < 10.0 {
        format!("{:.1} GB", gb)
    } else if tb < 1.0 {
        format!("{:.0} GB", gb)
    } else {
        trim_zero(format!("{:.1} TB", tb))
    }
}

/// "1.0 TB" → "1 TB".
fn trim_zero(s: String) -> String {
    s.replace(".0 ", " ")
}

/// Token counts: "980", "45.3K", "1.2M", "3.4B".
fn fmt_tokens(n: u64) -> String {
    let f = n as f64;
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}K", f / 1e3)
    } else if n < 1_000_000_000 {
        format!("{:.1}M", f / 1e6)
    } else {
        format!("{:.1}B", f / 1e9)
    }
}

/// The two largest units of a duration: "3d 4h", "3h 20m", "12m", "0m".
fn fmt_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3_600, secs % 3_600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

// ---------------------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------------------

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// (pid, exe file name) of every process (Toolhelp snapshot).
fn processes() -> Vec<(u32, String)> {
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut pe).is_ok();
        while ok {
            if pe.th32ProcessID != 0 {
                out.push((pe.th32ProcessID, from_wide(&pe.szExeFile)));
            }
            ok = Process32NextW(snap, &mut pe).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Opens a process for limited queries; closes on drop.
struct Proc(HANDLE);

impl Proc {
    fn open(pid: u32) -> Option<Proc> {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok().map(Proc)
    }

    /// (creation time, kernel + user time), both in 100 ns units.
    fn times(&self) -> Option<(u64, u64)> {
        use windows::Win32::System::Threading::GetProcessTimes;
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetProcessTimes(self.0, &mut c, &mut e, &mut k, &mut u) }.ok()?;
        Some((ft(c), ft(k) + ft(u)))
    }

    /// Private working set in bytes (memory only this process uses, as Task Manager's
    /// "Memory" column shows).
    fn private_ws(&self) -> Option<u64> {
        use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
        let mut pmc = PROCESS_MEMORY_COUNTERS_EX2 { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32, ..Default::default() };
        unsafe { GetProcessMemoryInfo(self.0, &mut pmc as *mut _ as *mut PROCESS_MEMORY_COUNTERS, pmc.cb) }.ok()?;
        Some(pmc.PrivateWorkingSetSize as u64)
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Friendly app name for an exe: its FileDescription ("Google Chrome"), else the exe
/// stem ("chrome"). Descriptions are cached by path (reading version info costs ~1 ms).
fn friendly_name(pid: u32, exe: &str) -> String {
    static CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> = LazyLock::new(Default::default);
    let stem = exe_stem(exe);
    let Some(path) = process_image_path(pid) else { return stem };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let desc = cache.entry(path.to_lowercase()).or_insert_with(|| file_description(&path)).clone();
    desc.unwrap_or(stem)
}

fn exe_stem(exe: &str) -> String {
    let lower = exe.to_ascii_lowercase();
    if lower.ends_with(".exe") {
        exe[..exe.len() - 4].to_string()
    } else {
        exe.to_string()
    }
}

/// One app's total, summed over all its processes (grouped by exe name).
struct Group {
    exe: String,
    pids: Vec<u32>,
    amount: f64,
}

/// Groups `(pid, exe, amount)` by exe name (case-insensitive), largest first.
fn group_by_exe(items: impl IntoIterator<Item = (u32, String, f64)>) -> Vec<Group> {
    let mut map: HashMap<String, Group> = HashMap::new();
    for (pid, exe, amount) in items {
        let g = map.entry(exe.to_lowercase()).or_insert_with(|| Group { exe: exe.clone(), pids: Vec::new(), amount: 0.0 });
        g.pids.push(pid);
        g.amount += amount;
    }
    let mut v: Vec<Group> = map.into_values().collect();
    v.sort_by(|a, b| b.amount.total_cmp(&a.amount));
    v
}

fn group_label(g: &Group) -> String {
    // The first pid whose image path we can read gives the description.
    for &pid in g.pids.iter().take(4) {
        if process_image_path(pid).is_some() {
            return friendly_name(pid, &g.exe);
        }
    }
    exe_stem(&g.exe)
}

/// CPU counters at one moment.
struct CpuSnap {
    at: Instant,
    /// System (idle, kernel + user) from `GetSystemTimes`.
    sys: Option<(u64, u64)>,
    /// (pid, creation time) → kernel + user time; the creation time guards against pid reuse.
    procs: HashMap<(u32, u64), u64>,
    names: HashMap<u32, String>,
}

fn system_times() -> Option<(u64, u64)> {
    use windows::Win32::System::Threading::GetSystemTimes;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    Some((ft(idle), ft(kernel) + ft(user)))
}

fn cpu_snapshot() -> CpuSnap {
    let mut procs = HashMap::new();
    let mut names = HashMap::new();
    for (pid, exe) in processes() {
        if let Some((created, cpu)) = Proc::open(pid).and_then(|p| p.times()) {
            procs.insert((pid, created), cpu);
            names.insert(pid, exe);
        }
    }
    CpuSnap { at: Instant::now(), sys: system_times(), procs, names }
}

/// Top 5 apps by CPU since the previous call (the first call samples twice, 250 ms apart).
/// Percentages are of the whole machine (all logical processors = 100%).
pub fn top_cpu() -> Result<Feed, FeedError> {
    static PREV: Mutex<Option<CpuSnap>> = Mutex::new(None);
    let mut prev_slot = PREV.lock().unwrap_or_else(|e| e.into_inner());
    let prev = match prev_slot.take() {
        Some(p) if p.at.elapsed() < Duration::from_secs(120) => p,
        _ => {
            let p = cpu_snapshot();
            std::thread::sleep(Duration::from_millis(250));
            p
        }
    };
    let cur = cpu_snapshot();
    let wall = cur.at.duration_since(prev.at).as_nanos() as f64 / 100.0; // 100 ns units
    let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    let capacity = (wall * ncpu).max(1.0);

    let items = cur.procs.iter().filter_map(|(&(pid, created), &cpu)| {
        let before = *prev.procs.get(&(pid, created))?;
        let pct = cpu.saturating_sub(before) as f64 / capacity * 100.0;
        Some((pid, cur.names.get(&pid)?.clone(), pct))
    });
    let groups = group_by_exe(items);

    let total = match (prev.sys, cur.sys) {
        (Some((li, lt)), Some((i, t))) if t > lt => (1.0 - i.saturating_sub(li) as f64 / (t - lt) as f64) * 100.0,
        _ => groups.iter().map(|g| g.amount).sum(),
    }
    .clamp(0.0, 100.0);

    let rows = groups
        .iter()
        .take(5)
        .map(|g| {
            let pct = g.amount.clamp(0.0, 100.0);
            FeedRow { label: group_label(g), value: format!("{pct:.1}%"), frac: Some((pct / 100.0) as f32), delta: None }
        })
        .collect();
    *prev_slot = Some(cur);
    Ok(Feed { value: Some(round1(total)), unit: "%".into(), caption: "CPU".into(), frac: Some((total / 100.0) as f32), rows, ..Default::default() })
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// (total, available) physical memory in bytes.
fn physical_memory() -> Option<(u64, u64)> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut ms = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut ms) }.ok()?;
    Some((ms.ullTotalPhys, ms.ullAvailPhys))
}

/// Top 5 apps by private working set, grouped by exe name.
pub fn top_memory() -> Result<Feed, FeedError> {
    let (total, avail) = physical_memory().ok_or_else(|| FeedError::Failed("Couldn't read memory status".into()))?;
    let items = processes().into_iter().filter_map(|(pid, exe)| Some((pid, exe, Proc::open(pid)?.private_ws()? as f64)));
    let groups = group_by_exe(items);
    let rows = groups
        .iter()
        .take(5)
        .map(|g| FeedRow { label: group_label(g), value: fmt_bytes(g.amount as u64), frac: Some((g.amount / total.max(1) as f64) as f32), delta: None })
        .collect();
    let used = total.saturating_sub(avail);
    let mut extra = BTreeMap::new();
    extra.insert("total_gb".into(), format!("{:.1}", total as f64 / GIB));
    Ok(Feed {
        value: Some(((used as f64 / GIB) * 10.0).round() / 10.0),
        unit: "GB".into(),
        caption: "Memory".into(),
        frac: Some((used as f64 / total.max(1) as f64) as f32),
        rows,
        extra,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------------------
// Battery, storage, uptime
// ---------------------------------------------------------------------------------------

pub fn battery() -> Result<Feed, FeedError> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s) }.map_err(|e| FeedError::Failed(format!("Couldn't read the battery: {e}")))?;
    if s.BatteryFlag == 128 || s.BatteryFlag == 255 || s.BatteryLifePercent == 255 {
        return Err(FeedError::NeedsSetup("This PC has no battery".into()));
    }
    let pct = s.BatteryLifePercent.min(100) as f64;
    let charging = s.BatteryFlag & 8 != 0;
    let time_left = if s.BatteryLifeTime == u32::MAX || charging { String::new() } else { fmt_duration(s.BatteryLifeTime as u64) };
    let mut extra = BTreeMap::new();
    extra.insert("charging".into(), charging.to_string());
    extra.insert("plugged_in".into(), (s.ACLineStatus == 1).to_string());
    extra.insert("time_left".into(), time_left);
    Ok(Feed { value: Some(pct), unit: "%".into(), caption: "Battery".into(), frac: Some((pct / 100.0) as f32), extra, ..Default::default() })
}

/// Free space on every fixed drive; the headline is C: (or the first fixed drive).
pub fn storage() -> Result<Feed, FeedError> {
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    const DRIVE_FIXED: u32 = 3; // Win32_System_WindowsProgramming, not worth a feature for one constant
    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new(); // (letter, label, free, total)
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = wide(&format!("{letter}:\\"));
        let root = PCWSTR(root.as_ptr());
        if unsafe { GetDriveTypeW(root) } != DRIVE_FIXED {
            continue;
        }
        let (mut free, mut total) = (0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(root, Some(&mut free), Some(&mut total), None) }.is_err() || total == 0 {
            continue;
        }
        let mut name = [0u16; 261];
        let label = if unsafe { GetVolumeInformationW(root, Some(&mut name), None, None, None, None) }.is_ok() { from_wide(&name) } else { String::new() };
        drives.push((letter, label, free, total));
    }
    let head = drives.iter().find(|d| d.0 == 'C').or(drives.first()).ok_or_else(|| FeedError::Failed("No fixed drives found".into()))?;
    let used_frac = |free: u64, total: u64| (total.saturating_sub(free) as f64 / total as f64) as f32;
    let rows = drives
        .iter()
        .map(|(l, label, free, total)| FeedRow {
            label: if label.is_empty() { format!("{l}:") } else { format!("{l}: {label}") },
            value: format!("{} free of {}", fmt_bytes(*free), fmt_bytes(*total)),
            frac: Some(used_frac(*free, *total)),
            delta: None,
        })
        .collect();
    Ok(Feed {
        value: Some((head.2 as f64 / GIB).round()),
        unit: "GB".into(),
        caption: format!("Free on {}:", head.0),
        frac: Some(used_frac(head.2, head.3)),
        rows,
        ..Default::default()
    })
}

pub fn uptime() -> Result<Feed, FeedError> {
    let ms = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() };
    let secs = ms / 1000;
    Ok(Feed { value: Some(secs as f64 / 3600.0), text: Some(fmt_duration(secs)), unit: "h".into(), caption: "Uptime".into(), ..Default::default() })
}

// ---------------------------------------------------------------------------------------
// Keyboard language
// ---------------------------------------------------------------------------------------

fn locale_info(name: &[u16], lctype: u32) -> Option<String> {
    use windows::Win32::Globalization::GetLocaleInfoEx;
    let mut buf = [0u16; 128];
    let n = unsafe { GetLocaleInfoEx(PCWSTR(name.as_ptr()), lctype, Some(&mut buf)) };
    (n > 1).then(|| from_wide(&buf))
}

pub fn language() -> Result<Feed, FeedError> {
    use windows::Win32::Globalization::{LCIDToLocaleName, LOCALE_SABBREVLANGNAME, LOCALE_SISO639LANGNAME2, LOCALE_SLOCALIZEDDISPLAYNAME};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let hkl = unsafe {
        let fg = GetForegroundWindow();
        let tid = if fg.0.is_null() { 0 } else { GetWindowThreadProcessId(fg, None) };
        GetKeyboardLayout(tid) // 0 = this thread
    };
    let langid = (hkl.0 as usize & 0xFFFF) as u32;
    let mut name = [0u16; 85];
    if unsafe { LCIDToLocaleName(langid, Some(&mut name), 0) } <= 1 {
        return Err(FeedError::Failed(format!("Unknown input language 0x{langid:04x}")));
    }
    // ISO 639-2 ("eng", "fra") reads better than the Windows abbreviation ("ENC" for en-CA).
    let code = locale_info(&name, LOCALE_SISO639LANGNAME2).or_else(|| locale_info(&name, LOCALE_SABBREVLANGNAME)).unwrap_or_default().to_uppercase();
    let display = locale_info(&name, LOCALE_SLOCALIZEDDISPLAYNAME).unwrap_or_default();
    let mut extra = BTreeMap::new();
    extra.insert("locale".into(), from_wide(&name));
    Ok(Feed { text: Some(code), caption: display, extra, ..Default::default() })
}

// ---------------------------------------------------------------------------------------
// Recent files
// ---------------------------------------------------------------------------------------

/// Target path of a `.lnk` without resolving it (no network or disk search).
fn lnk_target(lnk: &Path) -> Option<String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let w = wide(&lnk.to_string_lossy());
        pf.Load(PCWSTR(w.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let s = from_wide(&buf);
        (!s.is_empty()).then_some(s)
    }
}

/// The 6 newest entries of Windows' Recent list, folders skipped.
pub fn recent_files() -> Result<Feed, FeedError> {
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| FeedError::Failed("APPDATA isn't set".into()))?;
    let dir = PathBuf::from(appdata).join(r"Microsoft\Windows\Recent");
    let rd = std::fs::read_dir(&dir).map_err(|e| FeedError::Failed(format!("Couldn't read the Recent folder: {e}")))?;
    let mut links: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    links.sort_by(|a, b| b.0.cmp(&a.0));
    let mut rows = Vec::new();
    for (_, lnk) in links.iter().take(40) {
        let target = lnk_target(lnk);
        if let Some(t) = &target {
            // Only stat local paths: a UNC share can take seconds to answer.
            if !t.starts_with(r"\\") && Path::new(t).is_dir() {
                continue;
            }
        }
        let label = lnk.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        rows.push(FeedRow { label, value: target.unwrap_or_else(|| lnk.to_string_lossy().into_owned()), frac: None, delta: None });
        if rows.len() == 6 {
            break;
        }
    }
    Ok(Feed { value: Some(rows.len() as f64), caption: "Recent files".into(), rows, ..Default::default() })
}

// ---------------------------------------------------------------------------------------
// AI CLI token logs (Claude Code, Codex)
// ---------------------------------------------------------------------------------------

/// Token use from one API response.
#[derive(Debug, Clone, PartialEq)]
struct Usage {
    /// Dedup key (hash of message id + request id); `None` = always counted.
    key: Option<u64>,
    /// Unix seconds (UTC).
    ts: i64,
    model: Arc<str>,
    /// Uncached input.
    input: u64,
    output: u64,
    /// Tokens written to the prompt cache (all lifetimes).
    cache_write: u64,
    /// The part of `cache_write` written with the 1-hour lifetime (priced higher).
    cache_write_1h: u64,
    cache_read: u64,
}

impl Usage {
    fn total(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// "2026-11-15T04:42:50.146Z" (or with "+hh:mm"/"-hh:mm", or no zone = UTC) → Unix seconds.
fn parse_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !(b[10] == b'T' || b[10] == b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(num(0..4)?, num(5..7)?, num(8..10)?);
    let mut t = days * 86_400 + num(11..13)? * 3600 + num(14..16)? * 60 + num(17..19)?;
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
    }
    match b.get(i) {
        Some(&c) if c == b'+' || c == b'-' => {
            let off = num(i + 1..i + 3)? * 3600 + num(i + 4..i + 6)? * 60;
            t -= if c == b'+' { off } else { -off };
        }
        _ => {}
    }
    Some(t)
}

fn dedup_key(id: Option<&str>, req: Option<&str>) -> Option<u64> {
    if id.is_none() && req.is_none() {
        return None;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (id.unwrap_or(""), req.unwrap_or("")).hash(&mut h);
    Some(h.finish())
}

// Claude Code transcript lines (only the fields we need; everything else is skipped).
#[derive(Deserialize)]
struct ClaudeLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<ClaudeMessage>,
}

#[derive(Deserialize)]
struct ClaudeMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation: Option<ClaudeCacheCreation>,
}

#[derive(Deserialize)]
struct ClaudeCacheCreation {
    ephemeral_1h_input_tokens: Option<u64>,
}

/// One Claude Code transcript line → its usage, for `"type":"assistant"` lines only.
fn parse_claude_line(line: &str, _state: &mut ()) -> Option<Usage> {
    // Cheap prefilter: most lines are user turns, tool results or metadata.
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let l: ClaudeLine = serde_json::from_str(line).ok()?;
    if l.kind.as_deref() != Some("assistant") {
        return None;
    }
    let msg = l.message?;
    let model = msg.model.unwrap_or_default();
    if model.is_empty() || model.starts_with('<') {
        return None; // "<synthetic>" = locally generated, no API call
    }
    let u = msg.usage?;
    let cache_write = u.cache_creation_input_tokens.unwrap_or(0);
    let usage = Usage {
        key: dedup_key(msg.id.as_deref(), l.request_id.as_deref()),
        ts: parse_iso(l.timestamp.as_deref()?)?,
        model: model.into(),
        input: u.input_tokens.unwrap_or(0),
        output: u.output_tokens.unwrap_or(0),
        cache_write,
        cache_write_1h: u.cache_creation.and_then(|c| c.ephemeral_1h_input_tokens).unwrap_or(0).min(cache_write),
        cache_read: u.cache_read_input_tokens.unwrap_or(0),
    };
    (usage.total() > 0).then_some(usage)
}

// Codex CLI rollout lines.
#[derive(Deserialize)]
struct CodexLine {
    timestamp: Option<String>,
    /// "turn_context" (payload has the model), "event_msg" (payload.type says which), …
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<CodexPayload>,
}

#[derive(Deserialize)]
struct CodexPayload {
    #[serde(rename = "type")]
    kind: Option<String>,
    model: Option<String>,
    info: Option<CodexInfo>,
}

#[derive(Deserialize)]
struct CodexInfo {
    total_token_usage: Option<CodexTokens>,
    last_token_usage: Option<CodexTokens>,
}

#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq)]
struct CodexTokens {
    #[serde(default)]
    input_tokens: u64,
    /// Part of `input_tokens` served from the cache.
    #[serde(default)]
    cached_input_tokens: u64,
    /// Includes reasoning tokens.
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl CodexTokens {
    fn total(&self) -> u64 {
        if self.total_tokens > 0 {
            self.total_tokens
        } else {
            self.input_tokens + self.output_tokens
        }
    }
}

/// Per-file Codex parse state: token_count events carry running session totals (and
/// repeat), so each event counts only the growth since the previous one.
#[derive(Debug, Clone, Default)]
struct CodexState {
    prev: Option<CodexTokens>,
    model: Option<Arc<str>>,
}

fn parse_codex_line(line: &str, st: &mut CodexState) -> Option<Usage> {
    let is_tokens = line.contains("\"token_count\"");
    if !is_tokens && !line.contains("\"turn_context\"") {
        return None;
    }
    let l: CodexLine = serde_json::from_str(line).ok()?;
    let p = l.payload?;
    let kind = if l.kind.as_deref() == Some("turn_context") { Some("turn_context") } else { p.kind.as_deref() };
    match kind {
        Some("turn_context") => {
            if let Some(m) = p.model.filter(|m| !m.is_empty()) {
                st.model = Some(m.into());
            }
            None
        }
        Some("token_count") => {
            let info = p.info?;
            let ts = parse_iso(l.timestamp.as_deref()?)?;
            let delta = match (info.total_token_usage, st.prev) {
                (Some(t), Some(prev)) if t.total() >= prev.total() => {
                    st.prev = Some(t);
                    CodexTokens {
                        input_tokens: t.input_tokens.saturating_sub(prev.input_tokens),
                        cached_input_tokens: t.cached_input_tokens.saturating_sub(prev.cached_input_tokens),
                        output_tokens: t.output_tokens.saturating_sub(prev.output_tokens),
                        total_tokens: t.total() - prev.total(),
                    }
                }
                (Some(t), None) => {
                    st.prev = Some(t);
                    t
                }
                // Totals went backwards (counter reset): count the last turn only.
                (Some(t), Some(_)) => {
                    st.prev = Some(t);
                    info.last_token_usage.unwrap_or(t)
                }
                (None, _) => info.last_token_usage?,
            };
            if delta.total() == 0 {
                return None; // repeated event
            }
            let cached = delta.cached_input_tokens.min(delta.input_tokens);
            Some(Usage {
                key: None,
                ts,
                model: st.model.clone().unwrap_or_else(|| "codex".into()),
                input: delta.input_tokens - cached,
                output: delta.output_tokens,
                cache_write: 0,
                cache_write_1h: 0,
                cache_read: cached,
            })
        }
        _ => None,
    }
}

/// Parsed state of one log file.
struct Cached<S> {
    /// Bytes consumed (through the last complete line).
    offset: u64,
    len: u64,
    mtime: SystemTime,
    entries: Vec<Usage>,
    state: S,
}

type Cache<S> = LazyLock<Mutex<HashMap<PathBuf, Cached<S>>>>;

static CLAUDE_CACHE: Cache<()> = LazyLock::new(Default::default);
static CODEX_CACHE: Cache<CodexState> = LazyLock::new(Default::default);

/// Only logs touched in this window are read.
const MAX_AGE: Duration = Duration::from_secs(35 * 86_400);

/// `*.jsonl` files under `dir` (recursive) modified within [`MAX_AGE`].
fn jsonl_files(dir: &Path, out: &mut Vec<(PathBuf, u64, SystemTime)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let cutoff = SystemTime::now() - MAX_AGE;
    for e in rd.flatten() {
        let Ok(ty) = e.file_type() else { continue };
        let path = e.path();
        if ty.is_dir() {
            jsonl_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            if let Ok(m) = e.metadata() {
                if let Ok(mt) = m.modified() {
                    if mt >= cutoff {
                        out.push((path, m.len(), mt));
                    }
                }
            }
        }
    }
}

/// Feeds the complete lines in `buf` to `parse`; returns bytes consumed.
fn parse_lines<S>(buf: &[u8], state: &mut S, parse: fn(&str, &mut S) -> Option<Usage>, out: &mut Vec<Usage>) -> usize {
    let Some(end) = buf.iter().rposition(|&c| c == b'\n') else { return 0 };
    for line in buf[..end].split(|&c| c == b'\n') {
        if let Ok(s) = std::str::from_utf8(line) {
            if let Some(u) = parse(s.trim_end_matches('\r'), state) {
                out.push(u);
            }
        }
    }
    end + 1
}

/// Usage entries from every recent log under `roots`, reusing cached parses.
fn scan<S: Default>(roots: &[PathBuf], cache: &Cache<S>, parse: fn(&str, &mut S) -> Option<Usage>) -> Vec<Usage> {
    let mut files = Vec::new();
    for r in roots {
        jsonl_files(r, &mut files);
    }
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.retain(|p, _| files.iter().any(|f| &f.0 == p));
    let mut all = Vec::new();
    for (path, len, mtime) in files {
        let c = cache.entry(path.clone()).or_insert_with(|| Cached { offset: 0, len: 0, mtime: UNIX_EPOCH, entries: Vec::new(), state: S::default() });
        if c.len != len || c.mtime != mtime {
            if len < c.offset {
                // Rewritten or truncated: start over.
                *c = Cached { offset: 0, len: 0, mtime: UNIX_EPOCH, entries: Vec::new(), state: S::default() };
            }
            if let Ok(mut f) = std::fs::File::open(&path) {
                let mut buf = Vec::new();
                if f.seek(SeekFrom::Start(c.offset)).is_ok() && f.read_to_end(&mut buf).is_ok() {
                    c.offset += parse_lines(&buf, &mut c.state, parse, &mut c.entries) as u64;
                    c.len = len;
                    c.mtime = mtime;
                }
            }
        }
        all.extend(c.entries.iter().cloned());
    }
    all
}

/// Drops repeated responses (same key), keeping the copy with the most tokens.
fn dedup(entries: Vec<Usage>) -> Vec<Usage> {
    let mut seen: HashMap<u64, usize> = HashMap::new();
    let mut out: Vec<Usage> = Vec::with_capacity(entries.len());
    for e in entries {
        match e.key {
            Some(k) => match seen.get(&k) {
                Some(&i) => {
                    if e.total() > out[i].total() {
                        out[i] = e;
                    }
                }
                None => {
                    seen.insert(k, out.len());
                    out.push(e);
                }
            },
            None => out.push(e),
        }
    }
    out
}

/// USD per million tokens: (input, output, cache read). Cache writes are priced at
/// 1.25x input (5-minute lifetime) or 2x input (1-hour lifetime).
///
/// ESTIMATE ONLY: Anthropic first-party list prices as of 2026-10 (from the claude-api
/// reference bundled with Claude Code). Subscription plans (Pro/Max) aren't billed per
/// token, and Bedrock/Vertex prices differ. Unknown models count as $0.
fn claude_price(model: &str) -> (f64, f64, f64) {
    let m = model.to_ascii_lowercase();
    if m.contains("fable") || m.contains("mythos") {
        (10.0, 50.0, if m.contains("5-1") { 0.25 } else { 1.0 })
    } else if m.contains("opus-5-5") {
        (4.0, 20.0, 0.20)
    } else if m.contains("opus") {
        (5.0, 25.0, 0.50)
    } else if m.contains("sonnet-5") {
        (2.0, 10.0, 0.20)
    } else if m.contains("sonnet") {
        (3.0, 15.0, 0.30)
    } else if m.contains("haiku-5") {
        (0.10, 0.50, 0.01)
    } else if m.contains("haiku") {
        (1.0, 5.0, 0.10)
    } else {
        (0.0, 0.0, 0.0)
    }
}

fn estimate_cost(u: &Usage) -> f64 {
    let (i, o, r) = claude_price(&u.model);
    let w5 = u.cache_write - u.cache_write_1h;
    (u.input as f64 * i + u.output as f64 * o + w5 as f64 * i * 1.25 + u.cache_write_1h as f64 * i * 2.0 + u.cache_read as f64 * r) / 1e6
}

const SERIES_DAYS: i64 = 14;

#[derive(Debug, Default, PartialEq)]
struct Totals {
    today: u64,
    /// Daily totals, oldest first, ending today.
    series: Vec<f64>,
    /// Today's tokens per model, largest first.
    models_today: Vec<(Arc<str>, u64)>,
    last_5h: u64,
    last_7d: u64,
    messages_today: u64,
    cost_today: f64,
}

/// Buckets usage by local day. `offset(ts)` gives the local UTC offset in seconds at `ts`.
fn aggregate(entries: &[Usage], now: i64, offset: &mut dyn FnMut(i64) -> i64) -> Totals {
    let mut local_day = |ts: i64| (ts + offset(ts)).div_euclid(86_400);
    let today = local_day(now);
    let mut t = Totals { series: vec![0.0; SERIES_DAYS as usize], ..Default::default() };
    let mut models: HashMap<Arc<str>, u64> = HashMap::new();
    for e in entries {
        let n = e.total();
        if e.ts > now - 5 * 3600 && e.ts <= now + 300 {
            t.last_5h += n;
        }
        if e.ts > now - 7 * 86_400 && e.ts <= now + 300 {
            t.last_7d += n;
        }
        let age = today - local_day(e.ts);
        if (0..SERIES_DAYS).contains(&age) {
            t.series[(SERIES_DAYS - 1 - age) as usize] += n as f64;
        }
        if age == 0 {
            t.today += n;
            t.messages_today += 1;
            t.cost_today += estimate_cost(e);
            *models.entry(e.model.clone()).or_default() += n;
        }
    }
    t.models_today = models.into_iter().collect();
    t.models_today.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    t
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Local UTC offset (seconds) in effect at `ts`, honouring daylight saving for that date.
fn local_offset(ts: i64) -> i64 {
    use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
    let (y, m, d) = civil_from_days(ts.div_euclid(86_400));
    let sod = ts.rem_euclid(86_400);
    let utc = SYSTEMTIME { wYear: y as u16, wMonth: m as u16, wDay: d as u16, wHour: (sod / 3600) as u16, wMinute: (sod % 3600 / 60) as u16, wSecond: (sod % 60) as u16, ..Default::default() };
    let mut local = SYSTEMTIME::default();
    if unsafe { SystemTimeToTzSpecificLocalTime(None, &utc, &mut local) }.is_err() {
        return 0;
    }
    let lt = days_from_civil(local.wYear as i64, local.wMonth as i64, local.wDay as i64) * 86_400 + local.wHour as i64 * 3600 + local.wMinute as i64 * 60 + local.wSecond as i64;
    lt - ts
}

/// Builds the shared Feed shape for an AI CLI's usage.
fn usage_feed(entries: Vec<Usage>, with_cost: bool) -> Feed {
    let mut offsets: HashMap<i64, i64> = HashMap::new();
    let t = aggregate(&dedup(entries), unix_now(), &mut |ts| *offsets.entry(ts.div_euclid(3600)).or_insert_with(|| local_offset(ts)));
    let rows = t
        .models_today
        .iter()
        .take(5)
        .map(|(m, n)| FeedRow { label: m.to_string(), value: fmt_tokens(*n), frac: Some((*n as f64 / t.today.max(1) as f64) as f32), delta: None })
        .collect();
    let mut extra = BTreeMap::new();
    extra.insert("tokens_5h".into(), t.last_5h.to_string());
    extra.insert("tokens_week".into(), t.last_7d.to_string());
    extra.insert("messages_today".into(), t.messages_today.to_string());
    if with_cost {
        extra.insert("cost_today_usd".into(), format!("{:.2}", t.cost_today));
    }
    Feed { value: Some(t.today as f64), unit: "tokens".into(), caption: "Today".into(), series: t.series, rows, extra, ..Default::default() }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

/// Existing, distinct directories from `candidates`.
fn existing(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for c in candidates {
        if c.is_dir() && !out.iter().any(|o| o.as_os_str().eq_ignore_ascii_case(c.as_os_str())) {
            out.push(c);
        }
    }
    out
}

/// Token use from Claude Code's local transcripts. Cost is an estimate (see [`claude_price`]).
pub fn claude_code() -> Result<Feed, FeedError> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        candidates.push(PathBuf::from(dir).join("projects"));
    }
    if let Some(h) = home() {
        candidates.push(h.join(".claude").join("projects"));
        candidates.push(h.join(".config").join("claude").join("projects"));
    }
    let roots = existing(candidates);
    if roots.is_empty() {
        return Err(FeedError::NeedsSetup("Claude Code hasn't been used on this PC".into()));
    }
    Ok(usage_feed(scan(&roots, &CLAUDE_CACHE, parse_claude_line), true))
}

/// Token use from the Codex CLI's local rollout files. No cost estimate (Codex is
/// usually billed through a ChatGPT plan, and its per-token prices aren't built in).
pub fn codex() -> Result<Feed, FeedError> {
    let base = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".codex")));
    let Some(base) = base else { return Err(FeedError::NeedsSetup("Codex CLI hasn't been used on this PC".into())) };
    let roots = existing(vec![base.join("sessions"), base.join("archived_sessions")]);
    if roots.is_empty() {
        return Err(FeedError::NeedsSetup("Codex CLI hasn't been used on this PC".into()));
    }
    Ok(usage_feed(scan(&roots, &CODEX_CACHE, parse_codex_line), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude(id: &str, req: &str, ts: &str, model: &str, i: u64, o: u64, cw: u64, cr: u64) -> String {
        format!(
            r#"{{"parentUuid":"x","type":"assistant","message":{{"model":"{model}","id":"{id}","type":"message","role":"assistant","content":[{{"type":"text","text":"hi"}}],"usage":{{"input_tokens":{i},"cache_creation_input_tokens":{cw},"cache_read_input_tokens":{cr},"output_tokens":{o},"cache_creation":{{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":{cw}}}}}}},"requestId":"{req}","timestamp":"{ts}"}}"#
        )
    }

    #[test]
    fn formatters() {
        assert_eq!(fmt_bytes(640 * 1024 * 1024), "640 MB");
        assert_eq!(fmt_bytes((1.2 * GIB) as u64), "1.2 GB");
        assert_eq!(fmt_bytes((123.4 * GIB) as u64), "123 GB");
        assert_eq!(fmt_bytes((1024.0 * GIB) as u64), "1 TB");
        assert_eq!(fmt_bytes((1.84 * 1024.0 * GIB) as u64), "1.8 TB");
        assert_eq!(fmt_tokens(980), "980");
        assert_eq!(fmt_tokens(45_300), "45.3K");
        assert_eq!(fmt_tokens(1_234_567), "1.2M");
        assert_eq!(fmt_duration(3 * 3600 + 20 * 60 + 5), "3h 20m");
        assert_eq!(fmt_duration(3 * 86_400 + 4 * 3600 + 59), "3d 4h");
        assert_eq!(fmt_duration(12 * 60), "12m");
        assert_eq!(exe_stem("Chrome.EXE"), "Chrome");
    }

    #[test]
    fn iso_timestamps() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2026-11-15T04:42:50.146Z"), Some(1_794_717_770));
        assert_eq!(parse_iso("2026-11-15T00:42:50-04:00"), Some(1_794_717_770));
        assert_eq!(parse_iso("2026-11-15T05:42:50+01:00"), Some(1_794_717_770));
        assert_eq!(parse_iso("not a date"), None);
        let d = days_from_civil(2024, 2, 29);
        assert_eq!(civil_from_days(d), (2024, 2, 29));
    }

    #[test]
    fn claude_lines() {
        let l = claude("msg_1", "req_1", "2026-11-15T04:42:50.146Z", "claude-opus-5-5", 2, 432, 58_202, 10);
        let u = parse_claude_line(&l, &mut ()).unwrap();
        assert_eq!((u.input, u.output, u.cache_write, u.cache_read), (2, 432, 58_202, 10));
        assert_eq!(&*u.model, "claude-opus-5-5");
        assert_eq!(u.ts, 1_794_717_770);
        assert!(u.key.is_some());
        // User lines, synthetic messages and junk are ignored.
        assert!(parse_claude_line(r#"{"type":"user","message":{"role":"user","content":"usage assistant"}}"#, &mut ()).is_none());
        assert!(parse_claude_line(&claude("m", "r", "2026-11-15T04:42:50Z", "<synthetic>", 0, 0, 0, 0), &mut ()).is_none());
        assert!(parse_claude_line("{not json \"usage\" \"assistant\"", &mut ()).is_none());
    }

    #[test]
    fn claude_dedup() {
        let a = claude("msg_1", "req_1", "2026-11-15T04:00:00Z", "claude-opus-5-5", 1, 10, 0, 0);
        let b = claude("msg_2", "req_2", "2026-11-15T04:01:00Z", "claude-opus-5-5", 1, 20, 0, 0);
        let mut v = Vec::new();
        let buf = format!("{a}\n{a}\n{b}\n{a}\n{b}"); // last line incomplete (no newline)
        let used = parse_lines(buf.as_bytes(), &mut (), parse_claude_line, &mut v);
        assert_eq!(v.len(), 4);
        assert_eq!(used, buf.len() - b.len());
        let d = dedup(v);
        assert_eq!(d.len(), 2);
        assert_eq!(d.iter().map(|u| u.total()).sum::<u64>(), 11 + 21);
    }

    #[test]
    fn codex_lines() {
        let tc = |ts: &str, i: u64, c: u64, o: u64| {
            format!(
                r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{i},"cached_input_tokens":{c},"output_tokens":{o},"reasoning_output_tokens":0,"total_tokens":{}}},"last_token_usage":{{"input_tokens":5,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":10}},"model_context_window":272000}}}}}}"#,
                i + o
            )
        };
        let ctx = r#"{"timestamp":"2026-11-15T04:00:00Z","type":"turn_context","payload":{"cwd":"C:\\x","model":"gpt-5-codex","effort":"medium"}}"#;
        let lines = [ctx.to_string(), tc("2026-11-15T04:00:01Z", 1000, 400, 100), tc("2026-11-15T04:00:02Z", 1000, 400, 100), tc("2026-11-15T04:05:00Z", 1500, 900, 300)];
        let mut st = CodexState::default();
        let v: Vec<Usage> = lines.iter().filter_map(|l| parse_codex_line(l, &mut st)).collect();
        assert_eq!(v.len(), 2, "the repeated event counts once");
        assert_eq!(&*v[0].model, "gpt-5-codex");
        assert_eq!((v[0].input, v[0].cache_read, v[0].output), (600, 400, 100));
        assert_eq!((v[1].input, v[1].cache_read, v[1].output), (0, 500, 200));
        assert_eq!(v.iter().map(|u| u.total()).sum::<u64>(), 1800);
        // A null info (sent before the first turn) is ignored.
        let null_info = r#"{"timestamp":"2026-11-15T04:00:00Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#;
        assert!(parse_codex_line(null_info, &mut st).is_none());
    }

    #[test]
    fn daily_buckets() {
        let mk = |ts: i64, n: u64, model: &str| Usage { key: None, ts, model: model.into(), input: n, output: 0, cache_write: 0, cache_write_1h: 0, cache_read: 0 };
        // "Now" is 2026-11-15 02:00 UTC; local time is UTC-5, so it's still Nov 14 locally.
        let now = parse_iso("2026-11-15T02:00:00Z").unwrap();
        let entries = vec![
            mk(now - 3600, 100, "claude-opus-5-5"),         // Nov 14 20:00 local: today, in 5h
            mk(now - 6 * 3600, 50, "claude-sonnet-5-5"),     // Nov 14 15:00 local: today, not in 5h
            mk(now - 23 * 3600, 7, "claude-opus-5-5"),       // Nov 13 22:00 local: yesterday
            mk(now - 13 * 86_400, 3, "claude-opus-5-5"),     // 13 days ago: first bucket
            mk(now - 20 * 86_400, 1000, "claude-opus-5-5"),  // outside everything
        ];
        let t = aggregate(&entries, now, &mut |_| -5 * 3600);
        assert_eq!(t.today, 150);
        assert_eq!(t.messages_today, 2);
        assert_eq!(t.last_5h, 100);
        assert_eq!(t.last_7d, 157);
        assert_eq!(t.series.len(), 14);
        assert_eq!(t.series[13], 150.0);
        assert_eq!(t.series[12], 7.0);
        assert_eq!(t.series[0], 3.0);
        assert_eq!(t.models_today[0], ("claude-opus-5-5".into(), 100));
        // 100 input on Opus 5.5 ($4/M) + 50 input on Sonnet 5.5 ($2/M).
        assert!((t.cost_today - (100.0 * 4.0 + 50.0 * 2.0) / 1e6).abs() < 1e-12);
    }

    #[test]
    fn cost_estimate_uses_cache_rates() {
        let u = Usage { key: None, ts: 0, model: "claude-opus-5-5".into(), input: 1_000_000, output: 1_000_000, cache_write: 2_000_000, cache_write_1h: 1_000_000, cache_read: 1_000_000 };
        // 4 + 20 + 5 (5m write) + 8 (1h write) + 0.20 (read)
        assert!((estimate_cost(&u) - 37.2).abs() < 1e-9);
    }
}
