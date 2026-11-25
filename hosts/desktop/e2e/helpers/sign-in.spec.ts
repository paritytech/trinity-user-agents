import { describe, expect, it, vi } from 'vitest';

import { PairingLimitError, StuckPairingError, shouldResetSigner, signInWithReset, withSignInRetries } from './sign-in';

/**
 * A signer double built from plain functions rather than `vi.mock`. `signInWithReset`
 * declares only the two methods it calls, so this satisfies the parameter structurally
 * and needs no type assertion — the seam is already in the language.
 */
function fakeSigningHost() {
  const stop = vi.fn().mockResolvedValue(undefined);
  const removeDevices = vi.fn().mockResolvedValue(undefined);

  return { host: { stop, removeDevices }, stop, removeDevices };
}

describe('shouldResetSigner', () => {
  it('resets on a wedged pairing', () => {
    expect(shouldResetSigner(new StuckPairingError(60))).toBe(true);
  });

  it('resets on an exhausted slot budget', () => {
    expect(shouldResetSigner(new PairingLimitError())).toBe(true);
  });

  it('does not reset on an unrelated error', () => {
    expect(shouldResetSigner(new Error('boom'))).toBe(false);
  });
});

describe('withSignInRetries on PairingLimitError', () => {
  it('aborts remaining attempts — the daily slot budget cannot recover within a run', async () => {
    let calls = 0;
    const attempt = async (): Promise<void> => {
      calls++;
      throw new PairingLimitError();
    };

    await expect(withSignInRetries(attempt, { label: 'test', attempts: 3, delayMs: 1 })).rejects.toBeInstanceOf(
      PairingLimitError,
    );
    expect(calls).toBe(1);
  });
});

describe('signInWithReset', () => {
  it('clears the signer pairings and retries once the first phase exhausts', async () => {
    const { host, stop, removeDevices } = fakeSigningHost();
    let calls = 0;
    const attempt = async (): Promise<void> => {
      calls++;
      // The budget error aborts the first phase after one attempt; the post-reset
      // phase then succeeds, which is the behaviour the reset exists to produce.
      if (calls === 1) throw new PairingLimitError();
    };

    await signInWithReset({ label: 'test', signingHost: host, attempt, retryDelayMs: 1 });

    expect(calls).toBe(2);
    expect(stop).toHaveBeenCalledOnce();
    expect(removeDevices).toHaveBeenCalledOnce();
    // Ordering is load-bearing: `truapi-host` cannot clear pairings while serving.
    expect(stop.mock.invocationCallOrder[0]).toBeLessThan(removeDevices.mock.invocationCallOrder[0] ?? 0);
  });

  it('rethrows an unrelated error without touching the signer', async () => {
    const { host, stop, removeDevices } = fakeSigningHost();
    let calls = 0;
    const attempt = async (): Promise<void> => {
      calls++;
      throw new Error('boom');
    };

    await expect(signInWithReset({ label: 'test', signingHost: host, attempt, retryDelayMs: 1 })).rejects.toThrow('boom');
    // First phase is 2 attempts; a non-reset error must not buy a third.
    expect(calls).toBe(2);
    expect(stop).not.toHaveBeenCalled();
    expect(removeDevices).not.toHaveBeenCalled();
  });

  it('resets only after the first phase is spent, and re-cleans storage before retrying', async () => {
    const { host, stop, removeDevices } = fakeSigningHost();
    const order: string[] = [];
    let calls = 0;
    // Two wedged attempts exhaust the 2-attempt first phase — one is not enough,
    // because an ordinary retry would clear it without ever reaching the reset.
    const attempt = async (): Promise<void> => {
      calls++;
      order.push(`attempt${calls}`);
      if (calls <= 2) throw new StuckPairingError(60);
    };

    stop.mockImplementation(async () => {
      order.push('stop');
    });
    removeDevices.mockImplementation(async () => {
      order.push('removeDevices');
    });

    await signInWithReset({
      label: 'test',
      signingHost: host,
      attempt,
      beforeRetry: async () => {
        order.push('beforeRetry');
      },
      retryDelayMs: 1,
    });

    expect(order).toEqual(['attempt1', 'beforeRetry', 'attempt2', 'stop', 'removeDevices', 'beforeRetry', 'attempt3']);
  });
});
