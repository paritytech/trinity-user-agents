import { type CodecType } from 'scale-ts';

import { DEVICE_SYNC_MAX_UPDATE_BYTES, DEVICE_SYNC_STALE_AFTER_MS } from './constants';
import { SyncEntityCodec } from './schemas';
import { type DeviceSyncConnectionPhase, type DeviceSyncStatus } from './types';

type SyncEntity = CodecType<typeof SyncEntityCodec>;

/**
 * A collected entity plus the newest source timestamp it covers — `lastUpdate`
 * for contact-derived entities, `order`/`timestamp` for messages. The wire
 * `SyncEntity` carries no timestamp for `ChatsAdded`/`ChatsRemoved`, so the
 * collector has to hand it over separately for the checkpoint to be computable.
 */
export type TimestampedEntity = { entity: SyncEntity; maxTimestamp: number };

/** One `SyncUpdate`'s worth of entities and the checkpoint its Ack may advance to. */
export type SyncUpdateChunk = { entities: SyncEntity[]; timePoint: number };

const encodedSize = (entity: SyncEntity): number => SyncEntityCodec.enc(entity).length;

/** Same variant, a slice of its items — typed per tag so no assertion is needed. */
function sliceEntity(entity: SyncEntity, from: number, to: number): SyncEntity {
  switch (entity.tag) {
    case 'Devices':
      return { tag: 'Devices', value: entity.value.slice(from, to) };
    case 'ChatsAdded':
      return { tag: 'ChatsAdded', value: entity.value.slice(from, to) };
    case 'ChatsRemoved':
      return { tag: 'ChatsRemoved', value: entity.value.slice(from, to) };
    case 'Messages':
      return { tag: 'Messages', value: entity.value.slice(from, to) };
  }
}

/** Halve an over-budget entity until each piece fits, or is a single item. */
function splitToFit(entity: SyncEntity, maxBytes: number): SyncEntity[] {
  if (encodedSize(entity) <= maxBytes) return [entity];

  const count = entity.value.length;
  // A single item over budget cannot be split further — emit it alone and let
  // the send fail loudly rather than silently dropping the user's data.
  if (count <= 1) return [entity];

  const mid = Math.ceil(count / 2);

  return [...splitToFit(sliceEntity(entity, 0, mid), maxBytes), ...splitToFit(sliceEntity(entity, mid, count), maxBytes)];
}

/**
 * Splits a collected snapshot into `SyncUpdate`s that each fit the data channel,
 * and gives every one the checkpoint its own Ack may advance to.
 *
 * Spec: `timePoint` is a field of each `SyncUpdate`, and an acknowledgement lets
 * the sender advance `outgoingUpdateTime` "to the maximum timestamp found in the
 * SyncEntity". So each chunk carries the newest source timestamp it has FULLY
 * delivered — a piece of a split entity completes nothing, so it inherits the
 * previous chunk's checkpoint and its Ack is a no-op on the cursor. That is what
 * makes a large backlog drain monotonically instead of restarting from scratch
 * every time the channel drops mid-round.
 *
 * The last chunk carries `roundTimePoint`, the collection instant: once the whole
 * round is delivered the cursor must reach it, or entities deliberately filtered
 * out of the round would be re-collected forever.
 *
 * Order is preserved, so a peer applying chunks in order sees exactly the
 * sequence one oversized update would have given it. Returns `[]` when empty.
 */
function chunkSyncEntities(
  collected: TimestampedEntity[],
  roundTimePoint: number,
  maxBytes: number = DEVICE_SYNC_MAX_UPDATE_BYTES,
): SyncUpdateChunk[] {
  // `completesAt` is set only on the piece that finishes its source entity.
  const pieces: { entity: SyncEntity; completesAt: number | null }[] = [];
  for (const { entity, maxTimestamp } of collected) {
    const split = splitToFit(entity, maxBytes);
    for (let i = 0; i < split.length; i++) {
      pieces.push({ entity: split[i]!, completesAt: i === split.length - 1 ? maxTimestamp : null });
    }
  }

  const chunks: SyncUpdateChunk[] = [];
  let current: SyncEntity[] = [];
  let currentSize = 0;
  // Newest timestamp fully delivered so far. Starts at 0; `advanceOutgoingUpdateTime`
  // ignores a checkpoint that is not ahead of the stored one, so a chunk that
  // completes nothing simply makes no progress.
  let delivered = 0;

  function flush(): void {
    if (current.length === 0) return;
    chunks.push({ entities: current, timePoint: Math.min(delivered, roundTimePoint) });
    current = [];
    currentSize = 0;
  }

  for (const piece of pieces) {
    const size = encodedSize(piece.entity);
    if (current.length > 0 && currentSize + size > maxBytes) flush();
    current.push(piece.entity);
    currentSize += size;
    if (piece.completesAt !== null) delivered = Math.max(delivered, piece.completesAt);
  }
  flush();

  const last = chunks[chunks.length - 1];
  if (last) last.timePoint = roundTimePoint;

  return chunks;
}

function isDeviceSyncDisconnectStale(
  lastConnectionClosedAt: number | null,
  now: number,
  staleAfterMs: number = DEVICE_SYNC_STALE_AFTER_MS,
): boolean {
  if (lastConnectionClosedAt === null) return false;
  return now - lastConnectionClosedAt >= staleAfterMs;
}

/**
 * Maps the engine's connection phase + persisted disconnect time to the semantic
 * sync status. Never returns `null`: `'inactive'` is the "nothing to surface" status,
 * and the consuming feature decides whether to render it.
 */
function resolveDeviceSyncStatus(
  phase: DeviceSyncConnectionPhase,
  lastConnectionClosedAt: number | null,
  now: number,
  staleAfterMs: number = DEVICE_SYNC_STALE_AFTER_MS,
): DeviceSyncStatus {
  switch (phase) {
    case 'synced':
      return 'synced';
    case 'syncing':
      return 'syncing';
    case 'error':
      return 'error';
    case 'inactive':
    case 'connecting':
    case 'disconnected':
      break;
  }

  // After the grace window a long-dead connection is more informative as stale
  // ("open the mobile app") than as optimistic progress, so the stale check wins
  // over the connecting → syncing surface below.
  if (isDeviceSyncDisconnectStale(lastConnectionClosedAt, now, staleAfterMs)) {
    return 'stale';
  }

  // While (re)connecting, surface optimistic progress rather than nothing — the
  // handshake is in progress, not idle.
  if (phase === 'connecting') {
    return 'syncing';
  }

  return 'inactive';
}

export const deviceSyncService = { chunkSyncEntities, isDeviceSyncDisconnectStale, resolveDeviceSyncStatus };
