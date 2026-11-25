// @vitest-environment happy-dom

import { renderHook, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type ProductChatRoom, roomsResource } from '@/domains/chat';
import { type AccountId } from '@/domains/network';
import { onProductModalityOpenedSideEffect } from '@/domains/product';
import { truapiRuntime } from '@/aggregates/truapi-runtime';

import { useAnnounceProductRoomOpen } from './useAnnounceProductRoomOpen';

// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test fixture, not production code
const USER_ID = '0x2222222222222222222222222222222222222222222222222222222222222222' as AccountId;

const room = (sessionId: string, productId: string): ProductChatRoom => ({
  sessionId,
  roomId: sessionId,
  productId,
  userId: USER_ID,
  createdAt: 1000,
});

const applySpy = vi.spyOn(onProductModalityOpenedSideEffect, 'apply');

// The rooms read is keyed by the signed-in user, so a connected session is what
// makes it run at all. Its asynchronous settle is the timing the mount guard has
// to survive.
beforeEach(() => {
  truapiRuntime.set(prev => ({
    ...prev,
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test fixture, not production code
    authState: { tag: 'Connected', value: { identityAccountId: USER_ID, publicKey: USER_ID } } as never,
  }));
  roomsResource.instead(() => of([room('s1', 'app.dot'), room('s2', 'other.dot')]));
  applySpy.mockClear();
});

afterEach(() => {
  vi.clearAllMocks();
});

const settleRooms = () => waitFor(() => expect(Object.keys(roomsResource.snapshot()).length).toBeGreaterThan(0));

describe('useAnnounceProductRoomOpen', () => {
  it('does not fire on the initial mount for a restored product room', async () => {
    renderHook(() => useAnnounceProductRoomOpen('s1'));

    await settleRooms();
    expect(applySpy).not.toHaveBeenCalled();
  });

  it('fires worker-open when a product room becomes the open session after mount', async () => {
    const { rerender } = renderHook<void, { id: string | null }>(({ id }) => useAnnounceProductRoomOpen(id), {
      initialProps: { id: null },
    });
    await settleRooms();
    expect(applySpy).not.toHaveBeenCalled();

    rerender({ id: 's1' });
    await waitFor(() => expect(applySpy).toHaveBeenCalledWith({ productId: 'app.dot', kind: 'worker' }));
  });

  it('fires again when the user returns to the room that was open at mount', async () => {
    const { rerender } = renderHook<void, { id: string | null }>(({ id }) => useAnnounceProductRoomOpen(id), {
      initialProps: { id: 's1' },
    });
    await settleRooms();

    rerender({ id: 's2' });
    await waitFor(() => expect(applySpy).toHaveBeenCalledWith({ productId: 'other.dot', kind: 'worker' }));

    rerender({ id: 's1' });
    await waitFor(() => expect(applySpy).toHaveBeenCalledWith({ productId: 'app.dot', kind: 'worker' }));
  });

  it('fires once per open, not on every rooms re-emit', async () => {
    const { rerender } = renderHook<void, { id: string | null }>(({ id }) => useAnnounceProductRoomOpen(id), {
      initialProps: { id: null },
    });
    await settleRooms();

    rerender({ id: 's1' });
    await waitFor(() => expect(applySpy).toHaveBeenCalledTimes(1));

    rerender({ id: 's1' });
    expect(applySpy).toHaveBeenCalledTimes(1);
  });

  it('does not fire for a P2P session with no product room', async () => {
    const { rerender } = renderHook<void, { id: string | null }>(({ id }) => useAnnounceProductRoomOpen(id), {
      initialProps: { id: null },
    });
    await settleRooms();

    rerender({ id: 'peer-xyz' });
    expect(applySpy).not.toHaveBeenCalled();
  });

  it('does not fire when nothing is selected', async () => {
    const { rerender } = renderHook<void, { id: string | null }>(({ id }) => useAnnounceProductRoomOpen(id), {
      initialProps: { id: null },
    });
    await settleRooms();

    rerender({ id: null });
    expect(applySpy).not.toHaveBeenCalled();
  });
});
