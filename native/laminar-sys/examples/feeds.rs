//! Calls one widget connector and prints the Feed it returns.
//!
//! Run (from native/):
//!   cargo run -p laminar-sys --example feeds -- <id> [key=value ...]
//!   cargo run -p laminar-sys --example feeds -- github username=torvalds
//!   cargo run -p laminar-sys --example feeds -- list
//!
//! Secret settings (fields marked `secret`) are never printed, only "set (N chars)".
//! Prefer passing them through environment variables over the command line, e.g.
//! `LAMINAR_api_key=…` (any `LAMINAR_<field>` variable becomes that field).

use laminar_sys::connectors;
use laminar_sys::feed::Params;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(id) = args.first() else {
        eprintln!("usage: feeds <id|list> [key=value ...]");
        std::process::exit(2);
    };
    if id == "list" {
        for c in connectors::all() {
            let fields: Vec<String> = c.fields.iter().map(|f| format!("{}{}{}", f.key, if f.required { "*" } else { "" }, if f.secret { " (secret)" } else { "" })).collect();
            println!("{:<15} {:<28} every {:>5}s  {}", c.id, c.name, c.refresh_secs, fields.join(", "));
        }
        return;
    }
    let Some(info) = connectors::info(id) else {
        eprintln!("unknown connector {id} (try `list`)");
        std::process::exit(2);
    };
    let mut p = Params::new();
    for (k, v) in std::env::vars() {
        if let Some(f) = k.strip_prefix("LAMINAR_") {
            p.insert(f.to_string(), v);
        }
    }
    for a in &args[1..] {
        if let Some((k, v)) = a.split_once('=') {
            p.insert(k.to_string(), v.to_string());
        }
    }
    println!("{} ({})", info.name, info.id);
    for f in info.fields {
        let shown = match p.get(f.key) {
            None => "-".to_string(),
            Some(v) if f.secret => format!("set ({} chars)", v.len()),
            Some(v) => v.clone(),
        };
        println!("  {:<14} {shown}", f.key);
    }
    let t = Instant::now();
    let r = connectors::fetch(id, &p);
    println!("fetched in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    match r {
        Err(e) => {
            println!("error: {e:?}");
            std::process::exit(1);
        }
        Ok(f) => {
            println!("value     {:?} {} {}", f.value, f.currency.as_deref().unwrap_or(""), f.unit);
            if let Some(t) = &f.text {
                println!("text      {t}");
            }
            println!("caption   {}", f.caption);
            println!("delta_pct {:?}", f.delta_pct.map(|d| (d * 100.0).round() / 100.0));
            println!("frac      {:?}", f.frac);
            let tail: Vec<String> = f.series.iter().rev().take(6).rev().map(|v| format!("{v}")).collect();
            println!("series    {} points (last: {})", f.series.len(), tail.join(", "));
            let nonzero = f.grid.iter().filter(|g| **g > 0.0).count();
            println!("grid      {} cells ({nonzero} non-zero)", f.grid.len());
            println!("rows      {}", f.rows.len());
            for r in &f.rows {
                println!("  {:<40} {:>14}  frac={:?} delta={:?}", r.label, r.value, r.frac, r.delta);
            }
            println!("extra");
            for (k, v) in &f.extra {
                println!("  {k:<16} {v}");
            }
        }
    }
}
