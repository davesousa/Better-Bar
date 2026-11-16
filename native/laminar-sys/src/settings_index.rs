//! Every Windows setting Launchpad can search, read from Windows' own search index.
//!
//! Windows ships the list its own search box uses as
//! `C:\Windows\ImmersiveControlPanel\Settings\AllSystemSettings_{GUID}.xml` (three files on
//! Windows 11, about 1,750 entries). Each `<SearchableContent>` entry names a setting and
//! its synonyms as resource references (`@{windows?ms-resource://…}`, `@shell32.dll,-24325`),
//! which `SHLoadIndirectString` turns into text in the user's display language. Three kinds:
//!
//! * Settings app pages and the settings on them: `<PageID>` or `<Node Type="Page">`. The
//!   same files list each page's `ms-settings:` links as `<PolicyIds>`, so links come from
//!   Windows itself; [`PAGE_FALLBACK`] covers the few pages without them.
//! * Classic Control Panel tasks (`Classic_…`): `<DeepLink>` is a canonical Control Panel name
//!   (`Microsoft.PowerOptions\pagePlanSettings`), a command line, or a `shell:` link.
//! * Windows Security pages (`Defender_…`): opened through `windowsdefender://`.
//!
//! The format isn't documented, so everything here is defensive: an entry that can't be
//! understood is skipped, and a missing or unreadable file gives an empty list (search then
//! still has Laminar's own short list). Resolving ~3,500 strings takes a second or two, so
//! the result is cached in `%LOCALAPPDATA%\Laminar\settings-index.json`, keyed by the index
//! files' sizes and dates and the display language, and built on a background thread.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows::core::{HSTRING, PCWSTR};

/// One searchable setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingEntry {
    /// What the setting is called ("VPN settings", "Edit environment variables").
    pub name: String,
    /// Lower-case synonyms Windows lists for it.
    pub keywords: Vec<String>,
    /// Where it lives: "Windows Settings", "Control Panel" or "Windows Security".
    pub area: String,
    /// What to open: an `ms-settings:` / `windowsdefender:` / `shell:` link, or a program.
    pub target: String,
    /// Arguments for `target` when it is a program.
    pub args: String,
    /// A Segoe Fluent Icons glyph for it, when Windows gives one.
    pub glyph: Option<String>,
    /// A whole Settings page (not one setting on a page).
    pub page: bool,
}

/// The index, once it has loaded. `None` until the background load finishes.
pub fn get() -> Option<&'static [SettingEntry]> {
    INDEX.get().map(|v| v.as_slice())
}

static INDEX: OnceLock<Vec<SettingEntry>> = OnceLock::new();
static HAY: OnceLock<Vec<String>> = OnceLock::new();

/// Lower-case searchable text for each entry of [`get`] (same order): the name, the name
/// without hyphens ("wifi" finds "Wi-Fi"), and the synonyms. For a quick first filter.
pub fn hay() -> Option<&'static [String]> {
    HAY.get().map(|v| v.as_slice())
}

fn hay_of(e: &SettingEntry) -> String {
    let n = e.name.to_lowercase();
    format!("{n} {} {}", n.replace('-', ""), e.keywords.join(" "))
}

/// Start loading the index on a background thread (cheap to call more than once).
pub fn warm() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::spawn(|| {
        // Resolving package resources is ~5x faster with COM up on the thread.
        crate::com_init_sta();
        // Time zones for "time in …" too, so the first such search doesn't wait on the registry.
        let _ = crate::worldtime::zones();
        let v = load();
        let _ = HAY.set(v.iter().map(hay_of).collect());
        let _ = INDEX.set(v);
    });
}

/// Load from the cache, or build from Windows' files (and cache the result).
pub fn load() -> Vec<SettingEntry> {
    let files = index_files();
    if files.is_empty() {
        return Vec::new();
    }
    let key = cache_key(&files);
    let cache = cache_path();
    if let Some(p) = &cache {
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Ok(c) = serde_json::from_str::<Cache>(&text) {
                if c.key == key && !c.entries.is_empty() {
                    return c.entries;
                }
            }
        }
    }
    let entries = build(&files, &|s| resolve_indirect(s), &|clsid| clsid_registered(clsid), &|p| Path::new(p).exists());
    if let Some(p) = &cache {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(&Cache { key, entries: entries.clone() }) {
            let _ = std::fs::write(p, text);
        }
    }
    entries
}

#[derive(Serialize, Deserialize)]
struct Cache {
    key: String,
    entries: Vec<SettingEntry>,
}

fn index_files() -> Vec<PathBuf> {
    let windir = std::env::var_os("windir").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let dir = windir.join(r"ImmersiveControlPanel\Settings");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("AllSystemSettings_") && n.ends_with(".xml")).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    // Largest first: it is the most complete, so its names win when entries repeat.
    v.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
    v
}

fn cache_key(files: &[PathBuf]) -> String {
    let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    let mut k = format!("v1;lang={lang}");
    for f in files {
        let m = std::fs::metadata(f).ok();
        let len = m.as_ref().map(|m| m.len()).unwrap_or(0);
        let t = m.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        k.push_str(&format!(";{}:{len}:{t}", f.file_name().and_then(|n| n.to_str()).unwrap_or("")));
    }
    k
}

fn cache_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(r"Laminar\settings-index.json"))
}

/// Turn a resource reference into text ("@shell32.dll,-24325" → "Edit environment variables").
fn resolve_indirect(s: &str) -> Option<String> {
    let mut buf = vec![0u16; 2048];
    unsafe { windows::Win32::UI::Shell::SHLoadIndirectString(&HSTRING::from(s), &mut buf, None).ok()? };
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let t = String::from_utf16_lossy(&buf[..n]).trim().to_string();
    (!t.is_empty()).then_some(t)
}

fn clsid_registered(clsid: &str) -> bool {
    use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, HKEY, HKEY_CLASSES_ROOT, KEY_READ};
    let mut k = HKEY::default();
    let ok = unsafe { RegOpenKeyExW(HKEY_CLASSES_ROOT, &HSTRING::from(format!("CLSID\\{clsid}")), Some(0), KEY_READ, &mut k).is_ok() };
    if ok {
        unsafe {
            let _ = RegCloseKey(k);
        }
    }
    ok
}

fn expand_env(s: &str) -> String {
    let src = HSTRING::from(s);
    let mut buf = vec![0u16; 1024];
    let n = unsafe { windows::Win32::System::Environment::ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf)) } as usize;
    if n == 0 || n > buf.len() {
        return s.to_string();
    }
    String::from_utf16_lossy(&buf[..n.saturating_sub(1)])
}

// ---------------------------------------------------------------- parsing (pure)

/// One `<SearchableContent>` entry, before its text is resolved.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RawEntry {
    pub filename: String,
    /// Settings pages it appears on (first = main).
    pub pages: Vec<String>,
    /// `<Node Type="PageGroup">` (a whole category such as "Network & internet").
    pub group: Option<String>,
    /// `ms-settings:` link names listed for its pages (`<PolicyIds>`, without the scheme).
    pub policy_ids: Vec<String>,
    pub description: String,
    pub keywords: Vec<String>,
    pub deeplink: Option<String>,
    pub glyph: Option<String>,
    pub font: Option<String>,
    /// CLSID that must be registered (`COMCondition`), for Control Panel entries.
    pub condition_clsid: Option<String>,
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(j) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let ent = &tail[1..j];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=j]),
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    out
}

/// Text of each `<tag ...>text</tag>` in `s` (attributes allowed, no nesting of the same tag).
fn tags<'a>(s: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = s[pos..].find(&open) {
        let start = pos + i;
        let after = start + open.len();
        // Must be the whole tag name: "<Path>" or "<Path ", not "<PathX>".
        match s[after..].chars().next() {
            Some('>') | Some(' ') | Some('/') => {}
            _ => {
                pos = after;
                continue;
            }
        }
        let Some(gt) = s[after..].find('>') else { break };
        let attrs = &s[after..after + gt];
        let body_start = after + gt + 1;
        if attrs.ends_with('/') {
            out.push((attrs, ""));
            pos = body_start;
            continue;
        }
        let Some(end) = s[body_start..].find(&close) else { break };
        out.push((attrs, &s[body_start..body_start + end]));
        pos = body_start + end + close.len();
    }
    out
}

fn tag<'a>(s: &'a str, t: &str) -> Option<&'a str> {
    tags(s, t).into_iter().next().map(|(_, b)| b.trim()).filter(|b| !b.is_empty())
}

/// Every entry in one index file.
pub fn parse(xml: &str) -> Vec<RawEntry> {
    let mut out = Vec::new();
    for (_, body) in tags(xml, "SearchableContent") {
        let mut e = RawEntry { filename: tag(body, "Filename").map(unescape).unwrap_or_default(), ..Default::default() };
        if let Some(app) = tag(body, "ApplicationInformation") {
            e.deeplink = tag(app, "DeepLink").map(unescape);
            e.glyph = tag(app, "Glyph").map(unescape);
            e.font = tag(app, "FontFamily").map(unescape);
        }
        if let Some(id) = tag(body, "SettingIdentity") {
            for (_, path) in tags(id, "Path") {
                if let Some(p) = tag(path, "PageID") {
                    e.pages.push(unescape(p));
                }
                for (attrs, node) in tags(path, "Node") {
                    let node = unescape(node.trim());
                    if attrs.contains("\"Page\"") {
                        e.pages.push(node);
                    } else if attrs.contains("\"PageGroup\"") && e.group.is_none() {
                        e.group = Some(node);
                    }
                }
                if let Some(p) = tag(path, "PolicyIds") {
                    e.policy_ids.extend(p.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()));
                }
            }
            // Classic entries put PageID straight under SettingIdentity.
            if e.pages.is_empty() {
                if let Some(p) = tags(id, "PageID").into_iter().next() {
                    e.pages.push(unescape(p.1.trim()));
                }
            }
            if let Some(c) = tag(id, "Condition") {
                // shcond://v1#COMCondition;0;CLSID;{guid}
                if c.contains("COMCondition") {
                    e.condition_clsid = c.rsplit(';').next().map(|g| g.trim().to_string()).filter(|g| g.starts_with('{'));
                }
            }
        }
        if let Some(info) = tag(body, "SettingInformation") {
            e.description = tag(info, "Description").map(unescape).unwrap_or_default();
            for t in ["HighKeywords", "Keywords"] {
                if let Some(k) = tag(info, t) {
                    e.keywords.push(unescape(k));
                }
            }
        }
        out.push(e);
    }
    out
}

/// Links for pages Windows doesn't list `<PolicyIds>` for, and for whole categories.
const PAGE_FALLBACK: &[(&str, &str)] = &[
    ("SettingsPageAccountsUsers", "otherusers"),
    ("SettingsPageAdvancedSystemSettings", "developers"),
    ("SettingsPageFlights", "windowsinsider"),
    ("SettingsPageNetworkWorkplace", "workplace"),
    ("SettingsPagePCSystemBluetooth", "bluetooth"),
    ("SettingsPagePCSystemDevices", "connecteddevices"),
    ("SettingsPageRestoreOneBackup", "backup"),
    ("SettingsPageStart2", "personalization-start"),
    ("SettingsPageGroupNetwork", "network-status"),
    ("SettingsPageGroupPersonalization", "personalization"),
    ("SettingsPageGroupPrivacy", "privacy"),
    ("SettingsPageGroupPrivacy2", "privacy"),
    ("SettingsPageGroupPCSystem", "system"),
    ("SettingsPageGroupAccounts", "accounts"),
    ("SettingsPageGroupApps", "appsfeatures"),
    ("SettingsPageGroupDevices", "devices"),
    ("SettingsPageGroupGaming", "gaming-gamebar"),
    ("SettingsPageGroupTimeRegion", "dateandtime"),
    ("SettingsPageGroupUpdate", "windowsupdate"),
    ("SettingsPageGroupUpdateSecurity", "windowsupdate"),
    ("SettingsPageGroupEaseOfAccess", "easeofaccess-display"),
];

/// For pages with several links, the one that opens the page itself (Windows lists them
/// alphabetically, so the first is often a sub-page: Sound's first is the volume mixer).
const PAGE_PREFERRED: &[(&str, &str)] = &[
    ("SettingsPageAudio", "sound"),
    ("SettingsPageTimeRegionKeyboard", "keyboard"),
    ("SettingsPageTimeRegionLanguage", "regionlanguage"),
    ("SettingsPageTimeRegionSpelling", "typing"),
    ("SettingsPagePowerAndBattery", "powersleep"),
    ("SettingsPagePCSystemDisplay", "display"),
    ("SettingsPageAccountsSync", "sync"),
    ("SettingsPageAppsDefaults", "defaultapps"),
    ("SettingsPageInstalledApps", "appsfeatures"),
    ("SettingsPageDevices", "bluetooth"),
    ("SettingsPageSearchIndex", "search"),
    ("SettingsPageRationalizedSearchSettings", "search"),
    ("SettingsPageStorageSenseStorageOverview", "storagesense"),
    ("SettingsPagePasskeysManagement", "savedpasskeys"),
    ("SettingsPageOtherUsers", "otherusers"),
    ("SettingsPageA9", "recall"),
];

/// Compact form for matching link names against entry names ("network-vpn" → "networkvpn").
fn compact(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

/// The `ms-settings:` link for an entry, given every page's links.
fn settings_link(e: &RawEntry, name: &str, links: &HashMap<String, Vec<String>>) -> Option<String> {
    if let Some(page) = e.pages.first() {
        let ids = links.get(page).cloned().unwrap_or_default();
        if !ids.is_empty() {
            // A sub-page link named after this setting ("nightlight" for a Night light entry).
            let (f, n) = (compact(&e.filename), compact(name));
            if let Some(id) = ids.iter().filter(|id| id.len() > 5 && (f.contains(&compact(id)) || n.contains(&compact(id)))).max_by_key(|id| id.len()) {
                return Some(format!("ms-settings:{id}"));
            }
            if let Some((_, pref)) = PAGE_PREFERRED.iter().find(|(p, _)| p == page) {
                if ids.iter().any(|id| id == pref) {
                    return Some(format!("ms-settings:{pref}"));
                }
            }
            return Some(format!("ms-settings:{}", ids[0]));
        }
        if let Some((_, id)) = PAGE_FALLBACK.iter().find(|(p, _)| p == page) {
            return Some(format!("ms-settings:{id}"));
        }
        return None;
    }
    let g = e.group.as_deref()?;
    PAGE_FALLBACK.iter().find(|(p, _)| *p == g).map(|(_, id)| format!("ms-settings:{id}"))
}

/// What a Control Panel deep link opens: (program or link, arguments).
pub fn classic_target(deeplink: &str, expand: &dyn Fn(&str) -> String) -> Option<(String, String)> {
    let d = deeplink.trim();
    if d.is_empty() {
        return None;
    }
    let lower = d.to_ascii_lowercase();
    if lower.starts_with("shell:") || lower.contains("://") {
        return Some((d.to_string(), String::new()));
    }
    if lower.starts_with("microsoft.") {
        // Canonical name, optionally with a page: Microsoft.PowerOptions\pagePlanSettings.
        let (name, page) = match d.split_once('\\') {
            Some((n, p)) => (n, p),
            None => (d, ""),
        };
        let args = if page.is_empty() || page.starts_with('?') { format!("/name {name}") } else { format!("/name {name} /page {page}") };
        return Some(("control.exe".into(), args));
    }
    // A command line: program path (may hold %windir%) then arguments.
    let exp = expand(d);
    let el = exp.to_ascii_lowercase();
    let cut = [".exe", ".msc", ".cpl"].iter().filter_map(|x| el.find(x).map(|i| i + x.len())).min()?;
    let (exe, args) = exp.split_at(cut);
    Some((exe.trim().to_string(), args.trim().to_string()))
}

/// Windows Security deep link names → `windowsdefender://` link.
fn defender_link(deeplink: Option<&str>) -> String {
    match deeplink.map(|d| d.trim()) {
        None | Some("") | Some("None") => "windowsdefender:".into(),
        Some(d) => format!("windowsdefender://{}", d.to_ascii_lowercase()),
    }
}

/// Keyword strings: modern ones resolve to "a;b;c", classic ones are "@@ref@ref…" lists.
fn keyword_refs(k: &str) -> Vec<String> {
    if let Some(list) = k.strip_prefix("@@") {
        list.split('@').filter(|p| !p.trim().is_empty()).map(|p| format!("@{}", p.trim())).collect()
    } else {
        vec![k.to_string()]
    }
}

fn glyph_for(e: &RawEntry) -> Option<String> {
    let font = e.font.as_deref()?;
    // "Settings Fluent Icons" is a private font; only the system icon fonts are drawable.
    if !(font.contains("Segoe Fluent Icons") || font.contains("Segoe MDL2 Assets")) {
        return None;
    }
    e.glyph.clone().filter(|g| g.chars().count() == 1)
}

/// Resolve and de-duplicate entries from the given files.
pub fn build(files: &[PathBuf], lookup: &dyn Fn(&str) -> Option<String>, clsid_ok: &dyn Fn(&str) -> bool, exists: &dyn Fn(&str) -> bool) -> Vec<SettingEntry> {
    let mut raw = Vec::new();
    for f in files {
        if let Ok(bytes) = std::fs::read(f) {
            let text = String::from_utf8_lossy(&bytes);
            raw.extend(parse(text.trim_start_matches('\u{feff}')));
        }
    }
    build_from(&raw, lookup, clsid_ok, exists, &expand_env)
}

/// The resolving half of [`build`] (separate so tests can feed it parsed entries).
pub fn build_from(raw: &[RawEntry], lookup: &dyn Fn(&str) -> Option<String>, clsid_ok: &dyn Fn(&str) -> bool, exists: &dyn Fn(&str) -> bool, expand: &dyn Fn(&str) -> String) -> Vec<SettingEntry> {
    // Every page's links, from all entries.
    let mut links: HashMap<String, Vec<String>> = HashMap::new();
    for e in raw {
        if let Some(p) = e.pages.first() {
            let v = links.entry(p.clone()).or_default();
            for id in &e.policy_ids {
                if !v.contains(id) {
                    v.push(id.clone());
                }
            }
        }
    }
    let mut out: Vec<SettingEntry> = Vec::new();
    let mut seen: HashMap<(String, String, String), usize> = HashMap::new();
    let mut cache: HashMap<String, Option<String>> = HashMap::new();
    let mut res = |s: &str| -> Option<String> { cache.entry(s.to_string()).or_insert_with(|| if s.starts_with('@') { lookup(s) } else { Some(s.to_string()) }).clone() };
    for e in raw {
        let classic = e.filename.starts_with("Classic_");
        let defender = e.filename.starts_with("Defender_");
        let Some(name) = res(&e.description) else { continue };
        if name.starts_with('@') || name.starts_with("ms-resource") {
            continue;
        }
        let (area, target, args) = if classic {
            if let Some(c) = &e.condition_clsid {
                if !clsid_ok(c) {
                    continue;
                }
            }
            let Some((t, a)) = e.deeplink.as_deref().and_then(|d| classic_target(d, expand)) else { continue };
            if t.contains('\\') && !exists(&t) {
                continue;
            }
            ("Control Panel", t, a)
        } else if defender {
            ("Windows Security", defender_link(e.deeplink.as_deref()), String::new())
        } else {
            let Some(l) = settings_link(e, &name, &links) else { continue };
            ("Windows Settings", l, String::new())
        };
        let mut keywords: Vec<String> = Vec::new();
        for k in &e.keywords {
            for r in keyword_refs(k) {
                if let Some(t) = res(&r) {
                    for w in t.split(';') {
                        let w = w.trim().to_lowercase();
                        if !w.is_empty() && !w.starts_with('@') && !keywords.contains(&w) {
                            keywords.push(w);
                        }
                    }
                }
            }
        }
        let page = !classic && !defender && (e.filename.trim_end_matches("-2") == format!("AAA_{}", e.pages.first().map(|s| s.as_str()).unwrap_or("")) || e.group.is_some());
        let key = (name.to_lowercase(), target.clone(), args.clone());
        if let Some(&i) = seen.get(&key) {
            // Same setting listed twice (feature-flag variants): merge the synonyms.
            for k in keywords {
                if !out[i].keywords.contains(&k) {
                    out[i].keywords.push(k);
                }
            }
            out[i].page |= page;
            continue;
        }
        seen.insert(key, out.len());
        out.push(SettingEntry { name, keywords, area: area.into(), target, args, glyph: glyph_for(e), page });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<PCSettings>
  <SearchableContent ExcludeWithFeature="SettingsKeywordsCleanup">
    <Filename>AAA_SettingsPageNetworkVPN-2</Filename>
    <ApplicationInformation><FontFamily>Segoe Fluent Icons</FontFamily><Glyph>&#xE705;</Glyph></ApplicationInformation>
    <SettingIdentity><SettingPaths><Path><PageID>SettingsPageNetworkVPN</PageID><PolicyIds>network-vpn</PolicyIds></Path></SettingPaths></SettingIdentity>
    <SettingInformation><Description>@vpn-desc</Description><HighKeywords>@vpn-kw</HighKeywords></SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>AAA_SettingsPageNetworkVPN</Filename>
    <ApplicationInformation><FontFamily>Segoe Fluent Icons</FontFamily><Glyph>&#xE705;</Glyph></ApplicationInformation>
    <SettingIdentity><SettingPaths><Path><Node Type="Page">SettingsPageNetworkVPN</Node></Path></SettingPaths></SettingIdentity>
    <SettingInformation><Description>@vpn-desc</Description><HighKeywords>@vpn-kw2</HighKeywords></SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>SystemSettings_Display_BlueLight_NightLight</Filename>
    <SettingIdentity><SettingPaths><Path><PageID>SettingsPagePCSystemDisplay</PageID><PolicyIds>display;nightlight</PolicyIds></Path></SettingPaths></SettingIdentity>
    <SettingInformation><Description>Night light</Description></SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>Classic_{11135AE0-7372-4f85-8D1B-93D6EFBE5A99}</Filename>
    <ApplicationInformation><DeepLink>%windir%\system32\rundll32.exe sysdm.cpl,EditEnvironmentVariables</DeepLink></ApplicationInformation>
    <SettingIdentity><PageID>{C58C4893-3BE0-4B45-ABB5-A63E4B8C8651}</PageID></SettingIdentity>
    <SettingInformation><Description>@env-desc</Description><Keywords>@@shell32.dll,-1@shell32.dll,-2</Keywords></SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>Classic_{X}</Filename>
    <ApplicationInformation><DeepLink>Microsoft.PowerOptions\pagePlanSettings</DeepLink></ApplicationInformation>
    <SettingIdentity><PageID>{A}</PageID><Condition>shcond://v1#COMCondition;0;CLSID;{missing}</Condition></SettingIdentity>
    <SettingInformation><Description>Hidden power plan</Description></SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>Defender_VirusThreat</Filename>
    <ApplicationInformation><DeepLink>Threat</DeepLink></ApplicationInformation>
    <SettingIdentity><PageID>{6E6DDBCB}</PageID></SettingIdentity>
    <SettingInformation><Description>Virus &amp; threat protection</Description></SettingInformation>
  </SearchableContent>
</PCSettings>"#;

    fn lookup(s: &str) -> Option<String> {
        Some(
            match s {
                "@vpn-desc" => "VPN settings",
                "@vpn-kw" => "virtual private network;VPN profile",
                "@vpn-kw2" => "private network",
                "@env-desc" => "Edit environment variables",
                "@shell32.dll,-1" => "path",
                "@shell32.dll,-2" => "variables;system variables",
                _ => return None,
            }
            .to_string(),
        )
    }

    fn built() -> Vec<SettingEntry> {
        let raw = parse(SAMPLE);
        build_from(&raw, &lookup, &|c| c != "{missing}", &|_| true, &|s| s.replace("%windir%", r"C:\Windows"))
    }

    #[test]
    fn parses_every_kind() {
        let raw = parse(SAMPLE);
        assert_eq!(raw.len(), 6);
        assert_eq!(raw[0].pages, vec!["SettingsPageNetworkVPN"]);
        assert_eq!(raw[0].policy_ids, vec!["network-vpn"]);
        assert_eq!(raw[0].glyph.as_deref(), Some("\u{E705}"));
        assert_eq!(raw[1].pages, vec!["SettingsPageNetworkVPN"]);
        assert_eq!(raw[3].pages, vec!["{C58C4893-3BE0-4B45-ABB5-A63E4B8C8651}"]);
        assert_eq!(raw[4].condition_clsid.as_deref(), Some("{missing}"));
        assert_eq!(raw[5].description, "Virus & threat protection");
    }

    #[test]
    fn resolves_links_and_merges_duplicates() {
        let v = built();
        let vpn: Vec<_> = v.iter().filter(|e| e.name == "VPN settings").collect();
        assert_eq!(vpn.len(), 1, "feature-flag duplicates merge");
        assert_eq!(vpn[0].target, "ms-settings:network-vpn");
        assert!(vpn[0].page);
        assert!(vpn[0].keywords.contains(&"virtual private network".to_string()));
        assert!(vpn[0].keywords.contains(&"private network".to_string()));
        let night = v.iter().find(|e| e.name == "Night light").unwrap();
        assert_eq!(night.target, "ms-settings:nightlight", "a sub-page link named after the setting wins");
        let env = v.iter().find(|e| e.name == "Edit environment variables").unwrap();
        assert_eq!(env.target, r"C:\Windows\system32\rundll32.exe");
        assert_eq!(env.args, "sysdm.cpl,EditEnvironmentVariables");
        assert_eq!(env.area, "Control Panel");
        assert_eq!(env.keywords, vec!["path", "variables", "system variables"]);
        assert!(!v.iter().any(|e| e.name == "Hidden power plan"), "unregistered CLSID is skipped");
        let sec = v.iter().find(|e| e.area == "Windows Security").unwrap();
        assert_eq!(sec.target, "windowsdefender://threat");
    }

    #[test]
    fn control_panel_targets() {
        let ex = |s: &str| s.replace("%windir%", r"C:\Windows");
        assert_eq!(classic_target(r"Microsoft.PowerOptions\pagePlanSettings", &ex), Some(("control.exe".into(), "/name Microsoft.PowerOptions /page pagePlanSettings".into())));
        assert_eq!(classic_target(r"Microsoft.CredentialManager\?SelectedVault=CredmanVault", &ex), Some(("control.exe".into(), "/name Microsoft.CredentialManager".into())));
        assert_eq!(classic_target(r"%windir%\system32\mmc.exe %windir%\system32\diskmgmt.msc", &ex), Some((r"C:\Windows\system32\mmc.exe".into(), r"C:\Windows\system32\diskmgmt.msc".into())));
        assert_eq!(classic_target(r"shell:::{26EE0668-A00A-44D7-9371-BEB064C98683}\3", &ex).map(|t| t.1), Some(String::new()));
    }

    /// The real index on this PC (skipped where Windows has none).
    #[test]
    fn real_index_finds_common_settings() {
        let files = index_files();
        if files.is_empty() {
            return;
        }
        crate::com_init_sta();
        let t = std::time::Instant::now();
        let v = build(&files, &|s| resolve_indirect(s), &|c| clsid_registered(c), &|p| Path::new(p).exists());
        eprintln!("settings index: {} entries in {:?}", v.len(), t.elapsed());
        if let Ok(dir) = std::env::var("LAMINAR_DUMP_INDEX") {
            let _ = std::fs::write(dir, serde_json::to_string_pretty(&v).unwrap());
        }
        assert!(v.len() > 500, "only {} entries", v.len());
        let has = |target: &str| v.iter().any(|e| e.target == target);
        for t in ["ms-settings:network-vpn", "ms-settings:nightlight", "ms-settings:display", "ms-settings:sound", "ms-settings:startupapps"] {
            assert!(has(t), "missing {t}");
        }
        assert!(v.iter().any(|e| e.area == "Control Panel" && e.args.contains("EditEnvironmentVariables")));
    }
}
