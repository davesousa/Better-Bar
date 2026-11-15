//! Runs every local widget connector once, plus the read-only audio/display helpers,
//! and prints what came back with timings. Read-only: changes nothing on the PC.
//!
//! Prints aggregate numbers, process and model names only. Recent-file names and paths
//! are not printed (only how many there are), and no log text is ever read out.
//!
//!     cargo run -p laminar-sys --example local_feeds

use laminar_sys::connectors;
use laminar_sys::feed::{Feed, Params};
use std::time::Instant;

fn show(id: &str, f: &Feed) {
    let value = f.value.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into());
    let text = f.text.as_deref().map(|t| format!(" text={t:?}")).unwrap_or_default();
    let frac = f.frac.map(|v| format!(" frac={v:.2}")).unwrap_or_default();
    println!("  value={value} {}{text}{frac} caption={:?}", f.unit, f.caption);
    if !f.series.is_empty() {
        let s: Vec<String> = f.series.iter().map(|v| format!("{v:.0}")).collect();
        println!("  series[{}]: {}", f.series.len(), s.join(" "));
    }
    if id == "recent_files" {
        println!("  rows: {} (names hidden)", f.rows.len());
    } else {
        for r in &f.rows {
            let frac = r.frac.map(|v| format!(" ({:.0}%)", v * 100.0)).unwrap_or_default();
            println!("  - {} = {}{frac}", r.label, r.value);
        }
    }
    for (k, v) in &f.extra {
        println!("  {k} = {v}");
    }
}

fn main() {
    laminar_sys::com_init_sta();
    let p = Params::new();
    let ids = ["top_cpu", "top_memory", "battery", "storage", "claude_code", "codex", "recent_files", "language", "uptime"];
    for id in ids {
        let t = Instant::now();
        let r = connectors::fetch(id, &p);
        let ms = t.elapsed().as_millis();
        match r {
            Ok(f) => {
                println!("{id} ({ms} ms): ok");
                show(id, &f);
            }
            Err(e) => println!("{id} ({ms} ms): {e:?}"),
        }
    }

    // Second calls show the steady-state cost (CPU deltas, cached log parses).
    for id in ["top_cpu", "claude_code", "codex"] {
        let t = Instant::now();
        let ok = connectors::fetch(id, &p).is_ok();
        println!("{id} again ({} ms): ok={ok}", t.elapsed().as_millis());
    }

    let t = Instant::now();
    println!("audio::mic_muted = {:?} ({} ms)", laminar_sys::audio::mic_muted(), t.elapsed().as_millis());
    let t = Instant::now();
    println!("audio::default_output_name = {:?} ({} ms)", laminar_sys::audio::default_output_name(), t.elapsed().as_millis());
    let t = Instant::now();
    println!("display::brightness = {:?} ({} ms)", laminar_sys::display::brightness(), t.elapsed().as_millis());
}
