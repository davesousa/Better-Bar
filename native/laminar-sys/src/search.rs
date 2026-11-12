//! Fuzzy matching for the launcher (pure Rust, no Windows APIs).
//!
//! Ranking tiers (higher is better, tiers never overlap):
//! 1. prefix of the whole text         2001..=3000 (exact match = 3000, shorter text wins)
//! 2. match at the start of a word     1001..=2000 (earlier word, shorter text win);
//!    also multi-token queries whose tokens each start a word, in order ("vis co")
//! 3. plain substring                   501..=1000 (earlier position wins)
//! 4. subsequence                         1..=500  (consecutive runs, word-start hits
//!    such as initials "vsc" → "Visual Studio Code", and few gaps win)
//!
//! Matching is case-insensitive. Word boundaries are space `-` `_` `.` `\` `/` `&` `(` `)`
//! plus lower→upper camelCase transitions ("OneDrive" has a word starting at "Drive").

use crate::apps::AppEntry;

const SEPARATORS: [char; 9] = [' ', '-', '_', '.', '\\', '/', '&', '(', ')'];

/// Lower-cased chars of `s`, each flagged with whether it starts a word.
fn analyze(s: &str) -> (Vec<char>, Vec<bool>) {
    let mut chars = Vec::with_capacity(s.len());
    let mut starts = Vec::with_capacity(s.len());
    let mut prev: Option<char> = None;
    for c in s.chars() {
        let is_start = match prev {
            None => true,
            Some(p) => (SEPARATORS.contains(&p) && !SEPARATORS.contains(&c)) || (p.is_lowercase() && c.is_uppercase()),
        };
        for (k, lc) in c.to_lowercase().enumerate() {
            chars.push(lc);
            starts.push(is_start && k == 0);
        }
        prev = Some(c);
    }
    (chars, starts)
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// Scores `text` against `query`; `None` when the query's characters do not all appear
/// in order. An empty (or whitespace) query matches everything with score 0.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }
    let (t, starts) = analyze(text);
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let len_diff = (t.len() as i32 - q.len() as i32).max(0);

    // 1. Prefix.
    if t.starts_with(&q) {
        return Some(3000 - len_diff.min(999));
    }
    // 2. Word start (single token)...
    let mut word_idx = 0i32;
    for i in 1..t.len() {
        if starts[i] {
            word_idx += 1;
            if t[i..].starts_with(&q) {
                return Some(2000 - (word_idx * 20 + len_diff).min(999));
            }
        }
    }
    // ...or every whitespace-separated token starts a word, in order.
    let tokens: Vec<Vec<char>> = query.split_whitespace().map(|w| w.chars().flat_map(|c| c.to_lowercase()).collect()).collect();
    if tokens.len() > 1 {
        let mut pos = 0usize;
        let mut ok = true;
        let mut first_word = 0i32;
        for (k, tok) in tokens.iter().enumerate() {
            match (pos..t.len()).find(|&i| starts[i] && t[i..].starts_with(tok)) {
                Some(i) => {
                    if k == 0 {
                        first_word = starts[..=i].iter().filter(|&&s| s).count() as i32 - 1;
                    }
                    pos = i + tok.len();
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Some(2000 - (first_word * 20 + len_diff + 50).min(999));
        }
    }
    // 3. Substring.
    if let Some(p) = find(&t, &q, 0) {
        return Some(1000 - (p as i32 * 5 + len_diff).min(499));
    }
    // 4. Subsequence (whitespace in the query is ignored).
    let qs: Vec<char> = q.iter().copied().filter(|c| !c.is_whitespace()).collect();
    subsequence(&qs, &t, &starts).map(|raw| raw.clamp(1, 500))
}

/// DP over (query index, text index): best score with query[i] matched at text[j].
/// Per-match base score + word-start bonus, +consecutive bonus, -1 per skipped char.
fn subsequence(q: &[char], t: &[char], starts: &[bool]) -> Option<i32> {
    const NEG: i32 = i32::MIN / 4;
    let (m, n) = (q.len(), t.len());
    if m == 0 || m > n {
        return None;
    }
    let base = |j: usize| 10 + if starts[j] { 12 } else { 0 };
    let mut prev = vec![NEG; n];
    for j in 0..n {
        if t[j] == q[0] {
            prev[j] = base(j) - (j as i32).min(20) / 2;
        }
    }
    for &qc in q.iter().skip(1) {
        let mut cur = vec![NEG; n];
        // running = max over k < j-1 of prev[k] - (j-k-1), maintained incrementally.
        let mut running = NEG;
        for j in 1..n {
            if j >= 2 {
                running = running.saturating_sub(1).max(prev[j - 2].saturating_sub(1));
            }
            if t[j] == qc {
                let consec = if prev[j - 1] > NEG { prev[j - 1] + 8 } else { NEG };
                let best = consec.max(running);
                if best > NEG {
                    cur[j] = best + base(j);
                }
            }
        }
        prev = cur;
    }
    let best = prev.into_iter().max().filter(|&v| v > NEG)?;
    Some(best - ((n - m) as i32) / 4)
}

/// Scores every item's `name` against `query` and returns the best `limit` matches,
/// highest score first (ties: alphabetical). An empty query returns the first `limit`
/// items in their original order with score 0.
pub fn rank<'a>(query: &str, items: &'a [AppEntry], limit: usize) -> Vec<(&'a AppEntry, i32)> {
    let mut v: Vec<(&AppEntry, i32)> = items.iter().filter_map(|a| fuzzy_score(query, &a.name).map(|s| (a, s))).collect();
    if !query.trim().is_empty() {
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase())));
    }
    v.truncate(limit);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, t: &str) -> i32 {
        fuzzy_score(q, t).unwrap_or(-1)
    }

    #[test]
    fn no_match() {
        assert_eq!(fuzzy_score("xyz", "Calculator"), None);
        assert_eq!(fuzzy_score("calcx", "Calculator"), None);
        assert_eq!(fuzzy_score("long query", "short"), None);
    }

    #[test]
    fn empty_query() {
        assert_eq!(fuzzy_score("", "Anything"), Some(0));
        assert_eq!(fuzzy_score("   ", "Anything"), Some(0));
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(s("CALC", "calculator"), s("calc", "Calculator"));
        assert!(s("ÉDIT", "édition") > 2000);
    }

    #[test]
    fn exact_and_prefix() {
        assert_eq!(s("calculator", "Calculator"), 3000);
        assert!(s("cal", "Calendar") > 2000);
        // shorter text wins among prefixes
        assert!(s("cal", "Calendar") > s("cal", "Calculator Plus Pro"));
    }

    #[test]
    fn tier_order() {
        let prefix = s("term", "Terminal");
        let word = s("term", "Windows Terminal");
        let sub = s("term", "Determination");
        let subseq = s("term", "The Exam Room");
        assert!(prefix > word, "{prefix} {word}");
        assert!(word > sub, "{word} {sub}");
        assert!(sub > subseq, "{sub} {subseq}");
        assert!(subseq > 0);
    }

    #[test]
    fn word_boundaries() {
        for t in ["Foo-Bar", "foo_bar", "foo.bar", "foo\\bar", "Foo & Bar", "Foo Bar", "FooBar"] {
            assert!(s("bar", t) > 1000 && s("bar", t) <= 2000, "{t}: {}", s("bar", t));
        }
        // earlier word beats later word
        assert!(s("st", "Visual Studio Code") > s("st", "Microsoft Visual Studio"));
    }

    #[test]
    fn multi_token_word_starts() {
        let v = s("vis co", "Visual Studio Code");
        assert!(v > 1000 && v <= 2000, "{v}");
    }

    #[test]
    fn subsequence_initials_and_runs() {
        let vsc = s("vsc", "Visual Studio Code");
        assert!(vsc > 0 && vsc <= 500);
        // initials beat scattered mid-word hits
        assert!(vsc > s("vsc", "Movies Scanner"), "{vsc} vs {}", s("vsc", "Movies Scanner"));
        // consecutive runs beat scattered letters
        assert!(s("abcd", "abxcd") > s("abcd", "axbxcxd"));
        assert!(s("cltr", "Calculator") > 0);
    }

    #[test]
    fn rank_orders_and_limits() {
        let items: Vec<AppEntry> = ["Calculator", "Calendar", "Google Chrome", "Excel", "Windows Calculator Pro", "Recall"]
            .iter()
            .map(|n| AppEntry { id: n.to_string(), name: n.to_string() })
            .collect();
        let r = rank("cal", &items, 3);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].0.name, "Calendar"); // shorter prefix match first
        assert_eq!(r[1].0.name, "Calculator");
        assert_eq!(r[2].0.name, "Windows Calculator Pro"); // word start beats substring "Recall"
        let all = rank("", &items, 10);
        assert_eq!(all.len(), items.len());
        assert_eq!(all[0].0.name, "Calculator");
        assert!(rank("zzz", &items, 10).is_empty());
    }
}
