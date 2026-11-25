import { type ProductArchive } from '@/domains/product';

type Entry = { archive: ProductArchive; bytes: number; diskBacked: boolean; contenthash: string | null };

function archiveBytes(archive: ProductArchive): number {
  let total = 0;
  for (const content of Object.values(archive.files)) {
    // `files` is typed Uint8Array, but archive contents can arrive string-typed at
    // the IPC boundary on some paths (mirrors the `typeof content === 'string'`
    // guard in `index.ts`'s archive:persistToDisk handler); `.byteLength` on a
    // string would be undefined → NaN, so size strings explicitly.
    total += typeof content === 'string' ? Buffer.byteLength(content) : content.byteLength;
  }
  return total;
}

// In-memory archive cache with a byte ceiling, backing the polkadot:// protocol
// handler. Only disk-backed entries (persisted, or read back from disk) are
// re-servable, so they are the only ones evicted under pressure. Renderer-warmed
// entries (unpinned products, possibly serving a live webview) are never evicted —
// dropping them would 404 a running product's lazy-loaded assets. Insertion order
// is the LRU order; `get` re-inserts to mark recency.
export function createArchiveMemoryCache(maxBytes: number) {
  const map = new Map<string, Entry>();
  let totalBytes = 0;

  // Evict oldest-first until under the byte cap, but ONLY disk-backed entries —
  // they are re-servable from disk, so dropping them from memory is safe. A just-
  // inserted disk-backed entry larger than the whole cap self-evicts here; that is
  // intended (the protocol handler falls back to disk). Non-disk-backed entries are
  // never dropped, so the cap can be legitimately exceeded when they dominate.
  // Deleting an already-visited key mid-iteration is safe per the Map spec.
  function evictDiskBackedUntilUnderCap(): void {
    if (totalBytes <= maxBytes) return;
    for (const [domain, entry] of map) {
      if (totalBytes <= maxBytes) break;
      if (!entry.diskBacked) continue;
      map.delete(domain);
      totalBytes -= entry.bytes;
    }
  }

  function get(domain: string): ProductArchive | undefined {
    const entry = map.get(domain);
    if (!entry) return undefined;
    map.delete(domain);
    map.set(domain, entry);
    return entry.archive;
  }

  function set(domain: string, archive: ProductArchive, diskBacked: boolean, contenthash: string | null = null): void {
    const existing = map.get(domain);
    if (existing) totalBytes -= existing.bytes;
    map.delete(domain);
    const bytes = archiveBytes(archive);
    map.set(domain, { archive, bytes, diskBacked, contenthash });
    totalBytes += bytes;
    evictDiskBackedUntilUnderCap();
  }

  // Non-mutating read of an entry's contenthash tag — does NOT bump LRU recency
  // (unlike `get`, which re-inserts). The polkadot:// handler reads the tag to decide
  // whether a cached entry is the pinned/frozen version before serving it. Returns
  // `undefined` for a miss and `{ contenthash: null }` for a hash-blind renderer warm.
  function peek(domain: string): { contenthash: string | null } | undefined {
    const entry = map.get(domain);
    if (!entry) return undefined;
    return { contenthash: entry.contenthash };
  }

  function remove(domain: string): void {
    const existing = map.get(domain);
    if (!existing) return;
    totalBytes -= existing.bytes;
    map.delete(domain);
  }

  function clear(): void {
    map.clear();
    totalBytes = 0;
  }

  function stats(): { size: number; totalBytes: number } {
    return { size: map.size, totalBytes };
  }

  return { get, set, peek, delete: remove, clear, stats };
}
