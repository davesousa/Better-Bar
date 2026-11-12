//! Audio: master volume/mute, output devices, per-app mixer.
//!
//! Core Audio (MMDevice API): `IMMDeviceEnumerator` finds the default render endpoint;
//! `IAudioEndpointVolume` controls master volume/mute; `IAudioSessionManager2` →
//! `IAudioSessionEnumerator` → `ISimpleAudioVolume` gives the per-app volume mixer.
//! Changing the default output device has no public API, so `set_default_output` uses
//! the undocumented-but-stable `IPolicyConfig` COM interface (the one used by the Sound
//! control panel, EarTrumpet, SoundSwitch, AudioDeviceCmdlets; stable since Windows 7).
//! Requires COM on the calling thread ([`crate::com_init_sta`]). All calls are fast (<10 ms).

use crate::util::{file_description, process_image_path, take_pwstr, wide};
use serde::{Deserialize, Serialize};
use windows::core::{IUnknown, GUID, HRESULT, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eCommunications, eConsole, eMultimedia, eRender, ERole, IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};

/// An active audio output (render) endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDevice {
    /// Endpoint id (`{0.0.0.00000000}.{guid}`), pass to [`set_default_output`].
    pub id: String,
    /// Friendly name, e.g. "Speakers (Realtek(R) Audio)".
    pub name: String,
    pub is_default: bool,
}

/// One app in the volume mixer (sessions on the default output device).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSession {
    /// Process id; 0 = "System sounds".
    pub pid: u32,
    /// Exe FileDescription (e.g. "Google Chrome"), else exe name, else session display name.
    pub name: String,
    /// 0..1 session volume.
    pub volume: f32,
}

fn enumerator() -> Option<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() }
}

fn default_device() -> Option<IMMDevice> {
    unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole).ok() }
}

fn endpoint_volume() -> Option<IAudioEndpointVolume> {
    unsafe { default_device()?.Activate(CLSCTX_ALL, None).ok() }
}

/// Master volume of the default output device, 0..1 (scalar, as shown in the UI).
pub fn master_volume() -> Option<f32> {
    unsafe { endpoint_volume()?.GetMasterVolumeLevelScalar().ok() }
}

/// Sets the master volume (clamped to 0..1). Returns success.
pub fn set_master_volume(v: f32) -> bool {
    let Some(ev) = endpoint_volume() else { return false };
    unsafe { ev.SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), std::ptr::null()).is_ok() }
}

/// Mute state of the default output device.
pub fn is_muted() -> Option<bool> {
    unsafe { endpoint_volume()?.GetMute().ok().map(|b| b.as_bool()) }
}

/// Mutes/unmutes the default output device. Returns success.
pub fn set_muted(m: bool) -> bool {
    let Some(ev) = endpoint_volume() else { return false };
    unsafe { ev.SetMute(m, std::ptr::null()).is_ok() }
}

/// Default output endpoint ids for the console, multimedia and communications roles
/// (usually all the same device).
pub fn default_output_ids() -> [Option<String>; 3] {
    let Some(en) = enumerator() else { return [None, None, None] };
    [eConsole, eMultimedia, eCommunications].map(|r| unsafe { en.GetDefaultAudioEndpoint(eRender, r).and_then(|d| d.GetId()).map(take_pwstr).ok() })
}

/// Active output devices with friendly names; the default (console role) is flagged.
pub fn output_devices() -> Vec<AudioDevice> {
    let Some(en) = enumerator() else { return Vec::new() };
    let mut out = Vec::new();
    unsafe {
        let default_id = en.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|d| d.GetId()).map(take_pwstr).unwrap_or_default();
        let Ok(coll) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
        let n = coll.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(dev) = coll.Item(i) else { continue };
            let id = dev.GetId().map(take_pwstr).unwrap_or_default();
            let name = dev
                .OpenPropertyStore(STGM_READ)
                .and_then(|ps| ps.GetValue(&PKEY_Device_FriendlyName))
                .map(|pv| pv.to_string())
                .unwrap_or_default();
            out.push(AudioDevice { is_default: id == default_id, id, name });
        }
    }
    out
}

/// Vtable of `IPolicyConfig` (Windows 7+ layout, IID f8679f50-850a-41cf-9c72-430f290290c8).
/// Declared by hand (the `#[interface]` macro needs a direct `windows-core` dependency).
/// Only `SetDefaultEndpoint` is called; the other slots just keep the layout.
#[repr(C)]
struct IPolicyConfigVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
    get_mix_format: usize,
    get_device_format: usize,
    reset_device_format: usize,
    set_device_format: usize,
    get_processing_period: usize,
    set_processing_period: usize,
    get_share_mode: usize,
    set_share_mode: usize,
    get_property_value: usize,
    set_property_value: usize,
    set_default_endpoint: unsafe extern "system" fn(*mut core::ffi::c_void, PCWSTR, ERole) -> HRESULT,
    set_endpoint_visibility: usize,
}

const IID_IPOLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

/// CLSID of `CPolicyConfigClient`.
const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Makes `id` (from [`output_devices`]) the default output for all three roles
/// (console, multimedia, communications), like the Sound settings page does.
/// Uses undocumented `IPolicyConfig`; returns false if unavailable or the id is unknown.
pub fn set_default_output(id: &str) -> bool {
    // Refuse ids that are not current active render endpoints.
    if !output_devices().iter().any(|d| d.id == id) {
        return false;
    }
    let Ok(unk) = (unsafe { CoCreateInstance::<_, IUnknown>(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }) else {
        return false;
    };
    let mut pc: *mut core::ffi::c_void = std::ptr::null_mut();
    if unsafe { windows::core::Interface::query(&unk, &IID_IPOLICY_CONFIG, &mut pc) }.is_err() || pc.is_null() {
        return false;
    }
    let w = wide(id);
    let mut ok = true;
    unsafe {
        let vtbl = &**(pc as *mut *const IPolicyConfigVtbl);
        for role in [eConsole, eMultimedia, eCommunications] {
            ok &= (vtbl.set_default_endpoint)(pc, PCWSTR(w.as_ptr()), role).is_ok();
        }
        (vtbl.release)(pc);
    }
    ok
}

fn session_controls() -> Vec<(IAudioSessionControl2, ISimpleAudioVolume)> {
    let mut out = Vec::new();
    unsafe {
        let Some(dev) = default_device() else { return out };
        let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { return out };
        let Ok(en) = mgr.GetSessionEnumerator() else { return out };
        let n = en.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(ctl) = en.GetSession(i) else { continue };
            let (Ok(c2), Ok(vol)) = (windows::core::Interface::cast::<IAudioSessionControl2>(&ctl), windows::core::Interface::cast::<ISimpleAudioVolume>(&ctl)) else {
                continue;
            };
            out.push((c2, vol));
        }
    }
    out
}

/// Per-app sessions on the default output device (expired sessions skipped), one entry
/// per pid (first session wins), sorted with System sounds first then by name.
pub fn sessions() -> Vec<AudioSession> {
    use windows::Win32::Media::Audio::AudioSessionStateExpired;
    let mut out: Vec<AudioSession> = Vec::new();
    for (c2, vol) in session_controls() {
        unsafe {
            if c2.GetState().map(|s| s == AudioSessionStateExpired).unwrap_or(true) {
                continue;
            }
            let pid = c2.GetProcessId().unwrap_or(0);
            if out.iter().any(|s| s.pid == pid) {
                continue;
            }
            let is_system = c2.IsSystemSoundsSession() == windows::Win32::Foundation::S_OK;
            let name = if is_system || pid == 0 {
                "System sounds".to_string()
            } else {
                session_name(pid, &c2)
            };
            let volume = vol.GetMasterVolume().unwrap_or(0.0);
            out.push(AudioSession { pid: if is_system { 0 } else { pid }, name, volume });
        }
    }
    out.sort_by(|a, b| (a.pid != 0).cmp(&(b.pid != 0)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

fn session_name(pid: u32, c2: &IAudioSessionControl2) -> String {
    if let Some(path) = process_image_path(pid) {
        if let Some(desc) = file_description(&path) {
            return desc;
        }
        let file = path.rsplit('\\').next().unwrap_or(&path);
        return file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file).to_string();
    }
    let dn = unsafe { c2.GetDisplayName() }.map(take_pwstr).unwrap_or_default();
    if dn.is_empty() || dn.starts_with('@') {
        format!("PID {pid}")
    } else {
        dn
    }
}

/// Sets the volume (0..1) of every session belonging to `pid` (0 = System sounds) on
/// the default output device. Returns true if at least one session was updated.
pub fn set_session_volume(pid: u32, v: f32) -> bool {
    let mut any = false;
    for (c2, vol) in session_controls() {
        unsafe {
            let is_system = c2.IsSystemSoundsSession() == windows::Win32::Foundation::S_OK;
            let spid = if is_system { 0 } else { c2.GetProcessId().unwrap_or(u32::MAX) };
            if spid == pid {
                any |= vol.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null()).is_ok();
            }
        }
    }
    any
}
