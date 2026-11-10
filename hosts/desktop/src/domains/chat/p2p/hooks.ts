import { AccountId } from '@polkadot-api/substrate-bindings';
import { useMemo } from 'react';

import { useRead } from '@/shared/hooks';
import { useDeviceIdentity } from '@/domains/device';

import { p2pRequestsResource, p2pRoomsResource } from './resource';
import { p2pService } from './service';
import { type P2PRoom } from './types';

/**
 * Pure-resource reads over the P2P chat tables. Manager-dependent hooks
 * (sessions, contact search, request actions) and the manager lifecycle live
 * in the `p2p-chat` aggregate — the domain owns the data layer, not the
 * runtime manager.
 */

// P2P rooms are keyed by SS58 of this device's statement account, which the host
// now mints itself (`@/domains/device`). It is a device-scoped id, not a user one:
// two installs of the same account hold separate room sets, which is what the
// multi-device protocol syncs between.
const useCurrentUserId = (): string | null => {
  const identity = useDeviceIdentity();

  return useMemo(() => {
    if (!identity) return null;

    return AccountId().dec(identity.statementAccountPublicKey);
  }, [identity]);
};

/**
 * Live P2P rooms for the signed-in user.
 */
export const useP2PRooms = (): { data: P2PRoom[]; pending: boolean; error: unknown } => {
  const userId = useCurrentUserId();

  return useRead(p2pRoomsResource, {
    params: userId ? { userId } : null,
    defaultValue: [],
    map: (cache, { userId: uid }) => cache[uid],
  });
};

/**
 * Returns pending P2P chat requests (incoming + outgoing).
 */
export const useP2PRequests = () => {
  const userId = useCurrentUserId();

  const { data: requests, pending: pendingRequests } = useRead(p2pRequestsResource, {
    params: userId ? { userId } : null,
    defaultValue: [],
    map: (cache, { userId: uid }) => cache[uid],
  });

  const pendingIncoming = useMemo(() => requests.filter(p2pService.isPendingIncomingRequest), [requests]);

  const pendingOutgoing = useMemo(() => requests.filter(p2pService.isPendingOutgoingRequest), [requests]);

  return { data: pendingIncoming, outgoing: pendingOutgoing, pending: pendingRequests };
};
