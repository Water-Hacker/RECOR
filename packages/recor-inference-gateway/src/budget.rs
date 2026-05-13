//! Token-budget tracking. Atomic in-process counter; the metric on
//! top of it is emitted by the gateway's caller (we don't pull in a
//! Prometheus dep here — single-purpose crate principle).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct TokenBudget {
    /// Running totals per (purpose, model).
    per_label: Mutex<HashMap<(String, String), u64>>,
    /// Process-wide total.
    total: AtomicU64,
    /// Optional soft ceiling. The budget tracker does NOT enforce
    /// (the gateway doesn't refuse calls when over); callers expose
    /// the total via a Prometheus counter and humans get paged.
    ceiling: Option<u64>,
}

impl TokenBudget {
    pub fn new(ceiling: Option<u64>) -> Self {
        Self {
            per_label: Mutex::new(HashMap::new()),
            total: AtomicU64::new(0),
            ceiling,
        }
    }

    pub fn record(&self, purpose: &str, model: &str, tokens: u64) {
        let key = (purpose.to_string(), model.to_string());
        let mut guard = self.per_label.lock().unwrap();
        let entry = guard.entry(key).or_insert(0);
        *entry += tokens;
        self.total.fetch_add(tokens, Ordering::AcqRel);
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Acquire)
    }

    pub fn ceiling(&self) -> Option<u64> {
        self.ceiling
    }

    pub fn over_ceiling(&self) -> bool {
        match self.ceiling {
            Some(c) => self.total() > c,
            None => false,
        }
    }

    pub fn snapshot(&self) -> Vec<(String, String, u64)> {
        let guard = self.per_label.lock().unwrap();
        guard
            .iter()
            .map(|((p, m), v)| (p.clone(), m.clone(), *v))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_aggregates() {
        let b = TokenBudget::new(Some(100));
        b.record("adverse_media", "claude-opus-4-7", 30);
        b.record("adverse_media", "claude-opus-4-7", 40);
        b.record("pattern", "claude-haiku-4-5-20251001", 10);
        assert_eq!(b.total(), 80);
        let snap = b.snapshot();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn over_ceiling_check() {
        let b = TokenBudget::new(Some(10));
        b.record("p", "m", 20);
        assert!(b.over_ceiling());
        let b2 = TokenBudget::new(None);
        b2.record("p", "m", 9999);
        assert!(!b2.over_ceiling());
    }
}
