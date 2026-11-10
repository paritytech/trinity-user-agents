import { type CodecType } from 'scale-ts';
import { describe, expect, it } from 'vitest';

import { DEVICE_SYNC_STALE_AFTER_MS } from './constants';
import { SyncEntityCodec } from './schemas';
import { deviceSyncService } from './service';

describe('device-sync service', () => {
  const now = 1_000_000_000_000;

  describe('isDeviceSyncDisconnectStale', () => {
    it('returns false when never disconnected', () => {
      expect(deviceSyncService.isDeviceSyncDisconnectStale(null, now)).toBe(false);
    });

    it('returns false within the stale window', () => {
      expect(deviceSyncService.isDeviceSyncDisconnectStale(now - DEVICE_SYNC_STALE_AFTER_MS + 1, now)).toBe(false);
    });

    it('returns true after the stale window', () => {
      expect(deviceSyncService.isDeviceSyncDisconnectStale(now - DEVICE_SYNC_STALE_AFTER_MS, now)).toBe(true);
    });
  });

  describe('resolveDeviceSyncStatus', () => {
    it('reports synced when synced', () => {
      expect(deviceSyncService.resolveDeviceSyncStatus('synced', null, now)).toBe('synced');
    });

    it('reports syncing while the WebRTC handshake is still in progress', () => {
      expect(deviceSyncService.resolveDeviceSyncStatus('connecting', null, now)).toBe('syncing');
    });

    it('reports syncing while data is actively syncing', () => {
      expect(deviceSyncService.resolveDeviceSyncStatus('syncing', null, now)).toBe('syncing');
    });

    it('reports error on error phase', () => {
      expect(deviceSyncService.resolveDeviceSyncStatus('error', null, now)).toBe('error');
    });

    it('reports inactive shortly after disconnect within the grace window', () => {
      const closedAt = now - 30 * 60 * 1000;
      expect(deviceSyncService.resolveDeviceSyncStatus('disconnected', closedAt, now)).toBe('inactive');
    });

    it('reports inactive when idle and never disconnected', () => {
      expect(deviceSyncService.resolveDeviceSyncStatus('inactive', null, now)).toBe('inactive');
    });

    it('reports stale after the grace window even while reconnecting', () => {
      const closedAt = now - DEVICE_SYNC_STALE_AFTER_MS;
      expect(deviceSyncService.resolveDeviceSyncStatus('connecting', closedAt, now)).toBe('stale');
    });

    it('reports synced when live even if a stale disconnect timestamp is still persisted', () => {
      const closedAt = now - DEVICE_SYNC_STALE_AFTER_MS;
      expect(deviceSyncService.resolveDeviceSyncStatus('synced', closedAt, now)).toBe('synced');
    });
  });
});

describe('chunkSyncEntities', () => {
  const ROUND = 10_000;
  const contact = (fill: number) => ({ tag: 'Contact' as const, value: new Uint8Array(32).fill(fill) });
  const chatsAdded = (count: number, maxTimestamp: number) => ({
    entity: { tag: 'ChatsAdded' as const, value: Array.from({ length: count }, (_, i) => contact(i % 256)) },
    maxTimestamp,
  });
  const size = (entities: CodecType<typeof SyncEntityCodec>[]) =>
    entities.reduce((total, e) => total + SyncEntityCodec.enc(e).length, 0);

  it('returns an empty list for an empty snapshot', () => {
    expect(deviceSyncService.chunkSyncEntities([], ROUND)).toEqual([]);
  });

  it('keeps a snapshot that already fits as a single update carrying the round instant', () => {
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(3, 500)], ROUND);

    expect(chunks).toHaveLength(1);
    expect(chunks[0]!.entities).toHaveLength(1);
    expect(chunks[0]!.timePoint).toBe(ROUND);
  });

  it('splits an entity whose own vector exceeds the budget, and every chunk fits', () => {
    // 2000 contacts ≈ 66KB encoded — over the real 60KB budget on its own.
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(2000, 500)], ROUND);

    expect(chunks.length).toBeGreaterThan(1);
    for (const chunk of chunks) {
      expect(size(chunk.entities)).toBeLessThanOrEqual(60 * 1024);
    }
  });

  it('preserves item order and count across the split', () => {
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(2000, 500)], ROUND);
    const flattened = chunks.flatMap(c => c.entities).flatMap(e => (e.tag === 'ChatsAdded' ? e.value : []));

    expect(flattened).toHaveLength(2000);
    expect(flattened[0]!.value[0]).toBe(0);
    expect(flattened[1999]!.value[0]).toBe(1999 % 256);
  });

  it('packs several small entities into as few updates as possible', () => {
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(2, 1), chatsAdded(2, 2), chatsAdded(2, 3)], ROUND);

    expect(chunks).toHaveLength(1);
    expect(chunks[0]!.entities).toHaveLength(3);
  });

  // Spec: each SyncUpdate carries the checkpoint its own Ack may advance to —
  // "the maximum timestamp found in the SyncEntity".
  it('gives each chunk the newest timestamp it has FULLY delivered', () => {
    // Two entities, each ~33KB, so they land in separate chunks.
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(1000, 111), chatsAdded(1000, 222)], ROUND);

    expect(chunks).toHaveLength(2);
    expect(chunks[0]!.timePoint).toBe(111);
    // Last chunk always reaches the round instant, so a fully delivered round
    // never re-collects entities that were filtered out of it.
    expect(chunks[1]!.timePoint).toBe(ROUND);
  });

  // A piece of a split entity completes nothing, so its Ack must not advance
  // past data still queued behind it.
  it('gives a partial piece of a split entity no checkpoint of its own', () => {
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(2000, 999)], ROUND);

    expect(chunks.length).toBeGreaterThan(1);
    // Every chunk before the last completes nothing → checkpoint stays at 0,
    // which `advanceOutgoingUpdateTime` treats as no progress.
    for (const chunk of chunks.slice(0, -1)) {
      expect(chunk.timePoint).toBe(0);
    }
    expect(chunks[chunks.length - 1]!.timePoint).toBe(ROUND);
  });

  it('never lets a chunk checkpoint exceed the round instant', () => {
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(1000, ROUND + 5_000), chatsAdded(1000, ROUND)], ROUND);

    for (const chunk of chunks) {
      expect(chunk.timePoint).toBeLessThanOrEqual(ROUND);
    }
  });

  // A single item over budget cannot be split further; it must still be emitted
  // rather than silently dropped.
  it('emits an unsplittable single item alone', () => {
    const chunks = deviceSyncService.chunkSyncEntities([chatsAdded(1, 5)], ROUND, 4);

    expect(chunks).toHaveLength(1);
    expect(chunks[0]!.entities[0]!.tag).toBe('ChatsAdded');
  });
});
