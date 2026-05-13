# audit-witness chaincode

Hyperledger Fabric chaincode that anchors every RÉCOR declaration event
to the audit channel. Closes Gap G1 in
`docs/security/threat-model.md` and discharges doctrine D15
(cryptographic provenance).

See `docs/adr/0009-fabric-audit-anchoring.md` for the decision record.

## Layout

- `lib/audit_witness.go` — contract implementation.
- `lib/audit_witness_test.go` — unit tests with an in-memory stub.
- `cmd/main.go` — chaincode binary registered with the Fabric peer.

## Contract surface

| Method | Kind | Behaviour |
|--------|------|-----------|
| `PutAuditEntry(event_id, declaration_id, receipt_hash_hex, ts, signing_peer_attestation)` | invoke | Idempotent put; duplicate event_id is refused. |
| `GetAuditEntry(event_id)` | query | Returns the entry or nil. |
| `ListAuditEntriesForDeclaration(declaration_id)` | query | Returns every entry for a declaration, ordered ascending by event_id. |

## Building

```bash
cd chaincode/audit-witness
go mod tidy
go test ./...
go build -o /tmp/audit-witness ./cmd
```

The peer process invokes the binary via the chaincode-as-a-service or
external-builder pattern; see the Fabric ops team's deployment notes
(out of scope for this repo).

## Doctrines

- **D13 idempotency** — duplicate PutAuditEntry refused; bridge worker
  treats the rejection as success.
- **D14 fail-closed** — invalid input or write failure aborts the
  transaction.
- **D15 cryptographic provenance** — the receipt hash is the load-bearing
  field; verifier app re-derives and compares.
