//! Model-tier pinning (D22).
//!
//! The gateway never accepts a free-form model string from callers.
//! Callers ask for a `Tier`; the gateway maps to the wire-level model
//! identifier. Centralising this in one place means a model-version
//! bump is one PR, not a search across the codebase, and the tier
//! abstraction keeps the BPA-calibration story decoupled from any
//! single concrete model.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Tier A — flagship reasoning model. Use for high-stakes calls
    /// where accuracy matters more than latency / cost.
    A,
    /// Tier B — fast, cheap. Use for low-stakes summarisation /
    /// extraction work where the answer is structurally constrained.
    B,
}

impl Tier {
    /// Wire-level model identifier.
    pub fn model_id(self) -> &'static str {
        match self {
            // Pinned per the brief. The version is part of the
            // identifier so an inadvertent upstream change doesn't
            // silently alter our calibration.
            Self::A => "claude-opus-4-7",
            Self::B => "claude-haiku-4-5-20251001",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_a_pins_to_opus_4_7() {
        assert_eq!(Tier::A.model_id(), "claude-opus-4-7");
    }

    #[test]
    fn tier_b_pins_to_haiku_4_5() {
        assert_eq!(Tier::B.model_id(), "claude-haiku-4-5-20251001");
    }
}
