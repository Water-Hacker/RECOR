// TODO-064 closure: chaincode idempotency tests at the boundary the
// Declaration service's bridge worker exercises.
//
// The existing `audit_witness_test.go` carries one duplicate-refused
// case as a smoke; this file makes idempotency the load-bearing
// contract under test. The Declaration service emits each event_id
// exactly once but the bridge can retry on transport failure (D14
// fail-closed at the integration boundary), so the chaincode MUST:
//
//  1. Refuse to overwrite an existing event_id (the receipt anchor is
//     append-only — D13 idempotency on every state-changing op).
//  2. Refuse a second submission for the same event_id even when the
//     receipt_hash differs (an attacker re-signing the same event id
//     under a different hash must NOT win).
//  3. Leave the secondary index pristine — a refused second put must
//     NOT double-write into the (declaration_id, event_id) composite
//     index.
//  4. Surface the refusal as an error the bridge can pattern-match on
//     ("already exists"), so the application layer can convert it to
//     a success-no-op without falsely interpreting it as a write.
//
// Coverage measured against D15 (cryptographic provenance): every
// receipt hash anchored on Fabric is the canonical hash for its
// event_id forever. The tests below are the on-chain guard that
// makes that guarantee load-bearing.

package auditwitness

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// stateKeyCount counts how many entries exist under the audit-entry
// prefix. Used to assert that a refused-put did NOT side-effect the
// state map.
func stateKeyCount(s *mockStub, prefix string) int {
	n := 0
	for k := range s.state {
		if len(k) >= len(prefix) && k[:len(prefix)] == prefix {
			n++
		}
	}
	return n
}

// TestIdempotency_SameAnchorTwice — the bridge's retry path. The
// second invocation MUST be refused; the canonical entry MUST remain
// byte-identical to what the first invocation wrote (no overwrite
// even with identical inputs); the secondary index MUST NOT grow.
func TestIdempotency_SameAnchorTwice(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	eventID := "01900000-0000-7000-8000-0000000064a1"
	declID := "decl-idemp-001"

	// First write — must succeed.
	require.NoError(t, contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash, testTimestamp, []byte("att-1")))
	require.Equal(t, 1, stateKeyCount(stub, KeyPrefixAuditEntry))
	firstBytes := append([]byte(nil), stub.state[auditEntryKey(eventID)]...)
	firstIndexCount := stateKeyCount(stub, "\x00"+IndexObjectType)

	// Second write — bit-for-bit identical inputs. The chaincode
	// must refuse and the state must remain unchanged. This is the
	// load-bearing D13 invariant for the bridge's at-least-once
	// retry path.
	err := contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash, testTimestamp, []byte("att-1"))
	require.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")

	// Canonical entry: unchanged.
	assert.Equal(t, firstBytes, stub.state[auditEntryKey(eventID)],
		"second PutAuditEntry must not overwrite the canonical bytes")
	// Audit-entry namespace: still exactly one row.
	assert.Equal(t, 1, stateKeyCount(stub, KeyPrefixAuditEntry),
		"refused put leaked a duplicate audit entry")
	// Secondary index: no new composite-key row.
	assert.Equal(t, firstIndexCount, stateKeyCount(stub, "\x00"+IndexObjectType),
		"refused put leaked a duplicate composite-key row")
}

// TestIdempotency_ConflictingReceiptHashRejected — adversarial case.
// An operator (or a misbehaving bridge) re-submits the same event_id
// with a different receipt_hash. The chaincode must refuse rather
// than silently overwrite the original anchor; otherwise the
// declarant's BLAKE3 receipt could be voided after the fact and
// D15 (cryptographic provenance) would not hold.
func TestIdempotency_ConflictingReceiptHashRejected(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	eventID := "01900000-0000-7000-8000-0000000064a2"
	declID := "decl-idemp-002"

	require.NoError(t, contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash, testTimestamp, nil))
	originalBytes := append([]byte(nil), stub.state[auditEntryKey(eventID)]...)

	// Re-submission with the same event_id but a DIFFERENT
	// receipt_hash. This is the canonical attack surface: an actor
	// with bridge access tries to re-anchor an event to a hash they
	// control. The chaincode refuses; the on-chain hash for this
	// event_id remains testReceiptHash forever.
	err := contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash2, testTimestamp, nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")

	// Confirm the original receipt hash is still what the chaincode
	// returns when queried. This is the operator-facing read path,
	// the same one the audit verifier service hits.
	got, err := contract.GetAuditEntry(ctx, eventID)
	require.NoError(t, err)
	require.NotNil(t, got)
	assert.Equal(t, testReceiptHash, got.ReceiptHashHex,
		"on-chain receipt hash MUST remain the value first written; D15 would not hold otherwise")

	// Defence in depth: the raw bytes are unchanged too. (Catches a
	// hypothetical regression where ReceiptHashHex reads correctly
	// but some other field was silently mutated.)
	assert.Equal(t, originalBytes, stub.state[auditEntryKey(eventID)])
}

// TestIdempotency_RetryAfterRefusedIsStillNoOp — the bridge's retry
// loop may submit the same event_id N times. After the first refuse,
// every subsequent call must continue to refuse with the same error,
// without growing the state. This is the "stable refusal" property
// — the bridge counts on it to bound its retry budget.
func TestIdempotency_RetryAfterRefusedIsStillNoOp(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	eventID := "01900000-0000-7000-8000-0000000064a3"
	declID := "decl-idemp-003"
	require.NoError(t, contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash, testTimestamp, nil))

	for i := 0; i < 5; i++ {
		err := contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash, testTimestamp, nil)
		require.Error(t, err, "iteration %d", i)
		assert.Contains(t, err.Error(), "already exists",
			"every retry must surface the same error so the bridge can pattern-match it")
	}
	// State remains one audit entry + one index row.
	assert.Equal(t, 1, stateKeyCount(stub, KeyPrefixAuditEntry))
}

// TestIdempotency_DifferentEventIdsCoexist — sanity check: the
// idempotency rule is per event_id. Two distinct event_ids in the
// same declaration MUST both land; the idempotency guard must not
// over-reach and refuse legitimate parallel anchors.
func TestIdempotency_DifferentEventIdsCoexist(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	declID := "decl-idemp-004"
	eventA := "01900000-0000-7000-8000-0000000064a4"
	eventB := "01900000-0000-7000-8000-0000000064a5"

	require.NoError(t, contract.PutAuditEntry(ctx, eventA, declID, testReceiptHash, testTimestamp, nil))
	require.NoError(t, contract.PutAuditEntry(ctx, eventB, declID, testReceiptHash2, testTimestamp, nil))

	listed, err := contract.ListAuditEntriesForDeclaration(ctx, declID)
	require.NoError(t, err)
	require.Len(t, listed, 2)
	assert.Equal(t, eventA, listed[0].EventID)
	assert.Equal(t, eventB, listed[1].EventID)
	// Distinct receipt hashes preserved end-to-end.
	assert.Equal(t, testReceiptHash, listed[0].ReceiptHashHex)
	assert.Equal(t, testReceiptHash2, listed[1].ReceiptHashHex)
}
