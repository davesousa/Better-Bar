//! What the user picked for what they typed, so search learns ("v" → VPN after picking VPN
//! once), like Spotlight and Raycast.
//!
//! Each pick stores the query, a stable key for the result ("app:…", "set:…") and when and
//! how often it was chosen. [`History::boost`] adds to a result's score when the user has
//! picked it before for the same query, for a longer query starting with what is typed now
//! (typed "v", picked VPN after typing "vpn"), or for a shorter one. Older picks count less
//! (half weight after 30 days). Pure data: saving and loading are the caller's.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    picks: Vec<Pick>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Pick {
    /// Lower-case, trimmed query.
    q: String,
    key: String,
    n: u32,
    /// Unix seconds of the last pick.
    t: u64,
}

/// Picks kept; the least recently used go first.
const MAX_PICKS: usize = 400;

fn norm(q: &str) -> String {
    q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

impl History {
    /// Remember that `key` was chosen after typing `query`.
    pub fn record(&mut self, query: &str, key: &str, now: u64) {
        let q = norm(query);
        if q.is_empty() || key.is_empty() {
            return;
        }
        match self.picks.iter_mut().find(|p| p.q == q && p.key == key) {
            Some(p) => {
                p.n = p.n.saturating_add(1);
                p.t = now;
            }
            None => self.picks.push(Pick { q, key: key.to_string(), n: 1, t: now }),
        }
        if self.picks.len() > MAX_PICKS {
            self.picks.sort_by_key(|p| std::cmp::Reverse(p.t));
            self.picks.truncate(MAX_PICKS);
        }
    }

    /// Extra score for result `key` while `query` is typed (0 when never picked).
    pub fn boost(&self, query: &str, key: &str, now: u64) -> i32 {
        let q = norm(query);
        if q.is_empty() {
            return 0;
        }
        let mut best = 0.0f32;
        for p in self.picks.iter().filter(|p| p.key == key) {
            let base = if p.q == q {
                2200.0
            } else if p.q.starts_with(&q) {
                // Typed less than before: strong when most of it is typed.
                1200.0 + 600.0 * (q.len() as f32 / p.q.len() as f32)
            } else if q.starts_with(&p.q) {
                // Typed more than before: it still matched, so it is probably still wanted.
                700.0
            } else {
                continue;
            };
            let days = now.saturating_sub(p.t) as f32 / 86_400.0;
            let recency = 0.5f32.powf(days / 30.0);
            let v = base * (0.5 + 0.5 * recency) + 80.0 * (p.n.min(10) as f32) * recency;
            best = best.max(v);
        }
        best.round() as i32
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.picks.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.picks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    #[test]
    fn exact_beats_prefix_beats_nothing() {
        let mut h = History::default();
        h.record("VPN", "set:vpn", 1000);
        let exact = h.boost("vpn", "set:vpn", 1000);
        let prefix = h.boost("v", "set:vpn", 1000);
        let longer = h.boost("vpn set", "set:vpn", 1000);
        assert!(exact > prefix && prefix > longer && longer > 0, "{exact} {prefix} {longer}");
        assert_eq!(h.boost("x", "set:vpn", 1000), 0);
        assert_eq!(h.boost("vpn", "app:other", 1000), 0);
    }

    #[test]
    fn repeats_and_recency() {
        let mut h = History::default();
        h.record("code", "app:a", 0);
        let once = h.boost("code", "app:a", 0);
        h.record("code", "app:a", 0);
        h.record("code", "app:a", 0);
        assert!(h.boost("code", "app:a", 0) > once);
        assert!(h.boost("code", "app:a", 90 * DAY) < h.boost("code", "app:a", 0));
    }

    #[test]
    fn keeps_a_bounded_list() {
        let mut h = History::default();
        for i in 0..(MAX_PICKS + 50) {
            h.record(&format!("q{i}"), "k", i as u64);
        }
        assert_eq!(h.picks.len(), MAX_PICKS);
        assert!(h.boost("q449", "k", 449) > 0, "newest kept");
        assert_eq!(h.boost("q0", "k", 449), 0, "oldest dropped");
    }

    #[test]
    fn round_trips_as_json() {
        let mut h = History::default();
        h.record("dark", "cmd:dark", 5);
        let back: History = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back, h);
    }
}
