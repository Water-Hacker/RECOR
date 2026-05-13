//! Canonical-payload BLAKE3 hashing for receipt-hash re-derivation.
//!
//! The Declaration service computes the receipt hash as BLAKE3-256 over
//! the canonical-JSON form of the declaration body. We re-derive the
//! same bytes here and compare to the on-chain entry.
//!
//! **Canonical form**: the JSON object with keys sorted ascending and
//! no extraneous whitespace, encoded as UTF-8. This is the form the
//! Declaration service uses (see services/declaration/src/domain/
//! canonical.rs in the source service). Re-implementing it here keeps
//! the verifier dependency-free of the declaration crate (the verifier
//! is a downstream consumer; tight coupling would make it brittle to
//! Declaration's internal changes).

use serde_json::Value as JsonValue;

/// Re-derive the BLAKE3-256 hex digest from a canonical JSON value.
/// The value is canonicalised (keys sorted, no whitespace) before
/// hashing; the output is 64 lowercase hex characters.
pub fn derive_receipt_hash(payload: &JsonValue) -> String {
    let canonical = canonicalise(payload);
    let digest = blake3::hash(canonical.as_bytes());
    hex::encode(digest.as_bytes())
}

/// Stable JSON serialisation: sort object keys recursively.
fn canonicalise(value: &JsonValue) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Number(n) => out.push_str(&n.to_string()),
        JsonValue::String(s) => {
            // serde_json::to_string handles the JSON-string escaping rules.
            out.push_str(&serde_json::to_string(s).expect("string serialise"));
        }
        JsonValue::Array(arr) => {
            out.push('[');
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(v, out);
            }
            out.push(']');
        }
        JsonValue::Object(obj) => {
            out.push('{');
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("key serialise"));
                out.push(':');
                write_canonical(&obj[*k], out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deterministic_across_key_order() {
        let a = json!({"a": 1, "b": 2});
        let b = json!({"b": 2, "a": 1});
        assert_eq!(derive_receipt_hash(&a), derive_receipt_hash(&b));
    }

    #[test]
    fn different_payload_yields_different_hash() {
        let a = json!({"a": 1});
        let b = json!({"a": 2});
        assert_ne!(derive_receipt_hash(&a), derive_receipt_hash(&b));
    }

    #[test]
    fn hash_is_64_hex_chars() {
        let h = derive_receipt_hash(&json!({"x": "y"}));
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn nested_objects_are_canonicalised_recursively() {
        let a = json!({"outer": {"a": 1, "b": 2}});
        let b = json!({"outer": {"b": 2, "a": 1}});
        assert_eq!(derive_receipt_hash(&a), derive_receipt_hash(&b));
    }

    #[test]
    fn arrays_preserve_order() {
        let a = json!([1, 2, 3]);
        let b = json!([3, 2, 1]);
        assert_ne!(derive_receipt_hash(&a), derive_receipt_hash(&b));
    }
}
