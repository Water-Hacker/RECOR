/**
 * Tests for the VerificationStatus polling view.
 *
 * Strategy: mock global `fetch` to return controlled responses;
 * wrap the component in a QueryClient. The polling itself is
 * driven by TanStack Query's refetchInterval, which honours the
 * `staleTime: 0` config — first render kicks off a fetch.
 */

import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { VerificationStatus } from './VerificationStatus';
import type { SubmitDeclarationResponse } from '../../lib/api';

const RECEIPT: SubmitDeclarationResponse = {
  declaration_id: '018f0000-0000-4000-8000-000000000001',
  state: 'submitted',
  receipt_hash_hex: 'a'.repeat(64),
  submitted_at: '2026-05-12T01:00:00Z',
  receipt_url: 'http://test/v1/declarations/018f0000-0000-4000-8000-000000000001',
};

function renderWithClient(ui: React.ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={client}>{ui}</QueryClientProvider>,
  );
}

function mockGetDeclarationResponse(body: Record<string, unknown>) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
}

describe('VerificationStatus', () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('renders the receipt fields immediately from props (no fetch required)', async () => {
    mockGetDeclarationResponse({
      declaration_id: RECEIPT.declaration_id,
      entity_id: '018f0000-0000-4000-8000-0000000000aa',
      declarant_principal: 'spiffe://recor.cm/test',
      state: 'submitted',
      aggregate_version: 1,
      submitted_at: RECEIPT.submitted_at,
      receipt_hash_hex: RECEIPT.receipt_hash_hex,
      verification_state: 'pending',
    });

    renderWithClient(
      <VerificationStatus
        apiBaseUrl="http://test"
        declarantPrincipal="spiffe://recor.cm/test"
        receipt={RECEIPT}
      />,
    );

    // Receipt hash + declaration id render from props synchronously.
    expect(screen.getByText(RECEIPT.declaration_id)).toBeInTheDocument();
    expect(screen.getByText(RECEIPT.receipt_hash_hex)).toBeInTheDocument();
  });

  it('shows accepted state with green-lane verification metadata', async () => {
    mockGetDeclarationResponse({
      declaration_id: RECEIPT.declaration_id,
      entity_id: '018f0000-0000-4000-8000-0000000000aa',
      declarant_principal: 'spiffe://recor.cm/test',
      state: 'accepted',
      aggregate_version: 2,
      submitted_at: RECEIPT.submitted_at,
      receipt_hash_hex: RECEIPT.receipt_hash_hex,
      verification_state: 'accepted',
      verification_lane: 'green',
      verification_case_id: '019e1a00-0000-7000-8000-000000000001',
      verified_at: '2026-05-12T01:01:00Z',
    });

    renderWithClient(
      <VerificationStatus
        apiBaseUrl="http://test"
        declarantPrincipal="spiffe://recor.cm/test"
        receipt={RECEIPT}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText('Verification accepted')).toBeInTheDocument();
    });
    expect(screen.getByTestId('status-badge')).toHaveTextContent('green');
    expect(screen.getByText('019e1a00-0000-7000-8000-000000000001')).toBeInTheDocument();
  });

  it('shows rejected state with red-lane', async () => {
    mockGetDeclarationResponse({
      declaration_id: RECEIPT.declaration_id,
      entity_id: '018f0000-0000-4000-8000-0000000000aa',
      declarant_principal: 'spiffe://recor.cm/test',
      state: 'rejected',
      aggregate_version: 2,
      submitted_at: RECEIPT.submitted_at,
      receipt_hash_hex: RECEIPT.receipt_hash_hex,
      verification_state: 'rejected',
      verification_lane: 'red',
      verification_case_id: '019e1a00-0000-7000-8000-000000000002',
      verified_at: '2026-05-12T01:01:00Z',
    });

    renderWithClient(
      <VerificationStatus
        apiBaseUrl="http://test"
        declarantPrincipal="spiffe://recor.cm/test"
        receipt={RECEIPT}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText('Verification rejected')).toBeInTheDocument();
    });
    expect(screen.getByTestId('status-badge')).toHaveTextContent('red');
  });

  it('shows in_verification state for yellow-lane and KEEPS polling indicator', async () => {
    mockGetDeclarationResponse({
      declaration_id: RECEIPT.declaration_id,
      entity_id: '018f0000-0000-4000-8000-0000000000aa',
      declarant_principal: 'spiffe://recor.cm/test',
      state: 'in_verification',
      aggregate_version: 2,
      submitted_at: RECEIPT.submitted_at,
      receipt_hash_hex: RECEIPT.receipt_hash_hex,
      verification_state: 'in_verification',
      verification_lane: 'yellow',
      verification_case_id: '019e1a00-0000-7000-8000-000000000003',
      verified_at: '2026-05-12T01:01:00Z',
    });

    renderWithClient(
      <VerificationStatus
        apiBaseUrl="http://test"
        declarantPrincipal="spiffe://recor.cm/test"
        receipt={RECEIPT}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText('Under analyst review')).toBeInTheDocument();
    });
    // Polling indicator stays visible because in_verification is NOT
    // terminal — an analyst will eventually move it to accepted/rejected.
    expect(screen.getByTestId('polling-indicator')).toBeInTheDocument();
  });

  it('surfaces supersedes_declaration_id when present', async () => {
    const supersededId = '018f0000-0000-4000-8000-00000000beef';
    mockGetDeclarationResponse({
      declaration_id: RECEIPT.declaration_id,
      entity_id: '018f0000-0000-4000-8000-0000000000aa',
      declarant_principal: 'spiffe://recor.cm/test',
      state: 'submitted',
      aggregate_version: 1,
      submitted_at: RECEIPT.submitted_at,
      receipt_hash_hex: RECEIPT.receipt_hash_hex,
      verification_state: 'pending',
      supersedes_declaration_id: supersededId,
    });

    renderWithClient(
      <VerificationStatus
        apiBaseUrl="http://test"
        declarantPrincipal="spiffe://recor.cm/test"
        receipt={RECEIPT}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText(supersededId)).toBeInTheDocument();
    });
    expect(screen.getByText('Supersedes declaration')).toBeInTheDocument();
  });

  it('renders pending state with neutral styling when no projection yet', async () => {
    // Slow fetch — the component renders the receipt-only view first.
    let resolveFetch: ((response: Response) => void) | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn(
        () =>
          new Promise<Response>((resolve) => {
            resolveFetch = resolve;
          }),
      ),
    );

    renderWithClient(
      <VerificationStatus
        apiBaseUrl="http://test"
        declarantPrincipal="spiffe://recor.cm/test"
        receipt={RECEIPT}
      />,
    );

    // While fetch pending, we show "Awaiting verification" (pending) +
    // the polling indicator.
    expect(screen.getByText('Awaiting verification')).toBeInTheDocument();
    expect(screen.getByTestId('polling-indicator')).toBeInTheDocument();

    // Clean up the dangling promise so vitest doesn't complain about
    // unresolved handles.
    resolveFetch?.(
      new Response(
        JSON.stringify({
          declaration_id: RECEIPT.declaration_id,
          entity_id: '018f0000-0000-4000-8000-0000000000aa',
          declarant_principal: 'spiffe://recor.cm/test',
          state: 'submitted',
          aggregate_version: 1,
          submitted_at: RECEIPT.submitted_at,
          receipt_hash_hex: RECEIPT.receipt_hash_hex,
          verification_state: 'pending',
        }),
        { status: 200 },
      ),
    );
  });
});
