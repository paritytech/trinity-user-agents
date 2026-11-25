export { useP2PRequests, useP2PRooms } from './hooks';
export { p2pService } from './service';
export type { P2PChatManager, P2PChatRequest, P2PPeer, P2PRoom, SearchResult } from './types';

export { SUBSCRIPTION_BUDGET, subscriptionRegistry, trackedSubscribeStatements } from './subscription-registry';

export { clearAllOutboxRecords, clearAllP2PChatStorage, listBlockedPeerIds } from './repository';

// Cross-source orchestration primitives consumed by the `p2p-chat` aggregate.
export { createP2PChatManagerV2 } from './managerV2Factory';
export { createP2PChatSession } from './session';
export { isMessageTooLargeError } from './chatSessionV2';
export { useNotificationService } from './notifications/hooks';
