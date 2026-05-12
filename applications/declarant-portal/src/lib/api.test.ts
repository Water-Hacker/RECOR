/**
 * Tests for the Declaration service client.
 *
 * The schema tests are the contract — they assert the portal's view
 * of the server response. If the server's wire format changes in a
 * non-backwards-compatible way, these tests fail. The
 * `isTerminalVerificationState` helper is the contract for when the
 * UI stops polling.
 */

import { describe, expect, it } from 'vitest';

import { isTerminalVerificationState } from './api';

describe('isTerminalVerificationState', () => {
  it('returns true for accepted', () => {
    expect(isTerminalVerificationState('accepted')).toBe(true);
  });

  it('returns true for rejected', () => {
    expect(isTerminalVerificationState('rejected')).toBe(true);
  });

  it('returns false for in_verification (yellow lane awaits analyst)', () => {
    expect(isTerminalVerificationState('in_verification')).toBe(false);
  });

  it('returns false for pending (engine has not run yet)', () => {
    expect(isTerminalVerificationState('pending')).toBe(false);
  });

  it('returns false for not_verified (initial column default)', () => {
    expect(isTerminalVerificationState('not_verified')).toBe(false);
  });

  it('returns false for unknown states (defensive)', () => {
    expect(isTerminalVerificationState('on_fire')).toBe(false);
    expect(isTerminalVerificationState('')).toBe(false);
  });
});
