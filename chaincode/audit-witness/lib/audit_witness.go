// Package auditwitness implements the RÉCOR audit-witness Hyperledger
// Fabric chaincode. Every declaration event produced by the Declaration
// service is anchored to a Fabric channel via this contract.
//
// Key layout (all values are stored under deterministic namespace prefixes
// so independent operators can list / iterate without re-deriving the key
// shape):
//
//	recor.audit.declaration:<event_id>
//	    The canonical audit entry. event_id is the UUIDv7 the
//	    Declaration service minted when the event was first persisted.
//	    Payload: AuditEntry JSON (see below).
//
//	recor.audit.index.declaration:<declaration_id>:<event_id>
//	    Composite key for the secondary index, allowing
//	    ListAuditEntriesForDeclaration to use GetStateByPartialCompositeKey
//	    without scanning the entire channel. Value is the empty byte slice;
//	    the existence of the key is the index entry.
//
// Idempotency: PutAuditEntry refuses to overwrite an existing entry. The
// Declaration service emits an event_id once; replay attempts (from the
// bridge worker's at-least-once delivery) are no-ops at the chaincode
// boundary. This is the load-bearing property D13 (idempotency on every
// state-changing operation) — implemented HERE, not at the bridge layer,
// because Fabric peers are the trust anchor.
//
// Doctrine notes:
//   - D14 (fail-closed): every method returns an error rather than a
//     silent default on invalid input. The contract API translates the
//     error into a transaction abort, which the bridge then reads and
//     either retries or DLQs.
//   - D15 (cryptographic provenance): each AuditEntry carries the
//     BLAKE3 receipt hash. The verifier service (apps/audit-verifier)
//     re-derives the hash from the projection's canonical bytes and
//     compares against the on-chain value; mismatch = tampering.
package auditwitness

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"

	"github.com/hyperledger/fabric-contract-api-go/contractapi"
)

// Key prefixes. Exported so tests + sibling chaincodes can build keys
// without re-deriving the shape.
const (
	// KeyPrefixAuditEntry is the namespace for canonical entries keyed
	// by event_id.
	KeyPrefixAuditEntry = "recor.audit.declaration"
	// IndexObjectType is the composite-key namespace for the
	// (declaration_id, event_id) secondary index.
	IndexObjectType = "recor.audit.index.declaration"
)

// AuditEntry is the payload stored under KeyPrefixAuditEntry:<event_id>.
// Field ordering is fixed so the JSON encoding is canonical across peers.
//
// The signing-peer attestation is the Fabric-side complement to the
// declarant Ed25519 attestation. The Declaration service's bridge worker
// includes a signature over (event_id || declaration_id || receipt_hash)
// produced by the peer signing certificate it identifies as; the
// chaincode does not verify this here (peer identity is verified by the
// channel policy at endorsement time) but stores it so an external
// auditor can re-verify off-chain.
type AuditEntry struct {
	EventID                string `json:"event_id"`
	DeclarationID          string `json:"declaration_id"`
	ReceiptHashHex         string `json:"receipt_hash_hex"`
	Timestamp              string `json:"ts"`
	SigningPeerAttestation []byte `json:"signing_peer_attestation,omitempty"`
}

// AuditWitnessContract is the chaincode contract surface registered with
// the Fabric runtime. Methods on this type are callable as transactions
// (state-changing) or queries (read-only) per the contract-api binding.
type AuditWitnessContract struct {
	contractapi.Contract
}

// PutAuditEntry records an audit entry for one declaration event. The
// operation is idempotent: a second invocation with the same event_id
// returns an error rather than overwriting the previous entry. The
// Declaration service treats the "already exists" error as success
// (D13 idempotency) — replays are expected from at-least-once delivery.
//
// Returns an error if:
//   - any required field is empty;
//   - receipt_hash_hex is not 64 lowercase hex characters;
//   - an entry with this event_id already exists.
func (c *AuditWitnessContract) PutAuditEntry(
	ctx contractapi.TransactionContextInterface,
	eventID string,
	declarationID string,
	receiptHashHex string,
	ts string,
	signingPeerAttestation []byte,
) error {
	if err := validateNonEmpty("event_id", eventID); err != nil {
		return err
	}
	if err := validateNonEmpty("declaration_id", declarationID); err != nil {
		return err
	}
	if err := validateReceiptHash(receiptHashHex); err != nil {
		return err
	}
	if err := validateNonEmpty("ts", ts); err != nil {
		return err
	}

	key := auditEntryKey(eventID)
	existing, err := ctx.GetStub().GetState(key)
	if err != nil {
		return fmt.Errorf("get state failed for %s: %w", key, err)
	}
	if existing != nil {
		return fmt.Errorf("audit entry already exists for event_id=%s", eventID)
	}

	entry := AuditEntry{
		EventID:                eventID,
		DeclarationID:          declarationID,
		ReceiptHashHex:         receiptHashHex,
		Timestamp:              ts,
		SigningPeerAttestation: signingPeerAttestation,
	}
	bytes, err := json.Marshal(entry)
	if err != nil {
		return fmt.Errorf("marshal audit entry: %w", err)
	}
	if err := ctx.GetStub().PutState(key, bytes); err != nil {
		return fmt.Errorf("put state failed for %s: %w", key, err)
	}

	// Secondary index: (declaration_id, event_id). Empty value — the
	// existence of the composite key is the index entry.
	indexKey, err := ctx.GetStub().CreateCompositeKey(
		IndexObjectType, []string{declarationID, eventID},
	)
	if err != nil {
		return fmt.Errorf("create composite key: %w", err)
	}
	if err := ctx.GetStub().PutState(indexKey, []byte{0x00}); err != nil {
		return fmt.Errorf("put index state failed: %w", err)
	}

	return nil
}

// GetAuditEntry returns the entry stored for event_id, or nil if no
// entry exists. Missing-not-error is the deliberate choice — the
// verifier app distinguishes "absent" from "tampering" by checking the
// receipt-hash on the projection independently.
func (c *AuditWitnessContract) GetAuditEntry(
	ctx contractapi.TransactionContextInterface,
	eventID string,
) (*AuditEntry, error) {
	if err := validateNonEmpty("event_id", eventID); err != nil {
		return nil, err
	}
	key := auditEntryKey(eventID)
	bytes, err := ctx.GetStub().GetState(key)
	if err != nil {
		return nil, fmt.Errorf("get state failed for %s: %w", key, err)
	}
	if bytes == nil {
		return nil, nil
	}
	var entry AuditEntry
	if err := json.Unmarshal(bytes, &entry); err != nil {
		return nil, fmt.Errorf("unmarshal audit entry: %w", err)
	}
	return &entry, nil
}

// ListAuditEntriesForDeclaration returns every audit entry recorded for
// the given declaration_id, ordered ascending by event_id. The ordering
// is byte-lexicographic on the UUIDv7 string form — which, because v7
// embeds a millisecond timestamp in the high bits, is monotonic in
// submission order modulo same-millisecond ties.
//
// Returns an empty slice (not nil) when no entries are recorded for
// the declaration. Returns an error on iterator failure.
func (c *AuditWitnessContract) ListAuditEntriesForDeclaration(
	ctx contractapi.TransactionContextInterface,
	declarationID string,
) ([]*AuditEntry, error) {
	if err := validateNonEmpty("declaration_id", declarationID); err != nil {
		return nil, err
	}

	iter, err := ctx.GetStub().GetStateByPartialCompositeKey(
		IndexObjectType, []string{declarationID},
	)
	if err != nil {
		return nil, fmt.Errorf("iterate composite key: %w", err)
	}
	defer iter.Close()

	var eventIDs []string
	for iter.HasNext() {
		kv, err := iter.Next()
		if err != nil {
			return nil, fmt.Errorf("iterator next: %w", err)
		}
		_, parts, err := ctx.GetStub().SplitCompositeKey(kv.Key)
		if err != nil {
			return nil, fmt.Errorf("split composite key: %w", err)
		}
		if len(parts) != 2 {
			return nil, fmt.Errorf("unexpected composite key shape: %v", parts)
		}
		eventIDs = append(eventIDs, parts[1])
	}

	// Order is contract: callers (the verifier) expect ascending. The
	// iterator returns lexical order on the composite key which is
	// already (declaration_id, event_id) ascending, but we sort
	// defensively because the contract-api doc doesn't guarantee this
	// across all state-database implementations (LevelDB vs. CouchDB).
	sort.Strings(eventIDs)

	out := make([]*AuditEntry, 0, len(eventIDs))
	for _, eid := range eventIDs {
		entry, err := c.GetAuditEntry(ctx, eid)
		if err != nil {
			return nil, err
		}
		if entry == nil {
			// Index drift — the index has a key but the canonical
			// entry is gone. Fail loudly; this should never happen
			// in production (Fabric is append-only).
			return nil, fmt.Errorf("index drift: event_id=%s indexed but absent", eid)
		}
		out = append(out, entry)
	}
	return out, nil
}

// auditEntryKey returns the canonical state key for an event_id.
func auditEntryKey(eventID string) string {
	return fmt.Sprintf("%s:%s", KeyPrefixAuditEntry, eventID)
}

// validateNonEmpty rejects empty or whitespace-only field values.
func validateNonEmpty(field, value string) error {
	if strings.TrimSpace(value) == "" {
		return fmt.Errorf("%s must not be empty", field)
	}
	return nil
}

// validateReceiptHash enforces the BLAKE3-256 wire form: exactly 64
// lowercase hex characters. The Declaration service emits hashes in
// this form; mismatches indicate a contract violation by the bridge
// rather than tampering, and we want to fail fast.
func validateReceiptHash(hash string) error {
	if len(hash) != 64 {
		return fmt.Errorf("receipt_hash_hex must be 64 chars, got %d", len(hash))
	}
	for _, r := range hash {
		if !((r >= '0' && r <= '9') || (r >= 'a' && r <= 'f')) {
			return fmt.Errorf("receipt_hash_hex must be lowercase hex")
		}
	}
	return nil
}
