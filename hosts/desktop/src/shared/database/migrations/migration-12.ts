import { type Transaction } from 'dexie';

// The row shape of the legacy product-storage store this migration reads. The store
// is deleted by the same version, so the shape lives here and nowhere else.
type LegacyProductLocalStorageRow = {
  productId: string;
  data: Record<string, Uint8Array>;
};

// Frozen historical value: the scope the host stored every product's entries under
// before v12. It is not TrUAPI vocabulary — it is data written by old releases, so it
// must never change, and `@/shared` still names no protocol term.
const LEGACY_HOST_SCOPE = 'truapi';

/**
 * v12 upgrade: product storage moves from one row holding every entry to one row
 * per key. Rows under any other `productId` were never read by the core, so they are
 * dropped with the old store. `bulkPut` keeps a re-run safe.
 */
export async function migrateProductStorageToV12(tx: Transaction): Promise<void> {
  const legacy = await tx.table<LegacyProductLocalStorageRow, string>('productLocalStorage').get(LEGACY_HOST_SCOPE);
  if (!legacy) return;

  await tx
    .table<{ key: string; value: Uint8Array }, string>('productStorage')
    .bulkPut(Object.entries(legacy.data).map(([key, value]) => ({ key, value })));
}
