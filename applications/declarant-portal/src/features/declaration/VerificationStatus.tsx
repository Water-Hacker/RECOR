/**
 * Post-submission view: polls the declaration's projection and
 * displays its current verification state. Replaces the previous
 * one-shot ReceiptDisplay so the declarant can see the full
 * lifecycle: submitted → in_verification → accepted | rejected.
 *
 * Polling:
 *   - Refetch every 3 seconds while the state is non-terminal
 *     (`not_verified`, `pending`, `in_verification`)
 *   - Stop polling once the state becomes terminal
 *     (`accepted`, `rejected`) — TanStack Query's refetchInterval
 *     accepts a function that returns `false` to halt
 *   - On network error, retry with backoff (TanStack Query default)
 *
 * The receipt header (declaration_id, BLAKE3 hash, submitted_at)
 * stays visible throughout — those are immutable commitments the
 * declarant can verify offline. Verification state is the live
 * dimension.
 *
 * i18n: every visible string flows through `t()` (R-PORT-1). The
 * status-lane "badge" displays the raw lane/state token (`green`,
 * `accepted`, …) deliberately — those tokens are protocol values and
 * are NOT translated, so analysts and declarants can cite the same
 * identifier across languages.
 */

import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import clsx from 'clsx';

import {
  getDeclaration,
  isTerminalVerificationState,
  type GetDeclarationResponse,
  type SubmitDeclarationResponse,
  type VerificationLane,
} from '../../lib/api';

interface VerificationStatusProps {
  apiBaseUrl: string;
  declarantPrincipal: string;
  receipt: SubmitDeclarationResponse;
}

export function VerificationStatus({
  apiBaseUrl,
  declarantPrincipal,
  receipt,
}: VerificationStatusProps) {
  const { t } = useTranslation();
  const query = useQuery<GetDeclarationResponse>({
    queryKey: ['declaration', receipt.declaration_id],
    queryFn: () =>
      getDeclaration(
        { baseUrl: apiBaseUrl, declarantPrincipal },
        receipt.declaration_id,
      ),
    refetchInterval: (q) => {
      const state = q.state.data?.verification_state;
      if (state && isTerminalVerificationState(state)) {
        return false;
      }
      return 3000;
    },
    // First render shows the receipt; the projection populates as it
    // arrives. Don't block on the first fetch.
    initialData: undefined,
    staleTime: 0,
  });

  const projection = query.data;
  const verificationState = projection?.verification_state ?? 'pending';
  const lane = projection?.verification_lane;

  return (
    <div
      role="status"
      aria-live="polite"
      data-testid="verification-status-panel"
      className={clsx(
        'space-y-4 rounded-lg border-2 p-6',
        statusContainerCls(verificationState, lane),
      )}
    >
      <header className="flex items-start justify-between gap-4">
        <div>
          <h2 className={clsx('text-2xl font-semibold', statusHeadingCls(verificationState))}>
            {statusHeading(t, verificationState, lane)}
          </h2>
          <p className="mt-1 text-sm text-slate-700">
            {statusDescription(t, verificationState)}
          </p>
        </div>
        <StatusBadge state={verificationState} lane={lane} />
      </header>

      <dl className="grid grid-cols-1 gap-3 text-sm md:grid-cols-2">
        <Receipt
          label={t('verification.receiptFields.declarationId')}
          value={receipt.declaration_id}
          mono
        />
        <Receipt
          label={t('verification.receiptFields.receiptHash')}
          value={receipt.receipt_hash_hex}
          mono
        />
        <Receipt
          label={t('verification.receiptFields.submittedAt')}
          value={receipt.submitted_at}
          mono
        />
        <Receipt
          label={t('verification.receiptFields.aggregateState')}
          value={projection?.state ?? receipt.state}
        />

        {projection?.verification_case_id ? (
          <Receipt
            label={t('verification.receiptFields.verificationCaseId')}
            value={projection.verification_case_id}
            mono
          />
        ) : null}
        {projection?.verified_at ? (
          <Receipt
            label={t('verification.receiptFields.verifiedAt')}
            value={projection.verified_at}
            mono
          />
        ) : null}
        {projection?.supersedes_declaration_id ? (
          <Receipt
            label={t('verification.receiptFields.supersedesDeclaration')}
            value={projection.supersedes_declaration_id}
            mono
          />
        ) : null}
        {projection?.superseded_by_declaration_id ? (
          <Receipt
            label={t('verification.receiptFields.supersededByDeclaration')}
            value={projection.superseded_by_declaration_id}
            mono
          />
        ) : null}
      </dl>

      <p className="text-sm text-slate-700">
        {t('verification.receiptKeepNotice')}
      </p>

      {!isTerminalVerificationState(verificationState) ? (
        <p
          className="text-xs text-slate-500"
          data-testid="polling-indicator"
        >
          {t('verification.pollingIndicator')}
        </p>
      ) : null}

      {query.isError ? (
        <p role="alert" className="text-sm text-red-800">
          {t('verification.refreshFailed', {
            message: (query.error as Error).message,
          })}
        </p>
      ) : null}

      <button
        type="button"
        onClick={() => window.location.reload()}
        className="rounded-md border border-slate-400 bg-white px-4 py-2 text-sm font-medium text-slate-800 hover:bg-slate-100"
      >
        {t('verification.fileAnotherButton')}
      </button>
    </div>
  );
}

function statusContainerCls(state: string, lane: VerificationLane | undefined): string {
  if (state === 'accepted' || lane === 'green') {
    return 'border-emerald-600 bg-emerald-50';
  }
  if (state === 'rejected' || lane === 'red') {
    return 'border-red-700 bg-red-50';
  }
  if (state === 'in_verification' || lane === 'yellow') {
    return 'border-amber-500 bg-amber-50';
  }
  // pending / not_verified / unknown — neutral
  return 'border-slate-300 bg-slate-50';
}

function statusHeadingCls(state: string): string {
  if (state === 'accepted') return 'text-emerald-900';
  if (state === 'rejected') return 'text-red-900';
  if (state === 'in_verification') return 'text-amber-900';
  return 'text-slate-900';
}

function statusHeading(
  t: TFunction,
  state: string,
  lane: VerificationLane | undefined,
): string {
  if (state === 'accepted') return t('verification.headings.accepted');
  if (state === 'rejected') return t('verification.headings.rejected');
  if (state === 'in_verification') {
    return t('verification.headings.in_verification');
  }
  if (state === 'pending') return t('verification.headings.pending');
  if (lane === 'green') return t('verification.headings.accepted');
  if (lane === 'red') return t('verification.headings.rejected');
  if (lane === 'yellow') return t('verification.headings.in_verification');
  return t('verification.headings.submitted');
}

function statusDescription(t: TFunction, state: string): string {
  switch (state) {
    case 'accepted':
      return t('verification.descriptions.accepted');
    case 'rejected':
      return t('verification.descriptions.rejected');
    case 'in_verification':
      return t('verification.descriptions.in_verification');
    case 'pending':
      return t('verification.descriptions.pending');
    default:
      return t('verification.descriptions.default');
  }
}

interface StatusBadgeProps {
  state: string;
  lane: VerificationLane | undefined;
}

function StatusBadge({ state, lane }: StatusBadgeProps) {
  // The badge intentionally shows the raw protocol token (`green`,
  // `accepted`, …) and is NOT translated — it's a stable identifier
  // analysts and declarants reference across languages.
  const display = lane ?? state;
  return (
    <span
      className={clsx(
        'rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-wider',
        badgeCls(state, lane),
      )}
      data-testid="status-badge"
    >
      {display}
    </span>
  );
}

function badgeCls(state: string, lane: VerificationLane | undefined): string {
  if (state === 'accepted' || lane === 'green') {
    return 'bg-emerald-700 text-white';
  }
  if (state === 'rejected' || lane === 'red') {
    return 'bg-red-700 text-white';
  }
  if (state === 'in_verification' || lane === 'yellow') {
    return 'bg-amber-600 text-white';
  }
  return 'bg-slate-600 text-white';
}

function Receipt({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <dt className="font-medium text-slate-900">{label}</dt>
      <dd className={clsx('mt-1 text-slate-700', mono && 'font-mono break-all')}>
        {value}
      </dd>
    </div>
  );
}
