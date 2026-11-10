// @vitest-environment happy-dom

import { act, renderHook, waitFor } from '@testing-library/react';
import { type BehaviorSubject, of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// `online$` is mocked as a BehaviorSubject so the test can drive offline/online
// transitions that feed the "internet restored" window.
vi.mock('@/shared/env', async () => {
  const { BehaviorSubject } = await import('rxjs');
  return { online$: new BehaviorSubject(true) };
});

import { online$ } from '@/shared/env';
import { type ConnectionStatus, chainConnectionStatusResource } from '@/domains/network';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { useOnboardingConnection } from './useOnboardingConnection';

const onlineSubject = online$ as unknown as BehaviorSubject<boolean>;

// Both aggregate hooks run for real: the people-chain status is stated on the network
// resource it reads, and the auth state is published through the runtime it watches.
const chainStatus = (status: ConnectionStatus) => chainConnectionStatusResource.instead(() => of(status));

beforeEach(() => {
  onlineSubject.next(true);
  truapiRuntimeUseCase.publishAuthState({ tag: 'Pairing', value: { deeplink: 'polkadotapp://pair?handshake=0x00' } });
  chainStatus('connecting');
});

afterEach(() => {
  truapiRuntimeUseCase.dispose();
  vi.useRealTimers();
});

describe('useOnboardingConnection', () => {
  it('returns reaching when reconnecting without a recent internet restore', async () => {
    const { result } = renderHook(() => useOnboardingConnection());
    await waitFor(() => expect(result.current).toBe('reaching'));
  });

  it('shows restored briefly after the browser comes back online, then reaching', async () => {
    const { result } = renderHook(() => useOnboardingConnection());
    await waitFor(() => expect(result.current).toBe('reaching'));

    vi.useFakeTimers();
    act(() => {
      onlineSubject.next(false);
      onlineSubject.next(true);
    });
    expect(result.current).toBe('restored');

    act(() => {
      vi.advanceTimersByTime(1600);
    });
    expect(result.current).toBe('reaching');
  });

  it('returns accountSetup when connected with a recognized identity error', async () => {
    chainStatus('connected');
    truapiRuntimeUseCase.publishAuthState({ tag: 'LoginFailed', value: { reason: 'OriginPersonProviderError' } as never });
    const { result } = renderHook(() => useOnboardingConnection());
    await waitFor(() => expect(result.current).toBe('accountSetup'));
  });

  // Last: `justRestored$` is a module-level stream, so the offline → online flip the next
  // `beforeEach` performs would open a "restored" window over whichever case followed.
  it('returns offline when the browser is offline', async () => {
    onlineSubject.next(false);
    const { result } = renderHook(() => useOnboardingConnection());
    await waitFor(() => expect(result.current).toBe('offline'));
  });
});
