//! Exercises every read-only function of laminar-sys and prints results + timings.
//! Setters are only called with the value already in effect (no state change).
//!
//! Run: CARGO_TARGET_DIR=target/sys cargo run --release -p laminar-sys --example probe

use laminar_sys::*;
use std::time::Instant;

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    println!("[{:>7.1} ms] {label}", t.elapsed().as_secs_f64() * 1000.0);
    r
}

fn main() {
    println!("com_init_sta: {}", com_init_sta());

    // apps
    let apps = timed("apps::installed_apps (cold)", apps::installed_apps);
    let _ = timed("apps::installed_apps (warm)", apps::installed_apps);
    println!("  {} apps; first 10:", apps.len());
    for a in apps.iter().take(10) {
        println!("    {:<40} {}", a.name, a.id);
    }

    // search
    for q in ["cal", "term", "vsc"] {
        let r = timed(&format!("search::rank({q:?})"), || search::rank(q, &apps, 5));
        for (a, s) in r {
            println!("    {s:>5}  {}", a.name);
        }
    }

    // files (names only, no paths)
    for _ in 0..2 {
        let hits = timed("files::search_files(\"readme\", 5)", || files::search_files("readme", 5));
        println!("  {} hits: {:?}", hits.len(), hits.iter().map(|h| format!("{}{}", h.name, if h.is_folder { "/" } else { "" })).collect::<Vec<_>>());
    }
    let hits = timed("files::search_files(\"zzqxjv\", 5)", || files::search_files("zzqxjv", 5));
    println!("  {} hits for nonsense query", hits.len());

    // audio
    let vol = timed("audio::master_volume", audio::master_volume);
    let muted = timed("audio::is_muted", audio::is_muted);
    println!("  volume={vol:?} muted={muted:?}");
    if let Some(v) = vol {
        println!("  set_master_volume(current) -> {}", timed("audio::set_master_volume(same)", || audio::set_master_volume(v)));
        println!("  volume after = {:?}", audio::master_volume());
    }
    if let Some(m) = muted {
        println!("  set_muted(current) -> {}", timed("audio::set_muted(same)", || audio::set_muted(m)));
    }
    let devs = timed("audio::output_devices", audio::output_devices);
    for d in &devs {
        println!("    {}{}", if d.is_default { "* " } else { "  " }, d.name);
    }
    let ids = audio::default_output_ids();
    if ids[0].is_some() && ids[0] == ids[1] && ids[0] == ids[2] {
        let id = ids[0].clone().unwrap();
        let ok = timed("audio::set_default_output(current default, all roles already default)", || audio::set_default_output(&id));
        println!("  set_default_output(same) -> {ok}; default still = {:?}", audio::default_output_ids()[0] == Some(id));
    } else {
        println!("  skipped set_default_output: roles have different defaults (would change state)");
    }
    let sess = timed("audio::sessions", audio::sessions);
    for s in &sess {
        println!("    pid={:<6} vol={:.2} {}", s.pid, s.volume, s.name);
    }
    if let Some(s) = sess.first() {
        let ok = timed("audio::set_session_volume(same)", || audio::set_session_volume(s.pid, s.volume));
        println!("  set_session_volume({}, same) -> {ok}", s.pid);
    }

    // media
    let np = timed("media::now_playing", media::now_playing);
    match &np {
        Some(n) => println!(
            "  title={:?} artist={:?} album={:?} playing={} app={:?} thumb_png={:?}",
            n.title,
            n.artist,
            n.album,
            n.playing,
            n.app_id,
            n.thumbnail_png.as_ref().map(|b| (b.len(), b.starts_with(b"\x89PNG")))
        ),
        None => println!("  nothing playing"),
    }
    let _ = timed("media::now_playing_no_thumbnail", media::now_playing_no_thumbnail);

    // radios
    let wifi = timed("radios::wifi_on", radios::wifi_on);
    let bt = timed("radios::bluetooth_on", radios::bluetooth_on);
    println!("  wifi={wifi:?} bluetooth={bt:?}");
    let nets = timed("radios::wifi_networks_checked", radios::wifi_networks_checked);
    match nets {
        Ok(n) => {
            println!("  {} networks", n.len());
            for w in n.iter().take(10) {
                println!("    {}{:<32} {:>3}% {}", if w.connected { "* " } else { "  " }, w.ssid, w.signal_percent, if w.secured { "secured" } else { "open" });
            }
        }
        Err(e) => println!("  WLAN error {e}"),
    }

    // theme
    let dark = timed("theme::apps_use_dark", theme::apps_use_dark);
    println!("  apps dark={dark} system dark={}", theme::system_uses_dark());
    let acc = timed("theme::accent_color", theme::accent_color);
    println!("  accent={acc:?}");

    // stats
    let mut s = timed("stats::Sampler::new", stats::Sampler::new);
    let a = timed("stats::sample #1", || s.sample());
    std::thread::sleep(std::time::Duration::from_secs(1));
    let b = timed("stats::sample #2", || s.sample());
    println!("  #1 {a:?}\n  #2 {b:?}");

    // weather
    let (lat, lon) = weather::DEFAULT_LAT_LON;
    let w = timed("weather::fetch(Toronto)", || weather::fetch(lat, lon));
    println!("  {w:?}");
    let g = timed("weather::geocode(\"Toronto\")", || weather::geocode("Toronto"));
    println!("  {g:?}");
}
