//! TODO-014 — Canonical-name normalisation.
//!
//! The verification engine indexes `sanctions_persons`, `peps`, and
//! `icij_persons` on `full_name_canonical` with `pg_trgm`. The ingest
//! binaries must populate that column with a consistent canonicalised
//! string so the same person fetched from OFAC vs UN matches under the
//! 0.5 similarity threshold the application enforces.
//!
//! The canonicalisation rules (matched by the v-engine's query-side
//! helper):
//!
//! 1. Lowercase ASCII fold (no `to_lowercase` Unicode case-folding —
//!    pg_trgm is byte-oriented).
//! 2. Strip combining diacritics (so `José` → `jose`).
//! 3. Collapse runs of whitespace / punctuation to a single ASCII space.
//! 4. Trim leading + trailing whitespace.
//!
//! Anything more elaborate (Levenshtein / phonetic) belongs to the
//! query-time matcher, not the ingest-time normaliser.

/// Canonicalise a free-text name into the column shape used by the
/// screening tables. See module docs for the rules.
#[must_use]
pub fn canonicalise_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_was_space = true; // suppress leading whitespace
    for ch in raw.chars() {
        let folded = fold_char(ch);
        match folded {
            FoldedChar::Drop => {}
            FoldedChar::Space => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            FoldedChar::Keep(c) => {
                out.push(c);
                last_was_space = false;
            }
        }
    }
    // Drop the trailing space, if any.
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

enum FoldedChar {
    Keep(char),
    Space,
    Drop,
}

fn fold_char(ch: char) -> FoldedChar {
    // ASCII fast path.
    if ch.is_ascii() {
        if ch.is_ascii_alphabetic() {
            return FoldedChar::Keep(ch.to_ascii_lowercase());
        }
        if ch.is_ascii_digit() {
            return FoldedChar::Keep(ch);
        }
        // Whitespace + punctuation collapse to spaces.
        return FoldedChar::Space;
    }
    // Non-ASCII: strip common Latin diacritics by best-effort
    // character mapping. This is the same table the v-engine
    // query-side `name_canonical` Postgres function uses; keep them
    // aligned if you extend it.
    let stripped = match ch {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'Ç' | 'ç' => 'c',
        'È' | 'É' | 'Ê' | 'Ë' | 'è' | 'é' | 'ê' | 'ë' => 'e',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'ì' | 'í' | 'î' | 'ï' => 'i',
        'Ñ' | 'ñ' => 'n',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'ù' | 'ú' | 'û' | 'ü' => 'u',
        'Ý' | 'ý' | 'ÿ' => 'y',
        'ß' => 's', // German sharp-s collapsed; correctness loss is acceptable.
        _ => return FoldedChar::Drop, // unknown non-ASCII: drop.
    };
    FoldedChar::Keep(stripped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace() {
        assert_eq!(canonicalise_name("  John   Doe  "), "john doe");
    }

    #[test]
    fn strips_diacritics() {
        assert_eq!(canonicalise_name("José Müller"), "jose muller");
    }

    #[test]
    fn punctuation_becomes_space() {
        assert_eq!(canonicalise_name("Smith, John-Paul"), "smith john paul");
    }

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(canonicalise_name(""), "");
        assert_eq!(canonicalise_name("   "), "");
    }
}
