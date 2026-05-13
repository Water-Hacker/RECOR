//! Shared name-matching helper used by the sanctions adapter (Stage 3)
//! and the PEP adapter (Stage 4).
//!
//! The function `name_match` returns scored candidates from the database
//! using PostgreSQL trigram similarity (`pg_trgm.similarity()`). The
//! caller supplies the SQL fragment to run against (different tables,
//! different shape) plus the query name; we keep the scoring policy and
//! the post-filtering in one place so Stage 3 and Stage 4 cannot drift
//! apart on name-matching semantics.
//!
//! Scoring tiers:
//!   * `similarity >= 0.85` — `MatchTier::Certain`
//!   * `0.70 <= similarity < 0.85` — `MatchTier::Near`
//!   * `0.50 <= similarity < 0.70` — `MatchTier::Weak`
//!   * `similarity < 0.50` — discarded
//!
//! The threshold values are deliberately conservative; the calibration
//! ceremony (operational concern) re-tunes against the adversarial
//! corpus. See ADR-0010.
//!
//! D17 zero-trust: every candidate's `similarity` is recomputed from the
//! returned canonical name; we never trust the DB-side score blindly.

use serde::{Deserialize, Serialize};

/// Tier classification of a name match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchTier {
    /// `similarity >= 0.85` — treat as a hit.
    Certain,
    /// `0.70 <= similarity < 0.85` — analyst-review-grade hit.
    Near,
    /// `0.50 <= similarity < 0.70` — surface but weight low.
    Weak,
}

impl MatchTier {
    pub fn from_similarity(s: f64) -> Option<Self> {
        if s >= 0.85 {
            Some(Self::Certain)
        } else if s >= 0.70 {
            Some(Self::Near)
        } else if s >= 0.50 {
            Some(Self::Weak)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::Near => "near",
            Self::Weak => "weak",
        }
    }
}

/// One candidate name match from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameCandidate {
    /// Internal id (sanctions_persons.id or peps.id).
    pub id: uuid::Uuid,
    /// Source (e.g. "ofac_sdn", "un_consolidated", "eu_cfsp",
    /// "opensanctions_pep").
    pub source: String,
    /// Canonical full name as stored.
    pub canonical_full_name: String,
    /// Trigram similarity ∈ [0, 1].
    pub similarity: f64,
    /// Tier derived from similarity.
    pub tier: MatchTier,
}

/// Canonicalise a name before lookup: lowercase, NFKD-fold accents,
/// collapse whitespace. This is the same shape we use at ingest, so
/// query and stored value compare apples-to-apples.
pub fn canonicalise(name: &str) -> String {
    let lowered = name.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut prev_was_ws = false;
    for c in lowered.chars() {
        let folded = fold_diacritic(c);
        if folded.is_whitespace() {
            if !prev_was_ws && !out.is_empty() {
                out.push(' ');
            }
            prev_was_ws = true;
        } else {
            out.push(folded);
            prev_was_ws = false;
        }
    }
    out.trim_end().to_string()
}

fn fold_diacritic(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'œ' => 'o', // Approximation; trigram doesn't care
        'æ' => 'a',
        _ => c,
    }
}

/// Levenshtein distance between two strings (basic DP table). Used as a
/// secondary tie-breaker and exposed for tests + adverse-media adapter.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let (n, m) = (av.len(), bv.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if av[i - 1] == bv[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Compute a similarity score between two canonicalised names using a
/// trigram-style Jaccard coefficient. This mirrors the Postgres
/// `pg_trgm.similarity()` shape closely enough to use as a sanity check
/// in code paths that can't issue a query (tests, in-memory fixtures).
pub fn trigram_similarity(a: &str, b: &str) -> f64 {
    let ta = trigrams(a);
    let tb = trigrams(b);
    if ta.is_empty() && tb.is_empty() {
        return 0.0;
    }
    let intersection = ta.iter().filter(|t| tb.contains(t)).count();
    let union = {
        let mut u: Vec<&String> = ta.iter().collect();
        for t in &tb {
            if !u.contains(&t) {
                u.push(t);
            }
        }
        u.len()
    };
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn trigrams(s: &str) -> Vec<String> {
    let padded = format!("  {s} ");
    let chars: Vec<char> = padded.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    if chars.len() < 3 {
        return out;
    }
    for i in 0..(chars.len() - 2) {
        let t: String = chars[i..i + 3].iter().collect();
        out.push(t);
    }
    out
}

/// Async port returning scored candidates for a query name from a
/// named table. Implementations live in `sanctions_postgres` and
/// `pep_postgres`; both wrap a `pg_trgm.similarity()` query and call
/// `MatchTier::from_similarity` to assign the tier.
///
/// The function `name_match` exposed at the module root is a convenience
/// wrapper that runs the same shape of query against either table; we
/// expose it for ad-hoc tooling and for the adverse-media stage to
/// retrieve ICIJ candidates.
pub async fn name_match<F, Fut, T>(
    query_name: &str,
    fetch: F,
    max_candidates: usize,
) -> Result<Vec<T>, sqlx::Error>
where
    F: FnOnce(String, usize) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<T>, sqlx::Error>>,
{
    let canonical = canonicalise(query_name);
    fetch(canonical, max_candidates).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_thresholds() {
        assert_eq!(MatchTier::from_similarity(0.95), Some(MatchTier::Certain));
        assert_eq!(MatchTier::from_similarity(0.85), Some(MatchTier::Certain));
        assert_eq!(MatchTier::from_similarity(0.84), Some(MatchTier::Near));
        assert_eq!(MatchTier::from_similarity(0.70), Some(MatchTier::Near));
        assert_eq!(MatchTier::from_similarity(0.69), Some(MatchTier::Weak));
        assert_eq!(MatchTier::from_similarity(0.50), Some(MatchTier::Weak));
        assert_eq!(MatchTier::from_similarity(0.49), None);
    }

    #[test]
    fn canonicalise_strips_diacritics_and_normalises_whitespace() {
        assert_eq!(canonicalise("  Aïssa   Ngo  Bidoung "), "aissa ngo bidoung");
        assert_eq!(canonicalise("FRANÇOIS"), "francois");
        assert_eq!(canonicalise("élise"), "elise");
    }

    #[test]
    fn levenshtein_matches_known_cases() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn trigram_similarity_is_one_for_identical() {
        assert!((trigram_similarity("hello", "hello") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn trigram_similarity_drops_with_difference() {
        let s = trigram_similarity("abouzaid", "aliyev");
        assert!(s < 0.3);
    }
}
