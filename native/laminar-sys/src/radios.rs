//! Wi-Fi / Bluetooth radio switches and visible Wi-Fi networks.
//!
//! Radio on/off uses WinRT `Windows.Devices.Radios` (`Radio::RequestAccessAsync`,
//! `GetRadiosAsync`, `SetStateAsync`) — the same switches as the Quick Settings toggles,
//! without needing admin rights. Network listing uses the native Wi-Fi API (`wlanapi.dll`:
//! `WlanOpenHandle`, `WlanEnumInterfaces`, `WlanGetAvailableNetworkList`), which returns the
//! OS's cached scan results (no new scan is triggered, so it is fast).
//! Note: on Windows 11 24H2+ the WLAN API needs the Location privacy permission for
//! desktop apps ("Let desktop apps access your location"); without it the list is empty.

use serde::{Deserialize, Serialize};
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Foundation::HANDLE;
use windows::core::PCWSTR;
use windows::Win32::NetworkManagement::WiFi::{
    wlan_connection_mode_profile, WlanCloseHandle, WlanConnect, WlanEnumInterfaces, WlanFreeMemory, WlanGetAvailableNetworkList, WlanOpenHandle, WlanScan,
    WLAN_AVAILABLE_NETWORK_CONNECTED, WLAN_AVAILABLE_NETWORK_HAS_PROFILE, WLAN_AVAILABLE_NETWORK_LIST, WLAN_CONNECTION_PARAMETERS, WLAN_INTERFACE_INFO_LIST,
};

const T: u64 = 3000;

fn radio(kind: RadioKind) -> Option<Radio> {
    let list = wait!(Radio::GetRadiosAsync(), T)?;
    let n = list.Size().ok()?;
    (0..n).filter_map(|i| list.GetAt(i).ok()).find(|r| r.Kind().map(|k| k == kind).unwrap_or(false))
}

fn state(kind: RadioKind) -> Option<bool> {
    radio(kind)?.State().ok().map(|s| s == RadioState::On)
}

fn set_state(kind: RadioKind, on: bool) -> bool {
    let access = wait!(Radio::RequestAccessAsync(), T);
    if access != Some(RadioAccessStatus::Allowed) {
        return false;
    }
    let Some(r) = radio(kind) else { return false };
    let target = if on { RadioState::On } else { RadioState::Off };
    if r.State().map(|s| s == target).unwrap_or(false) {
        return true;
    }
    wait!(r.SetStateAsync(target), T) == Some(RadioAccessStatus::Allowed)
}

/// Wi-Fi radio state; `None` if there is no Wi-Fi radio.
pub fn wifi_on() -> Option<bool> {
    state(RadioKind::WiFi)
}

/// Bluetooth radio state; `None` if there is no Bluetooth radio.
pub fn bluetooth_on() -> Option<bool> {
    state(RadioKind::Bluetooth)
}

/// Turns the Wi-Fi radio on/off. Returns true on success (or if already in that state).
pub fn set_wifi(on: bool) -> bool {
    set_state(RadioKind::WiFi, on)
}

/// Turns the Bluetooth radio on/off. Returns true on success (or if already in that state).
pub fn set_bluetooth(on: bool) -> bool {
    set_state(RadioKind::Bluetooth, on)
}

/// A visible Wi-Fi network (deduped by SSID across BSS entries and interfaces).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WifiNetwork {
    /// SSID (lossy UTF-8). Hidden networks are skipped.
    pub ssid: String,
    /// 0..100 link quality as reported by WLAN (maps roughly linearly to -100..-50 dBm).
    pub signal_percent: u8,
    pub secured: bool,
    pub connected: bool,
    /// Windows has a saved profile for it (joined before), so it can connect without a password.
    #[serde(default)]
    pub known: bool,
}

/// Visible networks from the cached scan, connected first then strongest signal first.
/// Empty on any error (see [`wifi_networks_checked`] for the error code).
pub fn wifi_networks() -> Vec<WifiNetwork> {
    wifi_networks_checked().unwrap_or_default()
}

/// Like [`wifi_networks`] but reports the Win32 error: e.g. 5 (`ERROR_ACCESS_DENIED`) when
/// Location access is off for desktop apps (Windows 11 24H2+), 1062 when the WLAN
/// service is not running. A switched-off radio yields `Ok(empty)`. The UI can use this to show a "turn on location" hint.
pub fn wifi_networks_checked() -> Result<Vec<WifiNetwork>, u32> {
    let mut out: Vec<WifiNetwork> = Vec::new();
    let mut last_err = 0u32;
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        let e = WlanOpenHandle(2, None, &mut ver, &mut h);
        if e != 0 {
            return Err(e);
        }
        let mut ifl: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        let e = WlanEnumInterfaces(h, None, &mut ifl);
        if e != 0 {
            last_err = e;
        }
        if e == 0 && !ifl.is_null() {
            let n = (*ifl).dwNumberOfItems as usize;
            let ifs = std::slice::from_raw_parts((*ifl).InterfaceInfo.as_ptr(), n);
            for itf in ifs {
                let mut nl: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
                let e = WlanGetAvailableNetworkList(h, &itf.InterfaceGuid, 0, None, &mut nl);
                if e != 0 || nl.is_null() {
                    last_err = e;
                    continue;
                }
                let cnt = (*nl).dwNumberOfItems as usize;
                let nets = std::slice::from_raw_parts((*nl).Network.as_ptr(), cnt);
                for net in nets {
                    let len = (net.dot11Ssid.uSSIDLength as usize).min(32);
                    if len == 0 {
                        continue;
                    }
                    let ssid = String::from_utf8_lossy(&net.dot11Ssid.ucSSID[..len]).into_owned();
                    let w = WifiNetwork {
                        ssid,
                        signal_percent: net.wlanSignalQuality.min(100) as u8,
                        secured: net.bSecurityEnabled.as_bool(),
                        connected: net.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0,
                        known: net.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0,
                    };
                    match out.iter_mut().find(|o| o.ssid == w.ssid) {
                        Some(o) => {
                            o.signal_percent = o.signal_percent.max(w.signal_percent);
                            o.connected |= w.connected;
                            o.secured |= w.secured;
                            o.known |= w.known;
                        }
                        None => out.push(w),
                    }
                }
                WlanFreeMemory(nl as *const _);
            }
            WlanFreeMemory(ifl as *const _);
        }
        WlanCloseHandle(h, None);
    }
    // 0x80342002 = ERROR_NDIS_DOT11_POWER_STATE_INVALID: the Wi-Fi radio is off.
    if out.is_empty() && last_err != 0 && last_err != 0x8034_2002 {
        return Err(last_err);
    }
    out.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal_percent.cmp(&a.signal_percent)).then_with(|| a.ssid.cmp(&b.ssid)));
    Ok(out)
}

/// Asks every Wi-Fi adapter for a fresh scan. Results arrive in the OS cache a few seconds
/// later; call [`wifi_networks`] again after ~2-4 s.
pub fn wifi_scan() {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return;
        }
        let mut ifl: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut ifl) == 0 && !ifl.is_null() {
            let n = (*ifl).dwNumberOfItems as usize;
            for itf in std::slice::from_raw_parts((*ifl).InterfaceInfo.as_ptr(), n) {
                let _ = WlanScan(h, &itf.InterfaceGuid, None, None, None);
            }
            WlanFreeMemory(ifl as *const _);
        }
        WlanCloseHandle(h, None);
    }
}

/// Connects to a visible network that Windows already has a saved profile for.
/// Errors: 1168 (`ERROR_NOT_FOUND`) when the network isn't visible or has no saved profile
/// (a new network needs a password: hand off to Windows' own network list), or the WLAN error.
/// Returns once Windows has accepted the request; the connection completes asynchronously.
pub fn wifi_connect(ssid: &str) -> Result<(), u32> {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        let e = WlanOpenHandle(2, None, &mut ver, &mut h);
        if e != 0 {
            return Err(e);
        }
        let mut result = Err(1168u32);
        let mut ifl: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut ifl) == 0 && !ifl.is_null() {
            let n = (*ifl).dwNumberOfItems as usize;
            'ifs: for itf in std::slice::from_raw_parts((*ifl).InterfaceInfo.as_ptr(), n) {
                let mut nl: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
                if WlanGetAvailableNetworkList(h, &itf.InterfaceGuid, 0, None, &mut nl) != 0 || nl.is_null() {
                    continue;
                }
                let cnt = (*nl).dwNumberOfItems as usize;
                for net in std::slice::from_raw_parts((*nl).Network.as_ptr(), cnt) {
                    let len = (net.dot11Ssid.uSSIDLength as usize).min(32);
                    if &net.dot11Ssid.ucSSID[..len] != ssid.as_bytes() || net.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE == 0 {
                        continue;
                    }
                    let params = WLAN_CONNECTION_PARAMETERS {
                        wlanConnectionMode: wlan_connection_mode_profile,
                        strProfile: PCWSTR(net.strProfileName.as_ptr()),
                        pDot11Ssid: std::ptr::null_mut(),
                        pDesiredBssidList: std::ptr::null_mut(),
                        dot11BssType: net.dot11BssType,
                        dwFlags: 0,
                    };
                    let e = WlanConnect(h, &itf.InterfaceGuid, &params, None);
                    result = if e == 0 { Ok(()) } else { Err(e) };
                    WlanFreeMemory(nl as *const _);
                    break 'ifs;
                }
                WlanFreeMemory(nl as *const _);
            }
            WlanFreeMemory(ifl as *const _);
        }
        WlanCloseHandle(h, None);
        result
    }
}
