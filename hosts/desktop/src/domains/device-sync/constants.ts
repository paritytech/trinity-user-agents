/** The sync status becomes `stale` after this long without an active WebRTC session. */
export const DEVICE_SYNC_STALE_AFTER_MS = 60 * 60 * 1000;

/** How often the UI status stream re-evaluates the stale-disconnect rule. */
export const DEVICE_SYNC_UI_TICK_MS = 60_000;

/**
 * Budget for a single connection attempt before it is torn down and rebuilt.
 * WebRTC does not reliably drive a never-progressed peer connection to `failed`,
 * so an initiator that sent an Offer and never got a usable Answer would
 * otherwise wait forever. Android parity: `DeviceSyncEngine.CONNECT_TIMEOUT`.
 */
export const DEVICE_SYNC_HANDSHAKE_TIMEOUT_MS = 30_000;

/**
 * Pause between a torn-down attempt and its rebuild. Android parity:
 * `DeviceSyncEngine.RECONNECT_BACKOFF`. iOS instead ramps 1s→30s, but the
 * desktop is the always-on side of the pair, so it stays the eager one.
 */
export const DEVICE_SYNC_RESPAWN_BACKOFF_MS = 5_000;

/**
 * Consecutive failed handshakes to the PApp peer before the connection phase
 * escalates to `error`. At ~35s per attempt
 * ({@link DEVICE_SYNC_HANDSHAKE_TIMEOUT_MS} + {@link DEVICE_SYNC_RESPAWN_BACKOFF_MS})
 * this surfaces a failure within ~2 min while tolerating one slow cold start /
 * transient blip. Not terminal — a successful data-channel open resets the count.
 */
export const DEVICE_SYNC_MAX_FAILED_HANDSHAKES = 3;

/**
 * Entity-list budget for a single `SyncUpdate`, in bytes.
 *
 * Spec: "A single `SyncUpdate` SHOULD be kept small enough for its SCALE-encoded
 * size to fit below the data channel's maximum message size (SCTP
 * `max-message-size`, 64 KB by default): a snapshot of entities SHOULD be split
 * into multiple `SyncUpdate`s when necessary."
 *
 * iOS parity (`DeviceSyncEntityChunker.defaultMaxPayloadSize`). Exceeding the
 * negotiated SCTP max-message-size aborts the association (RFC 8831 §6.6), which
 * surfaces as the data channel silently closing mid-sync — and because
 * `outgoingUpdateTime` only advances on Ack, the next attempt re-collects the
 * same oversized snapshot and kills the channel again. 60 KB leaves room for the
 * `SyncMessage` tag, the `SyncUpdate` id/timePoint and the data-channel envelope.
 */
export const DEVICE_SYNC_MAX_UPDATE_BYTES = 60 * 1024;
