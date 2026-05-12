# Runbook — Audit verification

How an external party (operator, consortium member, journalist, citizen,
or court-appointed auditor) verifies that a historical RÉCOR declaration
has not been tampered with.

This is the user-facing complement to
`docs/runbooks/fabric-bridge.md`. The bridge writes; this surface
reads.

## What "verification" proves

The verifier checks the **chain of cryptographic provenance** from the
declaration body to the Fabric audit channel:

1. The declaration body in the projection is hashed with BLAKE3-256.
2. The Fabric chaincode stores the receipt hash for each declaration
   event, anchored by an inter-organisation consensus on the
   `recor-audit` channel.
3. The verifier compares the re-derived hash to the on-chain hash.

A successful verification proves the declaration body the platform
serves today is byte-identical (modulo whitespace) to the body that was
hashed at submission time AND that hash was witnessed by the consortium
(ARMP, ANIF, DGI, CONAC, etc.) at submission time.

A failed verification means one of three things:

- The projection has been altered post-hoc (e.g., illicit DBA action).
- The Fabric entry was malformed at anchor time (a bridge bug).
- The chain of bridge → chaincode delivery dropped an event (a bridge
  failure that landed in `fabric_bridge_dlq`).

The verifier report distinguishes these cases via the per-entry
`status` field.

## Surface

`GET /v1/audit/verify/{declaration_id}` on the `audit-verifier` app.

- Public read (OIDC-gated behind the platform's identity provider for
  audit logging; the verification result itself is not personally
  identifying).
- No write surface.

## Calling the verifier

```bash
# Public-facing URL in production (TBD by the deployment)
AUDIT_URL=https://audit.recor.cm

# Acquire an OIDC token (any authenticated principal; the endpoint is
# read-only and does not require any specific role).
TOKEN=$(...)

curl -fsSL \
  -H "Authorization: Bearer $TOKEN" \
  "$AUDIT_URL/v1/audit/verify/01900000-0000-7000-8000-000000000001" \
  | jq .
```

## Response shape

```json
{
  "declaration_id": "01900000-0000-7000-8000-000000000001",
  "result": "authentic",
  "on_chain_count": 2,
  "projection_count": 2,
  "entries": [
    {
      "event_id": "019aaaaa-...",
      "status": "matched",
      "tx_id": "abc123def456...",
      "on_chain_receipt_hash_hex": "...",
      "derived_receipt_hash_hex": "...",
      "on_chain_ts": "2026-05-12T10:00:00Z",
      "event_type": "declaration.submitted.v1"
    },
    {
      "event_id": "019bbbbb-...",
      "status": "matched",
      "tx_id": "fed987cba654...",
      "on_chain_receipt_hash_hex": "...",
      "derived_receipt_hash_hex": "...",
      "on_chain_ts": "2026-05-12T11:00:00Z",
      "event_type": "declaration.amended.v1"
    }
  ]
}
```

## Interpreting the result

| `result` | Meaning | Operator action |
|---|---|---|
| `authentic` | All entries matched. The declaration is verified. | None — happy path. |
| `tampered` | At least one entry shows a hash mismatch, OR an on-chain entry has no projection counterpart. | **Page security.** This is either a database tampering incident or a phantom anchor. |
| `incomplete` | All known entries matched, but at least one projection event has no on-chain anchor. | Inspect `fabric_bridge_dlq` for the missing event_id; manual re-anchor per `fabric-bridge.md`. |

The HTTP status code distinguishes:

- `200 OK` with `result: authentic / tampered / incomplete` — verifier
  did its job; interpret per the table above.
- `400 Bad Request` — `declaration_id` is not a UUID.
- `503 Service Unavailable` — Fabric is unreachable; verifier refuses
  to render a partial answer (D14 fail-closed). Try again later or
  escalate to the infra team.
- `502 Bad Gateway` — Fabric upstream returned an error; same
  escalation path.

## Cross-checking on-chain entries

For a high-stakes verification (court testimony, public dispute), the
verifier's report is not the final word — it's the platform attesting
to itself. A determined verifier can cross-check the report against
the Fabric channel directly:

1. The `tx_id` field in each entry is the Fabric transaction ID.
2. Any Fabric peer operated by a consortium member can be queried
   with that `tx_id` to retrieve the on-chain block + transaction
   payload.
3. The retrieved payload should match the `on_chain_receipt_hash_hex`
   in the report.

A future ticket will add a block-explorer-style UI for the audit
channel; today, the path is `peer chaincode query` against any
consortium member's peer (operators publish endpoints in their
inter-org documentation).

## What the verifier cannot prove

- **That the declarant who signed is who they say they are.** That's
  the Ed25519 attestation chain, which is a separate verification
  layer (declarant binding to the platform's identity provider).
- **That the declaration is factually true.** Verification proves
  cryptographic integrity, not factual truth. The verification engine
  (separate service) is the system that adjudicates truthfulness.
- **That the declaration was submitted at the on-chain `ts`.** The
  declaration service writes the timestamp; if the service itself is
  compromised, both the projection AND the chain anchor would carry
  the same falsified time. Mitigations: clock attestations, multi-party
  endorsement (which we have via Fabric's channel policy), and external
  time witnesses (e.g., RFC 3161 timestamps — a future ticket).

## Operator-side verification

When operators inside the consortium need to verify many declarations
at once (e.g., post-incident integrity sweep):

```bash
# Iterate over a list of declaration_ids
for id in $(cat declaration_ids.txt); do
  result=$(curl -fsSL \
    -H "Authorization: Bearer $TOKEN" \
    "$AUDIT_URL/v1/audit/verify/$id" \
    | jq -r .result)
  echo "$id $result"
done | tee verification-sweep.tsv
```

Bulk verification UI is on the roadmap; today, scripted curl is the
supported pattern.

## Escalation

| Condition | Escalate to |
|---|---|
| `result: tampered` on any declaration | Security on-call (P1) + lead architect |
| Verifier returning 503 sustained > 15min | Infra team Fabric cluster lead |
| Bulk sweep finds > 0.1% non-authentic | Security on-call (P1) + lead architect |
| Discrepancy between verifier report and external Fabric peer query | Security on-call (P1) — possible platform compromise |

## See also

- `docs/adr/0009-fabric-audit-anchoring.md` — design rationale
- `docs/runbooks/fabric-bridge.md` — bridge worker operations (anchor side)
- `chaincode/audit-witness/README.md` — chaincode surface
- `docs/security/threat-model.md` § G1 — the gap this surface closes
