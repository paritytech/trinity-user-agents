import { database } from '@/shared/database';

// Warm-start blobs for the embedded light client, one per chain. The blob is smoldot's
// own finalized-state database; the host never interprets it, only hands it back.

/**
 * The stored blob for a chain, or `null` when none is held.
 *
 * **Deliberately does not catch.** The provider's contract is explicit: a store that
 * cannot answer must reject rather than resolve empty, because an empty read is taken
 * as "nothing stored yet" and would let the next snapshot overwrite good state. Every
 * other read in this domain fails soft; this one must not, and a later pass to make it
 * "consistent" would reintroduce exactly the bug the contract warns about.
 */
async function load(genesisHash: string): Promise<string | null> {
  const row = await database.lightClientDatabases.get(genesisHash);

  return row?.blob ?? null;
}

async function save(genesisHash: string, blob: string): Promise<void> {
  await database.lightClientDatabases.put({ genesisHash, blob, updatedAt: Date.now() });
}

export const lightClientRepository = {
  load,
  save,
};
