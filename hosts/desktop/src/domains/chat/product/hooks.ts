import { useMemo } from 'react';

import { useRead } from '@/shared/hooks';
import { type AccountId } from '@/domains/network';
import { productRoomUseCase } from '../$usecase/productRoom';
import { type UserPeer } from '../session/types';

import { roomsResource } from './resource';
import { productChatService } from './service';

// function formatDate(timestamp: number): string {
//   const date = new Date(timestamp);
//   const today = new Date();
//   const yesterday = new Date(today);
//   yesterday.setDate(yesterday.getDate() - 1);
//
//   if (date.toDateString() === today.toDateString()) {
//     return 'Today';
//   } else if (date.toDateString() === yesterday.toDateString()) {
//     return 'Yesterday';
//   } else {
//     return date.toLocaleDateString('en-US', { day: 'numeric', month: 'long', year: 'numeric' });
//   }
// }

/**
 * The signed-in user as a chat peer.
 *
 * `userId` and `name` are parameters because the session lives in the
 * `truapi-runtime` aggregate, which a domain may not import. The calling feature
 * reads them (`useTruapiUserId` / `useTruapiSession`) and passes them down.
 */
export const useCurrentUserPeer = (userId: Nullable<AccountId>, name = '') => {
  const peer = useMemo((): UserPeer | null => {
    if (!userId) return null;

    return { type: 'user', accountId: userId, name };
  }, [userId, name]);

  return { data: peer };
};

// The current user's product chat rooms (across all products), live from the DB.
const useUserRooms = (userId: Nullable<AccountId>, name = '') => {
  const { data: peer } = useCurrentUserPeer(userId, name);
  const {
    data: rooms,
    pending: pendingRooms,
    error,
  } = useRead(roomsResource, {
    params: peer,
    defaultValue: [],
    map: (cache, { accountId }) => cache[accountId],
  });

  return { peer, rooms, pending: pendingRooms, error };
};

export const useProductSessions = (userId: Nullable<AccountId>, name = '') => {
  const { peer, rooms, pending, error } = useUserRooms(userId, name);

  const sessions = useMemo(
    () => (peer ? rooms.map(r => productRoomUseCase.createProductChatSession(peer, r)) : []),
    [peer, rooms],
  );

  return { data: sessions, pending, error };
};

// The current user's product chat rooms across every product — each carries its
// `productId` and `sessionId`, so callers that handle many products at once (e.g.
// a dashboard grid) can look up a product's room without a per-product hook.
export const useUserProductRooms = (userId: Nullable<AccountId>) => {
  const { rooms, pending, error } = useUserRooms(userId);
  return { data: rooms, pending, error };
};

// The current user's chat rooms for a single product.
export const useProductRooms = (productId: Nullable<string>, userId: Nullable<AccountId>) => {
  const { rooms, pending, error } = useUserRooms(userId);

  const data = useMemo(
    () => (productId ? rooms.filter(room => productChatService.belongsToProduct(room, productId)) : []),
    [rooms, productId],
  );

  return { data, pending, error };
};
