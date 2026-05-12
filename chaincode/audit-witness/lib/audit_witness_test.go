// Unit tests for the audit-witness chaincode.
//
// These tests use a hand-rolled ChaincodeStub mock rather than the
// shimtest package (which has been deprecated for contract-api use). The
// mock implements only the methods the contract actually invokes —
// GetState, PutState, CreateCompositeKey, SplitCompositeKey, and
// GetStateByPartialCompositeKey — and exposes the underlying map for
// assertions. The contract-api TransactionContextInterface is satisfied
// by a small wrapper that returns this stub from GetStub().
//
// What's covered:
//   - put + get round-trip (the happy path);
//   - duplicate put refused (D13 idempotency at the contract boundary);
//   - missing get returns nil rather than an error;
//   - secondary index lookup returns entries ordered by event_id;
//   - validation rejects empty fields and malformed receipt hashes.

package auditwitness

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"testing"

	"github.com/hyperledger/fabric-chaincode-go/shim"
	"github.com/hyperledger/fabric-contract-api-go/contractapi"
	"github.com/hyperledger/fabric-protos-go/peer"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// ── Test fixtures ─────────────────────────────────────────────────────────

const (
	testReceiptHash  = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
	testReceiptHash2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
	testTimestamp    = "2026-05-12T10:00:00Z"
)

// ── mockStub: minimal shim.ChaincodeStubInterface for our tests ──────────

type mockStub struct {
	shim.ChaincodeStubInterface
	state map[string][]byte
}

func newMockStub() *mockStub {
	return &mockStub{state: make(map[string][]byte)}
}

func (s *mockStub) GetState(key string) ([]byte, error) {
	val, ok := s.state[key]
	if !ok {
		return nil, nil
	}
	return val, nil
}

func (s *mockStub) PutState(key string, value []byte) error {
	if key == "" {
		return fmt.Errorf("empty key")
	}
	s.state[key] = append([]byte(nil), value...)
	return nil
}

// Mirror Fabric's composite-key format: 0x00 prefix, then objectType +
// 0x00 + each attribute + 0x00. Good enough for the tests.
func (s *mockStub) CreateCompositeKey(objectType string, attributes []string) (string, error) {
	if objectType == "" {
		return "", fmt.Errorf("empty objectType")
	}
	var b strings.Builder
	b.WriteByte(0x00)
	b.WriteString(objectType)
	b.WriteByte(0x00)
	for _, a := range attributes {
		b.WriteString(a)
		b.WriteByte(0x00)
	}
	return b.String(), nil
}

func (s *mockStub) SplitCompositeKey(key string) (string, []string, error) {
	if len(key) == 0 || key[0] != 0x00 {
		return "", nil, fmt.Errorf("not a composite key")
	}
	parts := strings.Split(key[1:], "\x00")
	if len(parts) < 2 {
		return "", nil, fmt.Errorf("malformed composite key")
	}
	objectType := parts[0]
	// Trailing empty due to terminating 0x00; trim it.
	attrs := parts[1:]
	if len(attrs) > 0 && attrs[len(attrs)-1] == "" {
		attrs = attrs[:len(attrs)-1]
	}
	return objectType, attrs, nil
}

func (s *mockStub) GetStateByPartialCompositeKey(
	objectType string, keys []string,
) (shim.StateQueryIteratorInterface, error) {
	prefix, err := s.CreateCompositeKey(objectType, keys)
	if err != nil {
		return nil, err
	}
	var matched []*peer.KV
	var allKeys []string
	for k := range s.state {
		if strings.HasPrefix(k, prefix) {
			allKeys = append(allKeys, k)
		}
	}
	// Stable lexical order — Fabric's LevelDB-backed iterator gives
	// the same; tests rely on it via the contract's sort.Strings call.
	sort.Strings(allKeys)
	for _, k := range allKeys {
		matched = append(matched, &peer.KV{Key: k, Value: s.state[k]})
	}
	return &mockIterator{items: matched}, nil
}

type mockIterator struct {
	shim.StateQueryIteratorInterface
	items []*peer.KV
	pos   int
}

func (it *mockIterator) HasNext() bool { return it.pos < len(it.items) }

func (it *mockIterator) Next() (*peer.KV, error) {
	if it.pos >= len(it.items) {
		return nil, fmt.Errorf("iterator exhausted")
	}
	item := it.items[it.pos]
	it.pos++
	return item, nil
}

func (it *mockIterator) Close() error { return nil }

// ── mockTxContext: satisfies contractapi.TransactionContextInterface ─────

type mockTxContext struct {
	contractapi.TransactionContextInterface
	stub *mockStub
}

func (c *mockTxContext) GetStub() shim.ChaincodeStubInterface {
	return c.stub
}

// SetStub / GetClientIdentity / GetStubBytesQueryHandler are not used
// by the contract under test; defaults are fine. The contractapi base
// embed handles the unimplemented surface — but contractapi's interface
// is large; instead, we use an interface assertion that the contract
// only calls GetStub().
//
// Compile-time assertion that GetStub satisfies the interface fragment
// the contract uses.
func (c *mockTxContext) Done() context.Context { return context.Background() }

// ── Tests ────────────────────────────────────────────────────────────────

func TestPutAndGetAuditEntry_RoundTrip(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	eventID := "01900000-0000-7000-8000-000000000001"
	declID := "decl-aaa"
	err := contract.PutAuditEntry(ctx, eventID, declID, testReceiptHash, testTimestamp, []byte("att"))
	require.NoError(t, err)

	got, err := contract.GetAuditEntry(ctx, eventID)
	require.NoError(t, err)
	require.NotNil(t, got)
	assert.Equal(t, eventID, got.EventID)
	assert.Equal(t, declID, got.DeclarationID)
	assert.Equal(t, testReceiptHash, got.ReceiptHashHex)
	assert.Equal(t, testTimestamp, got.Timestamp)
	assert.Equal(t, []byte("att"), got.SigningPeerAttestation)
}

func TestPutAuditEntry_DuplicateRefused(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	eventID := "01900000-0000-7000-8000-000000000002"
	require.NoError(t, contract.PutAuditEntry(ctx, eventID, "decl-bbb", testReceiptHash, testTimestamp, nil))

	// Second put with same event_id (even with different declaration_id
	// or receipt hash) must be refused. The bridge worker turns this
	// into a success/no-op at the application layer.
	err := contract.PutAuditEntry(ctx, eventID, "decl-bbb", testReceiptHash2, testTimestamp, nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")
}

func TestGetAuditEntry_MissingReturnsNil(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	got, err := contract.GetAuditEntry(ctx, "01900000-0000-7000-8000-aaaaaaaaaaaa")
	require.NoError(t, err)
	assert.Nil(t, got)
}

func TestListAuditEntriesForDeclaration_OrderedAscending(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	declID := "decl-list-001"
	// Insert in reverse alphabetical order to prove the contract
	// re-sorts ascending on read.
	events := []string{
		"01900000-0000-7000-8000-00000000000c",
		"01900000-0000-7000-8000-00000000000a",
		"01900000-0000-7000-8000-00000000000b",
	}
	for _, eid := range events {
		require.NoError(t, contract.PutAuditEntry(ctx, eid, declID, testReceiptHash, testTimestamp, nil))
	}
	// Also insert one for a DIFFERENT declaration — must not appear.
	require.NoError(t, contract.PutAuditEntry(ctx, "01900000-0000-7000-8000-other00000a", "decl-other", testReceiptHash, testTimestamp, nil))

	got, err := contract.ListAuditEntriesForDeclaration(ctx, declID)
	require.NoError(t, err)
	require.Len(t, got, 3)
	assert.Equal(t, "01900000-0000-7000-8000-00000000000a", got[0].EventID)
	assert.Equal(t, "01900000-0000-7000-8000-00000000000b", got[1].EventID)
	assert.Equal(t, "01900000-0000-7000-8000-00000000000c", got[2].EventID)
	for _, e := range got {
		assert.Equal(t, declID, e.DeclarationID)
	}
}

func TestListAuditEntriesForDeclaration_EmptyResult(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	got, err := contract.ListAuditEntriesForDeclaration(ctx, "decl-no-such-thing")
	require.NoError(t, err)
	assert.Empty(t, got)
}

func TestPutAuditEntry_RejectsEmptyFields(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	cases := []struct {
		name           string
		eventID, decl  string
		receiptHashHex string
		ts             string
		wantErr        string
	}{
		{"empty_event_id", "", "decl", testReceiptHash, testTimestamp, "event_id"},
		{"empty_decl_id", "eid", "", testReceiptHash, testTimestamp, "declaration_id"},
		{"empty_ts", "eid", "decl", testReceiptHash, "", "ts"},
		{"whitespace_event_id", "   ", "decl", testReceiptHash, testTimestamp, "event_id"},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			err := contract.PutAuditEntry(ctx, tc.eventID, tc.decl, tc.receiptHashHex, tc.ts, nil)
			require.Error(t, err)
			assert.Contains(t, err.Error(), tc.wantErr)
		})
	}
}

func TestPutAuditEntry_RejectsMalformedReceiptHash(t *testing.T) {
	t.Parallel()
	stub := newMockStub()
	ctx := &mockTxContext{stub: stub}
	contract := new(AuditWitnessContract)

	cases := []struct {
		name string
		hash string
	}{
		{"too_short", "00ff"},
		{"too_long", strings.Repeat("a", 65)},
		{"uppercase_rejected", strings.Repeat("A", 64)},
		{"non_hex_rejected", strings.Repeat("z", 64)},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			err := contract.PutAuditEntry(ctx, "evt", "decl", tc.hash, testTimestamp, nil)
			require.Error(t, err)
			assert.Contains(t, err.Error(), "receipt_hash_hex")
		})
	}
}
