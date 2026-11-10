import { useEffect, useRef } from 'react';

import { useUserProductRooms } from '@/domains/chat';
import { onProductModalityOpenedSideEffect } from '@/domains/product';
import { useTruapiUserId } from '@/aggregates/truapi-runtime';

// Announce a worker-modality open when the user opens a PRODUCT chat room. The
// open session id (from the /chat route) is resolved to its product via the
// product-rooms list; P2P sessions have no product room and fire nothing.
export const useAnnounceProductRoomOpen = (selectedSessionId: string | null) => {
  const { data: rooms, pending } = useUserProductRooms(useTruapiUserId());

  const productId = selectedSessionId ? (rooms.find(room => room.sessionId === selectedSessionId)?.productId ?? null) : null;

  // The session already open when this mounted: restoring it at launch is not a
  // fresh open, so it must not nag. Tracked as a session id rather than a
  // first-run flag because the rooms list resolves asynchronously — at mount every
  // session still resolves to no product, and a flag would be spent on that pass,
  // before the answer that identifies the restored room arrives.
  const announcedFor = useRef(selectedSessionId);

  useEffect(() => {
    if (selectedSessionId === announcedFor.current) return;
    // The rooms list has not answered for this session yet; `null` here means
    // "unknown", not "no product".
    if (productId === null && pending) return;

    announcedFor.current = selectedSessionId;
    if (productId) void onProductModalityOpenedSideEffect.apply({ productId, kind: 'worker' });
  }, [selectedSessionId, productId, pending]);
};
