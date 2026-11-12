//! Diagnostic: ask GitHub which Laminar release an installed copy would be offered,
//! without installing anything. It pretends to be an installed copy of the given version
//! (a throwaway folder in %TEMP%) and runs Velopack's real update check against the repo.
//!
//!     cargo run --release -p laminar --example update_feed -- 0.0.1
//!
//! Expected: "update available: 0.1.0" (or whatever the newest release is).

use velopack::locator::VelopackLocatorConfig;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager};

const REPO: &str = "https://github.com/davesousa/Better-Bar";

fn main() {
    let pretend = std::env::args().nth(1).unwrap_or_else(|| "0.0.1".into());
    let dir = std::env::temp_dir().join("laminar-update-feed");
    std::fs::create_dir_all(dir.join("packages")).expect("temp folder");
    // Only checked for existence by the update check.
    std::fs::write(dir.join("Update.exe"), b"").expect("placeholder");
    let manifest = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<package><metadata><id>LaminarDock</id><title>Laminar Dock</title>\
         <authors>Dave Sousa</authors><version>{pretend}</version><channel>win</channel><mainExe>laminar.exe</mainExe>\
         </metadata></package>"
    );
    std::fs::write(dir.join("sq.version"), manifest).expect("manifest");
    let loc = VelopackLocatorConfig {
        RootAppDir: dir.clone(),
        UpdateExePath: dir.join("Update.exe"),
        PackagesDir: dir.join("packages"),
        ManifestPath: dir.join("sq.version"),
        CurrentBinaryDir: dir.clone(),
        IsPortable: false,
    };
    let um = match UpdateManager::new(GithubSource::new(REPO, None, false), None, Some(loc)) {
        Ok(m) => m,
        Err(e) => {
            println!("couldn't set up the check: {e}");
            std::process::exit(2);
        }
    };
    println!("pretending to be installed version {pretend}; asking {REPO}");
    match um.check_for_updates() {
        Ok(UpdateCheck::UpdateAvailable(info)) => {
            let t = &info.TargetFullRelease;
            println!("update available: {} ({}, {} bytes, {} delta packages)", t.Version, t.FileName, t.Size, info.DeltasToTarget.len());
        }
        Ok(UpdateCheck::NoUpdateAvailable) => println!("no update: the newest release isn't newer than {pretend}"),
        Ok(UpdateCheck::RemoteIsEmpty) => println!("the feed is empty: no Velopack release published yet"),
        Err(e) => {
            println!("check failed: {e}");
            std::process::exit(1);
        }
    }
}
