//! Widget connectors: each turns one source (a web service or something on this PC)
//! into a [`Feed`]. The UI lists them with [`all`], asks [`fetch`] on a worker thread,
//! and stores secret fields in Credential Manager (see `secrets.rs`).
//!
//! Connector ids are part of the contract with the widget catalog in the `laminar`
//! crate (`widgets/catalog.rs`); don't rename them.
//!
//! Ownership while widgets are being built:
//!   - `local.rs`: local Windows sources (processes, battery, storage, AI CLI logs…)
//!   - every other file here: web services (`common.rs` holds their shared helpers).

pub mod local;

mod ai;
mod analytics;
mod calendar;
mod common;
mod dev;
mod google;
mod markets;
mod revenue;
mod social;
mod wallpaper;

pub use wallpaper::apply_wallpaper;

use crate::feed::*;

/// No-setup connector info for local sources.
const fn local_info(id: &'static str, name: &'static str, about: &'static str, refresh_secs: u64) -> ConnectorInfo {
    ConnectorInfo { id, name, about, help_url: "", fields: &[], refresh_secs }
}

static LOCAL: &[ConnectorInfo] = &[
    local_info("top_cpu", "Processes (CPU)", "The apps using the most processor time right now.", 3),
    local_info("top_memory", "Processes (memory)", "The apps using the most memory right now.", 5),
    local_info("battery", "Battery", "Charge level and time remaining.", 30),
    local_info("storage", "Storage", "Free space on your drives.", 120),
    local_info("claude_code", "Claude Code (this PC)", "Token use from Claude Code's local logs in %USERPROFILE%\\.claude. Nothing leaves your PC.", 60),
    local_info("codex", "Codex CLI (this PC)", "Token use from the Codex CLI's local logs in %USERPROFILE%\\.codex. Nothing leaves your PC.", 60),
    local_info("recent_files", "Recent files", "Files you opened recently (Windows' Recent list).", 60),
    local_info("language", "Keyboard language", "The input language of the window in front.", 2),
    local_info("uptime", "Uptime", "Time since Windows started.", 60),
];

/// Every connector the UI can offer.
pub fn all() -> Vec<&'static ConnectorInfo> {
    [LOCAL, ai::INFOS, revenue::INFOS, analytics::INFOS, google::INFOS, dev::INFOS, social::INFOS, markets::INFOS, calendar::INFOS, wallpaper::INFOS].into_iter().flatten().collect()
}

pub fn info(id: &str) -> Option<&'static ConnectorInfo> {
    all().into_iter().find(|c| c.id == id)
}

/// Fetch a feed. Blocking (network or disk): call from a worker thread.
pub fn fetch(id: &str, p: &Params) -> Result<Feed, FeedError> {
    match id {
        "top_cpu" => local::top_cpu(),
        "top_memory" => local::top_memory(),
        "battery" => local::battery(),
        "storage" => local::storage(),
        "claude_code" => local::claude_code(),
        "codex" => local::codex(),
        "recent_files" => local::recent_files(),
        "language" => local::language(),
        "uptime" => local::uptime(),
        // AI usage
        "anthropic" => ai::anthropic(p),
        "openai" => ai::openai(p),
        "openrouter" => ai::openrouter(p),
        "cursor" => ai::cursor(p),
        // Revenue
        "stripe" => revenue::stripe(p),
        "lemonsqueezy" => revenue::lemonsqueezy(p),
        "paddle" => revenue::paddle(p),
        "shopify" => revenue::shopify(p),
        "gumroad" => revenue::gumroad(p),
        "polar" => revenue::polar(p),
        "revenuecat" => revenue::revenuecat(p),
        // Analytics
        "plausible" => analytics::plausible(p),
        "umami" => analytics::umami(p),
        "search_console" => google::search_console(p),
        "gmail" => google::gmail(p),
        // Developer
        "github" => dev::github(p),
        "leetcode" => dev::leetcode(p),
        "linear" => dev::linear(p),
        "resend" => dev::resend(p),
        "supabase" => dev::supabase(p),
        "vercel" => dev::vercel(p),
        // Social
        "x" => social::x(p),
        "youtube" => social::youtube(p),
        "tiktok" => social::tiktok(p),
        "instagram" => social::instagram(p),
        // Markets
        "stocks" => markets::stocks(p),
        "crypto" => markets::crypto(p),
        // Other
        "calendar" => calendar::calendar(p),
        "wallpaper" => wallpaper::wallpaper(p),
        _ => Err(FeedError::Failed(format!("unknown connector {id}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry() {
        let ids: Vec<&str> = all().iter().map(|c| c.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "duplicate connector id");
        // Every web connector is dispatched, and reports missing settings without a network call.
        for c in all().into_iter().filter(|c| c.fields.iter().any(|f| f.required)) {
            match fetch(c.id, &Params::new()) {
                Err(FeedError::NeedsSetup(_)) => {}
                other => panic!("{}: expected NeedsSetup, got {other:?}", c.id),
            }
            assert!(!c.about.is_empty() && c.help_url.starts_with("https://"), "{}", c.id);
        }
    }
}
