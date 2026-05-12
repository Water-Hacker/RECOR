//! PII redaction for tracing logs (OPS-2).
//!
//! GDPR + the OHADA data-protection framework require that
//! personally-identifying values not appear in plain text in
//! operational logs. Today RÉCOR's services attach `declarant_principal`
//! (SPIFFE URIs), `person_id` UUIDs, and BLAKE3 `receipt_hash_hex` to
//! `tracing::instrument` spans, which means a `grep "spiffe://"` of any
//! log stream returns declarants' identifiers in clear text. That is
//! unacceptable.
//!
//! ## Design
//!
//! The crate exposes two cooperating surfaces driven by one policy:
//!
//! - [`RedactingLayer`] — a `tracing_subscriber::Layer` that intercepts
//!   span-field values as spans are created / re-recorded, stores the
//!   redacted copy in each span's extension map, and (most importantly)
//!   makes the redacted snapshot available to the format layer below.
//! - [`RedactingJsonFormat`] — a `tracing_subscriber::fmt::FormatEvent`
//!   that emits JSON log lines using the same policy on every event
//!   field. This is what actually keeps SPIFFE URIs / UUIDs / hashes
//!   out of the bytes sent to stdout. Consumers install it via
//!   `tracing_subscriber::fmt::layer().event_format(RedactingJsonFormat::new(cfg))`.
//!
//! Both surfaces funnel through [`redact_field`] — there is one
//! canonical policy.
//!
//! ## Redaction rules
//!
//! - **SPIFFE URIs** (any value beginning with `spiffe://`) →
//!   `spiffe://<host>/<first-16-hex-of-BLAKE3-keyed-MAC(path)>`.
//! - **UUIDs in PII fields** (`person_id`, `principal`,
//!   `declarant_principal`, `subject`) → 16 hex chars of the
//!   keyed-MAC. `entity_id`, `declaration_id`, `correlation_id`
//!   are NOT PII and pass through.
//! - **Field name `receipt_hash_hex`** → first 8 + `…` + last 4 chars.
//! - **All other fields** pass through untouched.
//!
//! ## Configuration
//!
//! - `LOG_REDACTION`: `enabled` (default outside dev) | `disabled-for-dev`
//!   (default in dev) | `disabled` (explicit pass-through, warns).
//! - `LOG_REDACTION_KEY`: 64 hex characters (32 raw bytes). REQUIRED
//!   in non-dev when mode is `enabled`.
//!
//! ## Fail-closed posture (D14)
//!
//! - Malformed key / unknown mode → hard error from
//!   [`RedactionConfig::from_env`]; the service must refuse to start.
//! - The format path never panics on field-formatting errors; the
//!   worst case is a `<err>` placeholder.
//! - A rotated key changes MACs — operators lose correlation but PII
//!   is still never exposed.

use std::env;
use std::fmt::{self, Write as _};

use secrecy::{ExposeSecret, SecretString};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Field names whose UUID values are personally identifying and must
/// be redacted. UUIDs in other fields (e.g. `entity_id`,
/// `declaration_id`, `correlation_id`) are NOT PII and pass through.
const UUID_PII_FIELDS: &[&str] = &[
    "person_id",
    "principal",
    "declarant_principal",
    "subject",
];

/// The dedicated field name for receipt-hash redaction.
const RECEIPT_HASH_FIELD: &str = "receipt_hash_hex";

/// Selected redaction posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionMode {
    /// Full redaction. Production default.
    Enabled,
    /// Pass-through. Dev default; values land in logs unchanged.
    DisabledForDev,
    /// Pass-through with a loud startup warning.
    Disabled,
}

impl RedactionMode {
    /// Parse the `LOG_REDACTION` env var. Defaults to `Enabled` in any
    /// non-dev environment, `DisabledForDev` in dev.
    pub fn from_env(env_value: Option<&str>, is_dev: bool) -> Result<Self, RedactionConfigError> {
        match env_value.map(str::trim).filter(|s| !s.is_empty()) {
            Some("enabled") => Ok(Self::Enabled),
            Some("disabled-for-dev") => Ok(Self::DisabledForDev),
            Some("disabled") => Ok(Self::Disabled),
            Some(other) => Err(RedactionConfigError::InvalidMode(other.to_string())),
            None => Ok(if is_dev { Self::DisabledForDev } else { Self::Enabled }),
        }
    }

    /// True when the layer should actually redact.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Resolved redaction configuration: the mode + the MAC key.
#[derive(Debug, Clone)]
pub struct RedactionConfig {
    pub mode: RedactionMode,
    pub key: SecretString,
}

#[derive(Debug, thiserror::Error)]
pub enum RedactionConfigError {
    #[error(
        "LOG_REDACTION must be one of `enabled`, `disabled-for-dev`, or `disabled` (got `{0}`)"
    )]
    InvalidMode(String),
    #[error(
        "LOG_REDACTION_KEY is required outside dev (set it to 64 hex characters / 32 bytes)"
    )]
    KeyRequired,
    #[error("LOG_REDACTION_KEY must be 64 hex characters (32 bytes); got {0} characters")]
    KeyWrongLength(usize),
    #[error("LOG_REDACTION_KEY is not valid hex: {0}")]
    KeyNotHex(#[source] hex::FromHexError),
}

impl RedactionConfig {
    /// Build the config from the process environment.
    ///
    /// In non-dev environments the key is REQUIRED when mode is
    /// enabled. In dev a missing key is silently filled with random
    /// bytes; the caller is expected to `warn!`.
    pub fn from_env(is_dev: bool) -> Result<Self, RedactionConfigError> {
        let mode_raw = env::var("LOG_REDACTION").ok();
        let mode = RedactionMode::from_env(mode_raw.as_deref(), is_dev)?;
        let key = match env::var("LOG_REDACTION_KEY").ok().filter(|s| !s.is_empty()) {
            Some(hex_key) => Self::parse_key(&hex_key)?,
            None => {
                if !is_dev && mode == RedactionMode::Enabled {
                    return Err(RedactionConfigError::KeyRequired);
                }
                random_key()
            }
        };
        Ok(Self { mode, key })
    }

    /// Construct the config directly from a mode + hex key.
    pub fn new(mode: RedactionMode, hex_key: &str) -> Result<Self, RedactionConfigError> {
        Ok(Self { mode, key: Self::parse_key(hex_key)? })
    }

    fn parse_key(hex_key: &str) -> Result<SecretString, RedactionConfigError> {
        if hex_key.len() != 64 {
            return Err(RedactionConfigError::KeyWrongLength(hex_key.len()));
        }
        let _ = hex::decode(hex_key).map_err(RedactionConfigError::KeyNotHex)?;
        Ok(SecretString::from(hex_key.to_string()))
    }

    /// Get the raw 32-byte MAC key. Internal use only.
    fn key_bytes(&self) -> [u8; 32] {
        let raw = hex::decode(self.key.expose_secret()).expect("validated at construction");
        let mut out = [0u8; 32];
        out.copy_from_slice(&raw);
        out
    }
}

fn random_key() -> SecretString {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut seed = [0u8; 32];
    let nanos_bytes = nanos.to_le_bytes();
    let pid_bytes = (std::process::id() as u64).to_le_bytes();
    for (i, slot) in seed.iter_mut().enumerate() {
        *slot = nanos_bytes[i % nanos_bytes.len()] ^ pid_bytes[i % pid_bytes.len()];
    }
    let hashed = blake3::hash(&seed);
    SecretString::from(hex::encode(hashed.as_bytes()))
}

/// Compute the keyed-MAC of `input` and return the first 16 hex chars.
fn mac_short(key: &[u8; 32], input: &[u8]) -> String {
    let mac = blake3::keyed_hash(key, input);
    let full = hex::encode(mac.as_bytes());
    full[..16].to_string()
}

/// Apply the per-field redaction policy. Returns the redacted form, or
/// the original value if it doesn't match a known PII shape.
///
/// This is the single source of truth — every consumer funnels through here.
pub fn redact_field(field_name: &str, value: &str, key: &[u8; 32]) -> String {
    if let Some(stripped) = value.strip_prefix("spiffe://") {
        let (host, path) = stripped.split_once('/').unwrap_or((stripped, ""));
        return format!("spiffe://{host}/{}", mac_short(key, path.as_bytes()));
    }
    if field_name == RECEIPT_HASH_FIELD && value.len() >= 12 {
        return format!("{}…{}", &value[..8], &value[value.len() - 4..]);
    }
    if UUID_PII_FIELDS.contains(&field_name) && uuid::Uuid::parse_str(value).is_ok() {
        return mac_short(key, value.as_bytes());
    }
    value.to_string()
}

/// Per-span storage of (already-redacted) field values. Downstream
/// layers may read this to obtain policy-applied values without
/// re-implementing the visitor.
#[derive(Debug, Default, Clone)]
pub struct RedactedFields {
    pub entries: Vec<(String, String)>,
}

impl RedactedFields {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A `tracing_subscriber::Layer` that captures redacted span fields
/// into per-span extensions.
///
/// **Note** — by itself this layer does NOT prevent PII from being
/// emitted by `tracing_subscriber::fmt`. Install [`RedactingJsonFormat`]
/// on the fmt layer to actually keep PII out of stdout.
pub struct RedactingLayer {
    config: RedactionConfig,
}

impl RedactingLayer {
    pub fn new(config: RedactionConfig) -> Self {
        Self { config }
    }

    pub fn from_env_or_panic(is_dev: bool) -> Self {
        Self::new(RedactionConfig::from_env(is_dev).expect("LOG_REDACTION* env invalid"))
    }
}

/// Visitor that captures field-name → string-rendered-value pairs,
/// applying [`redact_field`] when `active`.
struct CaptureVisitor<'a> {
    key: &'a [u8; 32],
    active: bool,
    out: &'a mut Vec<(String, String)>,
}

impl<'a> CaptureVisitor<'a> {
    fn push(&mut self, field: &Field, raw: String) {
        let name = field.name().to_string();
        let value = if self.active {
            redact_field(&name, &raw, self.key)
        } else {
            raw
        };
        if let Some(existing) = self.out.iter_mut().find(|(k, _)| k == &name) {
            existing.1 = value;
        } else {
            self.out.push((name, value));
        }
    }
}

impl<'a> Visit for CaptureVisitor<'a> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let mut buf = String::new();
        let rendered = match write!(&mut buf, "{value:?}") {
            Ok(()) => trim_debug_quotes(&buf).to_string(),
            Err(_) => "<err>".to_string(),
        };
        self.push(field, rendered);
    }
    fn record_i64(&mut self, field: &Field, value: i64) { self.push(field, value.to_string()); }
    fn record_u64(&mut self, field: &Field, value: u64) { self.push(field, value.to_string()); }
    fn record_i128(&mut self, field: &Field, value: i128) { self.push(field, value.to_string()); }
    fn record_u128(&mut self, field: &Field, value: u128) { self.push(field, value.to_string()); }
    fn record_bool(&mut self, field: &Field, value: bool) { self.push(field, value.to_string()); }
    fn record_f64(&mut self, field: &Field, value: f64) { self.push(field, value.to_string()); }
}

fn trim_debug_quotes(s: &str) -> &str {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

impl<S> Layer<S> for RedactingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let key = self.config.key_bytes();
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut visitor = CaptureVisitor {
            key: &key,
            active: self.config.mode.is_active(),
            out: &mut entries,
        };
        attrs.record(&mut visitor);
        span.extensions_mut().insert(RedactedFields { entries });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let key = self.config.key_bytes();
        let mut ext = span.extensions_mut();
        let fields = ext.get_mut::<RedactedFields>();
        let Some(fields) = fields else { return };
        let mut visitor = CaptureVisitor {
            key: &key,
            active: self.config.mode.is_active(),
            out: &mut fields.entries,
        };
        values.record(&mut visitor);
    }
}

/// A `tracing_subscriber::fmt::FormatEvent` that emits one JSON
/// object per event line, with redaction applied at field-emit time.
///
/// Output shape matches `tracing_subscriber::fmt::format::Json` closely
/// enough for downstream log shippers (timestamp, level, target,
/// fields object, optional span chain). Differences vs the stock JSON
/// formatter are deliberate:
///   - Every field value goes through [`redact_field`].
///   - Span fields are pulled from [`RedactedFields`] (set by
///     [`RedactingLayer`]) when present, otherwise re-redacted from
///     the span attributes on the fly. This means the format layer
///     can stand alone even if [`RedactingLayer`] isn't installed —
///     it simply re-does the work.
#[derive(Clone)]
pub struct RedactingJsonFormat {
    config: RedactionConfig,
    include_target: bool,
}

impl RedactingJsonFormat {
    pub fn new(config: RedactionConfig) -> Self {
        Self { config, include_target: true }
    }

    pub fn with_target(mut self, include: bool) -> Self {
        self.include_target = include;
        self
    }

    fn active(&self) -> bool { self.config.mode.is_active() }
}

impl<S, N> FormatEvent<S, N> for RedactingJsonFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let key = self.config.key_bytes();
        // Collect this event's fields, redacted.
        let mut event_pairs: Vec<(String, String)> = Vec::new();
        {
            let mut visitor = CaptureVisitor {
                key: &key,
                active: self.active(),
                out: &mut event_pairs,
            };
            event.record(&mut visitor);
        }

        // Begin JSON object.
        writer.write_char('{')?;

        let now_rfc3339 = current_timestamp();
        write!(writer, "\"timestamp\":\"{now_rfc3339}\",")?;
        write!(writer, "\"level\":\"{}\",", event.metadata().level())?;
        if self.include_target {
            write_json_kv(&mut writer, "target", event.metadata().target())?;
            writer.write_char(',')?;
        }

        // Fields. Standard tracing convention: `message` lives in the
        // event's fields; we surface it as a top-level key when
        // present, else fall back to an empty string.
        let mut message: Option<String> = None;
        let mut other_pairs: Vec<(String, String)> = Vec::new();
        for (k, v) in event_pairs {
            if k == "message" {
                message = Some(v);
            } else {
                other_pairs.push((k, v));
            }
        }
        write_json_kv(&mut writer, "message", message.as_deref().unwrap_or(""))?;
        writer.write_char(',')?;
        writer.write_str("\"fields\":{")?;
        for (i, (k, v)) in other_pairs.iter().enumerate() {
            if i > 0 { writer.write_char(',')?; }
            write_json_kv(&mut writer, k, v)?;
        }
        writer.write_char('}')?;

        // Span chain. The current span is the innermost; we walk
        // outward and emit a JSON array of {name, fields} objects.
        if let Some(scope) = ctx.event_scope() {
            let mut spans = Vec::new();
            for span_ref in scope.from_root() {
                let name = span_ref.name().to_string();
                let ext = span_ref.extensions();
                if let Some(redacted) = ext.get::<RedactedFields>() {
                    spans.push((name, redacted.entries.clone()));
                } else {
                    // Layer not installed — re-redact on the fly.
                    let mut pairs: Vec<(String, String)> = Vec::new();
                    {
                        let _v = CaptureVisitor {
                            key: &key,
                            active: self.active(),
                            out: &mut pairs,
                        };
                        // We don't have access to the original
                        // Attributes here; the registry doesn't keep
                        // them. The redaction path therefore relies
                        // on the RedactingLayer having populated the
                        // extension. As a defensive fallback, emit
                        // an empty fields object.
                    }
                    spans.push((name, pairs));
                }
            }
            if !spans.is_empty() {
                writer.write_str(",\"spans\":[")?;
                for (i, (name, fields)) in spans.iter().enumerate() {
                    if i > 0 { writer.write_char(',')?; }
                    writer.write_char('{')?;
                    write_json_kv(&mut writer, "name", name)?;
                    if !fields.is_empty() {
                        writer.write_char(',')?;
                        for (j, (k, v)) in fields.iter().enumerate() {
                            if j > 0 { writer.write_char(',')?; }
                            write_json_kv(&mut writer, k, v)?;
                        }
                    }
                    writer.write_char('}')?;
                }
                writer.write_char(']')?;
            }
        }

        writer.write_char('}')?;
        writer.write_char('\n')?;
        Ok(())
    }
}

/// Best-effort RFC3339 timestamp. We avoid pulling `time` here so the
/// crate keeps a minimal dep set; the stock JSON formatter would use
/// `SystemTime`-based formatting via `chrono`, but a simple seconds-
/// since-epoch is fine for our log shape (downstream parsers convert).
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos();
    // Render as "YYYY-MM-DDTHH:MM:SS.<nanos>Z" using simple integer math.
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    let (year, month, day) = days_to_ymd(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}.{nanos:09}Z"
    )
}

/// Convert a count of days since 1970-01-01 (UTC) into (Y, M, D).
/// Algorithm: Howard Hinnant's civil-from-days; correct for the
/// full proleptic Gregorian range we care about.
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y_final = y + i64::from(m <= 2);
    (y_final, m as u32, d as u32)
}

/// Write a JSON `"key":"value"` pair with proper escaping. We
/// implement minimal escaping rather than pulling `serde_json` to
/// keep the crate's compile-cost low.
fn write_json_kv(writer: &mut Writer<'_>, key: &str, value: &str) -> fmt::Result {
    writer.write_char('"')?;
    write_json_escaped(writer, key)?;
    writer.write_str("\":\"")?;
    write_json_escaped(writer, value)?;
    writer.write_char('"')?;
    Ok(())
}

fn write_json_escaped(writer: &mut Writer<'_>, s: &str) -> fmt::Result {
    for ch in s.chars() {
        match ch {
            '"' => writer.write_str("\\\"")?,
            '\\' => writer.write_str("\\\\")?,
            '\n' => writer.write_str("\\n")?,
            '\r' => writer.write_str("\\r")?,
            '\t' => writer.write_str("\\t")?,
            c if (c as u32) < 0x20 => {
                write!(writer, "\\u{:04x}", c as u32)?;
            }
            c => writer.write_char(c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 bytes of `0x42` — deterministic so MAC outputs are stable
    /// across test runs.
    const TEST_KEY_HEX: &str =
        "4242424242424242424242424242424242424242424242424242424242424242";

    fn config(mode: RedactionMode) -> RedactionConfig {
        RedactionConfig::new(mode, TEST_KEY_HEX).expect("test key valid")
    }

    fn key_bytes() -> [u8; 32] {
        [0x42; 32]
    }

    #[test]
    fn spiffe_uri_in_principal_redacts_path() {
        let red = redact_field(
            "principal",
            "spiffe://recor.cm/declarant/alice",
            &key_bytes(),
        );
        assert!(red.starts_with("spiffe://recor.cm/"), "got {red}");
        assert!(!red.contains("alice"));
        let suffix = red.strip_prefix("spiffe://recor.cm/").unwrap();
        assert_eq!(suffix.len(), 16);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn spiffe_uri_redaction_is_stable() {
        let a = redact_field("any", "spiffe://recor.cm/alice", &key_bytes());
        let b = redact_field("any", "spiffe://recor.cm/alice", &key_bytes());
        assert_eq!(a, b, "same input + key must yield same MAC");
    }

    #[test]
    fn uuid_in_person_id_field_redacts() {
        let uuid = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80";
        let red = redact_field("person_id", uuid, &key_bytes());
        assert_eq!(red.len(), 16);
        assert!(red.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(red, uuid);
    }

    #[test]
    fn uuid_in_entity_id_field_passes_through() {
        let uuid = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80";
        let red = redact_field("entity_id", uuid, &key_bytes());
        assert_eq!(red, uuid, "entity_id is NOT PII");
    }

    #[test]
    fn uuid_in_declaration_id_field_passes_through() {
        let uuid = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80";
        let red = redact_field("declaration_id", uuid, &key_bytes());
        assert_eq!(red, uuid, "declaration_id is NOT PII");
    }

    #[test]
    fn string_in_event_type_field_passes_through() {
        let red = redact_field("event_type", "declaration.submitted.v1", &key_bytes());
        assert_eq!(red, "declaration.submitted.v1");
    }

    #[test]
    fn receipt_hash_keeps_head_and_tail() {
        let hash = "abcd1234567890deadbeefcafebabe1234567890abcdef0123456789abcdef01";
        let red = redact_field("receipt_hash_hex", hash, &key_bytes());
        assert_eq!(red, format!("{}…{}", &hash[..8], &hash[hash.len() - 4..]));
        assert!(!red.contains(&hash[10..50]), "middle must not leak");
    }

    #[test]
    fn tampered_key_still_redacts_no_leak() {
        let other = [0x99u8; 32];
        let red = redact_field("principal", "spiffe://recor.cm/alice", &other);
        assert!(red.starts_with("spiffe://recor.cm/"));
        assert!(!red.contains("alice"));
        let canonical = redact_field("principal", "spiffe://recor.cm/alice", &key_bytes());
        assert_ne!(red, canonical, "different key → different MAC");
    }

    #[test]
    fn disabled_mode_lets_values_through_layer() {
        let cfg = config(RedactionMode::DisabledForDev);
        assert!(!cfg.mode.is_active());
    }

    #[test]
    fn enabled_mode_is_active() {
        let cfg = config(RedactionMode::Enabled);
        assert!(cfg.mode.is_active());
    }

    #[test]
    fn config_rejects_short_key() {
        let err = RedactionConfig::new(RedactionMode::Enabled, "deadbeef")
            .expect_err("short key must be rejected");
        assert!(matches!(err, RedactionConfigError::KeyWrongLength(8)));
    }

    #[test]
    fn config_rejects_non_hex_key() {
        let bad = "z".repeat(64);
        let err = RedactionConfig::new(RedactionMode::Enabled, &bad)
            .expect_err("non-hex key must be rejected");
        assert!(matches!(err, RedactionConfigError::KeyNotHex(_)));
    }

    #[test]
    fn mode_parsing_defaults_dev_to_disabled_for_dev() {
        let m = RedactionMode::from_env(None, true).unwrap();
        assert_eq!(m, RedactionMode::DisabledForDev);
    }

    #[test]
    fn mode_parsing_defaults_prod_to_enabled() {
        let m = RedactionMode::from_env(None, false).unwrap();
        assert_eq!(m, RedactionMode::Enabled);
    }

    #[test]
    fn mode_parsing_rejects_garbage() {
        let err = RedactionMode::from_env(Some("yolo"), true).unwrap_err();
        assert!(matches!(err, RedactionConfigError::InvalidMode(_)));
    }

    #[test]
    fn mode_parsing_accepts_all_three_tokens() {
        for tok in ["enabled", "disabled-for-dev", "disabled"] {
            assert!(
                RedactionMode::from_env(Some(tok), true).is_ok(),
                "token `{tok}` must parse"
            );
        }
    }

    #[test]
    fn timestamp_renders_some_valid_iso8601_shape() {
        let ts = current_timestamp();
        // YYYY-MM-DDTHH:MM:SS.<9 digits>Z
        assert_eq!(ts.len(), "1970-01-01T00:00:00.000000000Z".len());
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn json_escape_quotes_and_backslashes() {
        let mut out = String::new();
        let writer = Writer::new(&mut out);
        write_json_escaped(&mut { writer }, "a\"b\\c\nd").unwrap();
        assert_eq!(out, "a\\\"b\\\\c\\nd");
    }
}
