//! Prints Windows Update status (read-only).
//! Run: CARGO_TARGET_DIR=target/sys cargo run --release -p laminar-sys --example update
fn main() {
    laminar_sys::com_init_sta();
    let t = std::time::Instant::now();
    let s = laminar_sys::update::status();
    println!("{s:?} in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
}
