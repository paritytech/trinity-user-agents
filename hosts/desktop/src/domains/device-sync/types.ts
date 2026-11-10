/**
 * Public domain types for the device-sync feature. Wire types live in `codec.ts`;
 * these are storage / runtime types.
 */

export type KnownUserDeviceStatus = 'active' | 'removed';

export type KnownUserDevice = {
  statementAccountId: string; // hex
  encryptionPublicKey: string; // hex (X25519, 32 bytes)
  status: KnownUserDeviceStatus;
  lastUpdate: number; // ms since epoch
  outgoingUpdateTime: number; // last acked timePoint we sent to this peer
  /**
   * UUID of the most-recent device-sync signaling attempt acknowledged by
   * both ends. ABSENT until the first successful Offer/Answer round-trip:
   *
   *  - Acceptor writes it the moment it ADOPTS an incoming Offer's offerId
   *    (the Offer carries an offerId we know the initiator has minted).
   *  - Initiator writes it only after RECEIVING an Answer — proof the
   *    acceptor saw and adopted the same offerId.
   *
   * On (re)start we read this and send `reconnected(offerId)` so the peer
   * disposes the matching stale attempt instead of letting a 45s handshake
   * timeout drive the recovery.
   */
  lastOfferId?: string;
};

export type ChatIdValue = { type: 'contact'; accountId: string }; // SS58

/** Per-peer connection phase emitted by the orchestrator (engine-internal vocabulary). */
export type DeviceSyncConnectionPhase = 'inactive' | 'connecting' | 'syncing' | 'synced' | 'disconnected' | 'error';

/** Activity reported by the per-pair sync state machine. */
export type SyncActivity = 'active' | 'idle' | 'error';

/**
 * Semantic sync status for the tracked peer. The consuming feature maps this to
 * icon / copy / visibility — the domain does not decide whether anything renders.
 */
export type DeviceSyncStatus = 'inactive' | 'syncing' | 'synced' | 'stale' | 'error';

/** Persisted device-sync connection metadata (survives reloads). */
export type DeviceSyncConnectionMeta = {
  id: 'default';
  lastConnectionClosedAt: number | null;
};
