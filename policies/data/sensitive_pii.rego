# Sensitive-PII classification + audit-retention rules.
#
# Used by the audit-reconciler + the retention worker to decide
# which fields require longer-than-default retention AND extra
# logging. Mirrors docs/compliance/data-classification.md.
package recor.data

import future.keywords.contains
import future.keywords.if
import future.keywords.in

# Fields classified as Sensitive-PII across RÉCOR's domain.
sensitive_pii_fields := {
    "primary_id_document",
    "biometric_reference_hash",
    "date_of_birth",
    "nationality",
    "canonical_full_name",
}

# Decide whether a record contains Sensitive-PII.
is_sensitive_pii(record) if {
    some field in sensitive_pii_fields
    record[field]
}

# Audit-log retention (D15 — forever for the event log,
# 30 days for the outbox, per docs/compliance/data-retention.md).
event_log_retention_days := 36500  # 100 years; effectively forever
outbox_retention_days := 30

# Records flagged as Sensitive-PII MUST be logged with the
# log_redaction layer active (OPS-2).
deny contains msg if {
    is_sensitive_pii(input.record)
    input.log_redaction != "enabled"
    msg := "Sensitive-PII record observed while log_redaction is not enabled (OPS-2)"
}
