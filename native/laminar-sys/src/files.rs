//! File/folder search in the user's profile via the Windows Search indexer.
//!
//! WinRT `Windows.Storage`: `StorageFolder::GetFolderFromPathAsync(%USERPROFILE%)` →
//! `CreateItemQueryWithOptions(QueryOptions)` with an AQS `UserSearchFilter` on the
//! file name, `FolderDepth::Deep`, `IndexerOption::OnlyUseIndexerAndOptimizeForIndexedProperties`
//! (never falls back to a slow filesystem crawl) and sorted by `System.DateModified`
//! descending (recent files first; this is also the fastest ordering measured).
//!
//! Matching semantics: `System.FileName:word` is a *word-prefix* match on the indexed
//! file name ("bud" finds "budget-2024.xlsx" and "my_buddy.py", not "rosebud.txt").
//! A true substring filter (`System.FileName:~="..."`) was measured at ~800 ms because
//! the indexer has to scan, versus ~150 ms for word-prefix, so word-prefix is used.
//! Only indexed locations are searched (by default the profile's Desktop/Documents/etc.).
//! Hits inside developer/cache folders are dropped: node_modules and site-packages in
//! the AQS itself (measured: ~280 ms with, vs. 90% junk without), .git/AppData/caches
//! by a post-filter.
//! Results are re-ranked with [`crate::search::fuzzy_score`] on the name (stable, so
//! recency breaks ties). The call is bounded by a deadline; if the indexer is off or
//! slow the result is simply empty. Typical: 200-300 ms (first call per thread +~100 ms).
//! Requires COM on the calling thread ([`crate::com_init_sta`]).

use crate::search::fuzzy_score;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::HSTRING;
use windows::Storage::Search::{FolderDepth, IndexerOption, QueryOptions, SortEntry};
use windows::Storage::{StorageFolder, StorageItemTypes};

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHit {
    /// Full path.
    pub path: String,
    /// File or folder name (with extension).
    pub name: String,
    pub is_folder: bool,
}

/// Overall time budget for one search.
const BUDGET: Duration = Duration::from_millis(1500);

thread_local! {
    // The profile StorageFolder is cached per thread (first lookup costs ~10-30 ms).
    static PROFILE: RefCell<Option<StorageFolder>> = const { RefCell::new(None) };
}

fn profile_folder() -> Option<StorageFolder> {
    if let Some(f) = PROFILE.with(|p| p.borrow().clone()) {
        return Some(f);
    }
    let path = std::env::var("USERPROFILE").ok()?;
    let f: StorageFolder = wait!(StorageFolder::GetFolderFromPathAsync(&HSTRING::from(path)), 1000)?;
    PROFILE.with(|p| *p.borrow_mut() = Some(f.clone()));
    Some(f)
}

/// Searches indexed files and folders under the user profile whose name contains the
/// query words. Returns at most `limit` hits, best name match first; empty on any error,
/// on timeout, or when the indexer is unavailable.
pub fn search_files(query: &str, limit: usize) -> Vec<FileHit> {
    search_inner(query, limit).unwrap_or_default()
}

/// Builds the AQS filter: the query is split into alphanumeric words and every word
/// must word-prefix-match the file name (`System.FileName:word`, implicitly ANDed).
/// Splitting on punctuation also strips anything AQS would interpret as syntax.
/// Dependency folders are excluded in the query itself; see [`is_noise`] for the rest.
fn aqs(query: &str) -> Option<String> {
    let words: Vec<&str> = query.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return None;
    }
    let mut f = words.iter().map(|w| format!("System.FileName:{w}")).collect::<Vec<_>>().join(" ");
    // Excluding these in AQS (word match on the folder path, ~+30 ms) instead of only
    // post-filtering matters: they can make up >90% of hits for words like "readme".
    f.push_str(" NOT System.ItemFolderPathDisplay:node_modules NOT System.ItemFolderPathDisplay:site-packages");
    Some(f)
}

fn search_inner(query: &str, limit: usize) -> Option<Vec<FileHit>> {
    if limit == 0 {
        return Some(Vec::new());
    }
    let filter = aqs(query)?;
    let start = Instant::now();
    let folder = profile_folder()?;
    let opts = QueryOptions::new().ok()?;
    opts.SetFolderDepth(FolderDepth::Deep).ok()?;
    opts.SetIndexerOption(IndexerOption::OnlyUseIndexerAndOptimizeForIndexedProperties).ok()?;
    if let Ok(so) = opts.SortOrder() {
        let _ = so.Clear();
        let _ = so.Append(&SortEntry { PropertyName: HSTRING::from("System.DateModified"), AscendingOrder: false });
    }
    opts.SetUserSearchFilter(&HSTRING::from(filter)).ok()?;
    let q = folder.CreateItemQueryWithOptions(&opts).ok()?;
    let remaining = BUDGET.saturating_sub(start.elapsed()).as_millis() as u64;
    // Over-fetch so the fuzzy re-rank has something to choose from.
    let fetch = (limit * 3).clamp(15, 60) as u32;
    let items = wait!(q.GetItemsAsync(0, fetch), remaining.max(1))?;
    let n = items.Size().ok()?;
    let mut hits: Vec<(FileHit, i32)> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let Ok(it) = items.GetAt(i) else { continue };
        let name = it.Name().map(|h| h.to_string()).unwrap_or_default();
        let path = it.Path().map(|h| h.to_string()).unwrap_or_default();
        if path.is_empty() || is_noise(&path) {
            continue;
        }
        let is_folder = it.IsOfType(StorageItemTypes::Folder).unwrap_or(false);
        let score = fuzzy_score(query, &name).unwrap_or(0);
        hits.push((FileHit { path, name, is_folder }, score));
    }
    // Stable sort: equal scores keep the indexer's most-recent-first order.
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    Some(hits.into_iter().take(limit).map(|(h, _)| h).collect())
}

/// Paths inside tooling/cache folders that are indexed but never what a user wants.
fn is_noise(path: &str) -> bool {
    const NOISE: [&str; 10] = [
        r"\node_modules\",
        r"\.git\",
        r"\appdata\",
        r"\site-packages\",
        r"\__pycache__\",
        r"\.gradle\",
        r"\.venv\",
        r"\.cache\",
        r"\.cargo\",
        r"\.rustup\",
    ];
    let p = path.to_lowercase();
    NOISE.iter().any(|n| p.contains(n))
}

#[cfg(test)]
mod tests {
    #[test]
    fn aqs_filter() {
        assert_eq!(super::aqs("  "), None);
        let ex = " NOT System.ItemFolderPathDisplay:node_modules NOT System.ItemFolderPathDisplay:site-packages";
        assert_eq!(super::aqs("read\"me"), Some(format!("System.FileName:read System.FileName:me{ex}")));
        assert_eq!(super::aqs("tax 2024"), Some(format!("System.FileName:tax System.FileName:2024{ex}")));
        assert_eq!(super::aqs("(x:*)"), Some(format!("System.FileName:x{ex}")));
        assert!(super::is_noise(r"C:\Users\x\proj\node_modules\a\README.md"));
        assert!(!super::is_noise(r"C:\Users\x\Documents\README.md"));
    }
}
