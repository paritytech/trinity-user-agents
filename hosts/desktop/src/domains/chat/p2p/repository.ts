import { createDexieDatabase } from '@/shared/dexie';
import { removeLocalStorageKeysByPrefix } from '@/shared/utils';
import { type ChatMessage } from '../session/types';

import { type P2PChatRequest, type P2POutboxEntry, type P2PRoom } from './types';

// HOP `hop_claim` evicts the entry server-side on success, so a single
// download empties the pool for that identifier. Without local persistence
// every chat reopen would 404 against the bulletin server. We keep the
// decrypted bytes in IndexedDB keyed by hex(identifier).
export type DownloadedFileBlob = {
  identifierHex: string;
  mimeType: string;
  bytes: Uint8Array;
  downloadedAt: number;
};

export const p2pChatDatabase = createDexieDatabase<{
  rooms: P2PRoom;
  messages: ChatMessage;
  requests: P2PChatRequest;
  // Dormant V1 table — no writers since the V2 session manager landed, and the
  // localStorage records that replaced it are gone too. Dropping it needs a
  // schema version bump — separate cleanup.
  outbox: P2POutboxEntry;
  downloadedFiles: DownloadedFileBlob;
}>({
  name: 'p2p-chat',
  version: 3,
  schema: {
    rooms: 'sessionId, peerId, userId, lastUpdate',
    messages: 'messageId, sessionId, lastUpdate',
    requests: 'requestId, peerId, userId, lastUpdate',
    outbox: 'id, peerId',
    downloadedFiles: 'identifierHex, downloadedAt',
  },
});

/**
 * Wipe every P2P chat row. Called on logout so one user's chat history
 * doesn't bleed into the next user paired on this device, and so it isn't
 * readable from DevTools / file-system after the user has signed out.
 *
 * `onPairingSuccess` already clears contacts + device-sync on a new
 * handshake, but that only runs if the user re-pairs — a plain logout
 * leaves these rows in place. The matching multi-device cleanup lives in
 * `papp-provider/hooks.ts::onV2Disconnect`.
 */
export const clearAllP2PChatStorage = async (): Promise<void> => {
  clearAllOutboxRecords();
  await Promise.all([
    p2pChatDatabase.rooms.clear(),
    p2pChatDatabase.messages.clear(),
    p2pChatDatabase.requests.clear(),
    p2pChatDatabase.outbox.clear(),
    p2pChatDatabase.downloadedFiles.clear(),
  ]);
};

// ── Legacy V2 outbox records (localStorage) ─────────────────────────────
// One JSON record per (user, peer), holding the unacked batch. Nothing writes these
// any more: the statement store holds that batch and the session restores it at init.
// Only the purge remains, so records written by older builds do not linger.

const OUTBOX_KEY_PREFIX = 'p2p-chat-outbox:v1';

export function clearAllOutboxRecords(): void {
  removeLocalStorageKeysByPrefix(OUTBOX_KEY_PREFIX);
}

// ── Changed-since queries ───────────────────────────────────────────────
// device-sync's collector uses these to gather entities whose `lastUpdate`
// advanced past the last sync watermark. Write-side callers bump
// `lastUpdate` via `p2pService.stamp*` so mutations stay visible to sync.

export function listMessagesChangedSince(timestamp: number): Promise<ChatMessage[]> {
  return p2pChatDatabase.messages.where('lastUpdate').above(timestamp).toArray();
}

/** Full-table read for the chat search use case. */
export function listAllP2PMessages(): Promise<ChatMessage[]> {
  return p2pChatDatabase.messages.toArray();
}

export function listRoomsChangedSince(timestamp: number): Promise<P2PRoom[]> {
  return p2pChatDatabase.rooms.where('lastUpdate').above(timestamp).toArray();
}

export function listRequestsChangedSince(timestamp: number): Promise<P2PChatRequest[]> {
  return p2pChatDatabase.requests.where('lastUpdate').above(timestamp).toArray();
}

// ── Blocking ────────────────────────────────────────────────────────────

/**
 * Peers this user has blocked on this device. The single definition of "blocked":
 * both enforcement paths — the live V2 session manager and the device-sync applier —
 * read the rule from here rather than re-deriving it against `isBlocked`.
 *
 * Block state is device-local (no `SyncEntity` carries it), so a paired sibling that
 * hasn't blocked the peer keeps replicating their messages to us.
 */
export async function listBlockedPeerIds(userId: string): Promise<string[]> {
  const blockedRooms = await p2pChatDatabase.rooms
    .where('userId')
    .equals(userId)
    .filter(room => room.isBlocked === true)
    .toArray();

  return blockedRooms.map(room => room.peerId);
}
