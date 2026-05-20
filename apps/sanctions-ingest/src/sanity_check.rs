//! TODO-014 — Sanity check on dataset deltas.
//!
//! The classic operational failure mode of an external sanctions feed
//! is an upstream parsing error that yields an empty XML file. If the
//! ingest worker upserts that empty set, the platform loses its
//! ability to screen against the source. The platform's defence is
//! the % drop check below: if the new row count is below
//! `(1 - max_drop_ratio) * prior_row_count`, refuse the apply.
//!
//! Operators who genuinely have a small upstream feed (the EU CFSP
//! is occasionally < 100 rows) override with `--force`; the override
//! writes a justification into `ingest_log`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SanityCheckOutcome {
    /// New row count is acceptable — apply the delta.
    Pass,
    /// Row count dropped more than `max_drop_ratio` since the prior
    /// revision. Refuse the apply unless `--force` was passed.
    Blocked {
        prior: u64,
        proposed: u64,
        max_drop_ratio: f64,
    },
}

/// Sanity-check the proposed row count against the prior count.
///
/// `max_drop_ratio` is a value in `[0.0, 1.0]`; `0.25` means "block
/// when more than 25% of rows would drop". The check is one-sided —
/// growth never blocks.
///
/// When `prior == 0` (first ingestion of this source), the check
/// always passes — the operator's choice to seed the table is by
/// definition correct.
#[must_use]
pub fn sanity_check(prior: u64, proposed: u64, max_drop_ratio: f64) -> SanityCheckOutcome {
    if prior == 0 {
        return SanityCheckOutcome::Pass;
    }
    if proposed >= prior {
        return SanityCheckOutcome::Pass;
    }
    let drop = (prior - proposed) as f64;
    let drop_ratio = drop / prior as f64;
    if drop_ratio > max_drop_ratio {
        SanityCheckOutcome::Blocked {
            prior,
            proposed,
            max_drop_ratio,
        }
    } else {
        SanityCheckOutcome::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_ingestion_always_passes() {
        assert!(matches!(
            sanity_check(0, 5000, 0.25),
            SanityCheckOutcome::Pass
        ));
    }

    #[test]
    fn growth_passes_unconditionally() {
        assert!(matches!(
            sanity_check(1000, 1500, 0.25),
            SanityCheckOutcome::Pass
        ));
    }

    #[test]
    fn small_drop_passes() {
        // 1000 → 800 = 20% drop, threshold 25% → pass.
        assert!(matches!(
            sanity_check(1000, 800, 0.25),
            SanityCheckOutcome::Pass
        ));
    }

    #[test]
    fn large_drop_blocks() {
        // 1000 → 700 = 30% drop, threshold 25% → block.
        let out = sanity_check(1000, 700, 0.25);
        assert!(matches!(
            out,
            SanityCheckOutcome::Blocked { prior: 1000, proposed: 700, .. }
        ));
    }

    #[test]
    fn empty_feed_blocks_when_prior_nonempty() {
        let out = sanity_check(1, 0, 0.25);
        assert!(matches!(out, SanityCheckOutcome::Blocked { .. }));
    }
}
