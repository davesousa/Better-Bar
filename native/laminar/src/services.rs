//! Background worker for slow system calls (app list, file search, volume, media,
//! radios, stats, weather, icon extraction). The UI thread never blocks: it sends a
//! request, and the worker posts `WM_APP_SVC` to the dock when a reply is ready.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use laminar_sys::apps::AppEntry;
use laminar_sys::files::FileHit;
use laminar_sys::media::NowPlaying;
use laminar_sys::stats::{Sampler, Stats};
use laminar_sys::weather::Weather;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, IWICImagingFactory};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_APP_SVC: u32 = WM_APP + 10;

pub enum Media {
    PlayPause,
    Next,
    Previous,
}

pub enum Power {
    Lock,
    SignOut,
    Sleep,
    Restart,
    ShutDown,
}

pub enum Req {
    Apps,
    Files(u64, String),
    Control,
    Stats,
    Weather(f32, f32),
    SetVolume(f32),
    SetMuted(bool),
    SetWifi(bool),
    /// List visible Wi-Fi networks now, then again after a fresh scan.
    WifiList,
    /// Join a network; falls back to Windows' own network list for new networks.
    WifiConnect(String),
    SetBluetooth(bool),
    SetDark(bool),
    Media(Media),
    Power(Power),
    /// Extract icons for these app ids into the icon cache folder.
    Icons(std::path::PathBuf, Vec<String>),
    /// Windows Update status (runs on its own thread: it takes several seconds).
    Update,
}

/// The Control Center's view of the system.
#[derive(Clone, Default)]
pub struct ControlState {
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    pub dark: bool,
    pub media: Option<NowPlaying>,
}

pub enum Reply {
    Apps(Vec<AppEntry>),
    Files(u64, Vec<FileHit>),
    Control(ControlState),
    /// Visible networks, or the Windows error code (5: Location access is off).
    Wifi(Result<Vec<laminar_sys::radios::WifiNetwork>, u32>),
    Stats(Stats),
    Weather(Option<Weather>),
    /// (app id, display name) for icons now in the cache.
    Icons(Vec<(String, String)>),
    Update(laminar_sys::update::UpdateStatus),
}

pub struct Services {
    tx: Sender<Req>,
    replies: Arc<Mutex<Vec<Reply>>>,
}

impl Services {
    pub fn start(notify: HWND) -> Self {
        let (tx, rx) = channel::<Req>();
        let replies = Arc::new(Mutex::new(Vec::new()));
        let out = replies.clone();
        let hwnd = notify.0 as isize;
        std::thread::Builder::new()
            .name("laminar-services".into())
            .spawn(move || worker(rx, out, hwnd))
            .ok();
        Self { tx, replies }
    }

    pub fn send(&self, r: Req) {
        let _ = self.tx.send(r);
    }

    pub fn take(&self) -> Vec<Reply> {
        std::mem::take(&mut *self.replies.lock().unwrap())
    }
}

fn control_state() -> ControlState {
    ControlState {
        volume: laminar_sys::audio::master_volume(),
        muted: laminar_sys::audio::is_muted(),
        wifi: laminar_sys::radios::wifi_on(),
        bluetooth: laminar_sys::radios::bluetooth_on(),
        dark: laminar_sys::theme::apps_use_dark(),
        media: laminar_sys::media::now_playing(),
    }
}

fn worker(rx: Receiver<Req>, out: Arc<Mutex<Vec<Reply>>>, hwnd: isize) {
    laminar_sys::com_init_sta();
    let wic: Option<IWICImagingFactory> = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() };
    let mut sampler = Sampler::new();
    let post = |r: Reply| {
        out.lock().unwrap().push(r);
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as _)), WM_APP_SVC, WPARAM(0), LPARAM(0));
        }
    };
    while let Ok(first) = rx.recv() {
        // Coalesce: for searches only the newest query matters.
        let mut batch = vec![first];
        while let Ok(more) = rx.try_recv() {
            batch.push(more);
        }
        let newest_files = batch.iter().rposition(|r| matches!(r, Req::Files(..)));
        for (i, req) in batch.into_iter().enumerate() {
            match req {
                Req::Apps => post(Reply::Apps(laminar_sys::apps::installed_apps())),
                Req::Files(gen, q) => {
                    if Some(i) == newest_files {
                        post(Reply::Files(gen, laminar_sys::files::search_files(&q, 6)));
                    }
                }
                Req::Control => post(Reply::Control(control_state())),
                Req::Stats => post(Reply::Stats(sampler.sample())),
                Req::Weather(lat, lon) => post(Reply::Weather(laminar_sys::weather::fetch(lat, lon))),
                Req::SetVolume(v) => {
                    laminar_sys::audio::set_master_volume(v);
                }
                Req::SetMuted(m) => {
                    laminar_sys::audio::set_muted(m);
                    post(Reply::Control(control_state()));
                }
                Req::SetWifi(on) => {
                    laminar_sys::radios::set_wifi(on);
                    post(Reply::Control(control_state()));
                }
                Req::WifiList => {
                    post(Reply::Wifi(laminar_sys::radios::wifi_networks_checked()));
                    laminar_sys::radios::wifi_scan();
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    post(Reply::Wifi(laminar_sys::radios::wifi_networks_checked()));
                }
                Req::WifiConnect(ssid) => match laminar_sys::radios::wifi_connect(&ssid) {
                    Ok(()) => {
                        // Give Windows a moment to associate, then refresh the list and the tile.
                        for _ in 0..3 {
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            post(Reply::Wifi(laminar_sys::radios::wifi_networks_checked()));
                        }
                        post(Reply::Control(control_state()));
                    }
                    Err(e) => {
                        // New or password-protected network: Windows' own list asks for the password.
                        eprintln!("wifi connect {ssid}: {e}; opening the Windows network list");
                        crate::apps::launch("ms-availablenetworks:");
                    }
                },
                Req::SetBluetooth(on) => {
                    laminar_sys::radios::set_bluetooth(on);
                    post(Reply::Control(control_state()));
                }
                Req::SetDark(on) => {
                    laminar_sys::theme::set_dark_mode(on);
                    post(Reply::Control(control_state()));
                }
                Req::Media(m) => {
                    match m {
                        Media::PlayPause => laminar_sys::media::play_pause(),
                        Media::Next => laminar_sys::media::next(),
                        Media::Previous => laminar_sys::media::previous(),
                    };
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    post(Reply::Control(control_state()));
                }
                Req::Power(p) => {
                    match p {
                        Power::Lock => laminar_sys::power::lock(),
                        Power::SignOut => laminar_sys::power::sign_out(),
                        Power::Sleep => laminar_sys::power::sleep(),
                        Power::Restart => laminar_sys::power::restart(),
                        Power::ShutDown => laminar_sys::power::shut_down(),
                    };
                }
                Req::Update => {
                    let out = out.clone();
                    std::thread::spawn(move || {
                        laminar_sys::com_init_sta();
                        let s = laminar_sys::update::status();
                        out.lock().unwrap().push(Reply::Update(s));
                        unsafe {
                            let _ = PostMessageW(Some(HWND(hwnd as _)), WM_APP_SVC, WPARAM(0), LPARAM(0));
                        }
                    });
                }
                Req::Icons(dir, ids) => {
                    let Some(wic) = &wic else { continue };
                    let mut done = Vec::new();
                    for (k, id) in ids.iter().enumerate() {
                        if let Some(name) = crate::iconcache::extract_to_cache(wic, &dir, id) {
                            done.push((id.clone(), name));
                        }
                        // Report progress in batches so tiles fill in as icons arrive.
                        if done.len() >= 24 || k + 1 == ids.len() {
                            post(Reply::Icons(std::mem::take(&mut done)));
                        }
                    }
                }
            }
        }
    }
}
