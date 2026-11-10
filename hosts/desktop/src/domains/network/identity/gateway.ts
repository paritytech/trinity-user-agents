import { AccountId } from '@polkadot-api/substrate-bindings';

import { consumerIdentityService } from './service';
import { type ConsumerIdentity } from './types';

// Minimal structural view of the papi unsafe api — the resource hands in the real
// one, specs hand in fakes. The storage item is optional because `Resources` is not
// guaranteed on every People-chain runtime.
type StorageEntry = {
  getValues?: (keys: unknown[][]) => Promise<unknown[]>;
};
type UnsafeQueryApi = { query: Record<string, Record<string, StorageEntry> | undefined> };

const accountIdCodec = AccountId();

/**
 * Read `Resources.Consumers` for a batch of SS58 addresses.
 *
 * Returns `null` when the pallet or storage item is absent — "we cannot tell".
 * Callers MUST NOT read that as "these accounts have no identity": a renamed
 * storage item would otherwise look like every peer losing their username at once.
 * A row that is genuinely empty comes back as a `null` value inside the map.
 */
async function readConsumers(
  api: UnsafeQueryApi,
  accountIds: string[],
): Promise<Nullable<Record<string, ConsumerIdentity | null>>> {
  const storage = api.query['Resources']?.['Consumers'];
  if (!storage?.getValues) return null;
  if (accountIds.length === 0) return {};

  // One malformed address must not discard the batch: it is dropped, the rest read.
  // The codec runs purely as that filter — the address itself is what gets passed on.
  const queryable: string[] = [];
  for (const accountId of accountIds) {
    try {
      accountIdCodec.enc(accountId);
    } catch {
      continue;
    }
    queryable.push(accountId);
  }

  // The SS58 address, NOT its encoded bytes: papi builds the storage key from the
  // *decoded* argument and checks it against the runtime's key type first, so a
  // pre-encoded `Uint8Array` fails that check and the whole read rejects with
  // "Incompatible runtime entry Storage(Resources.Consumers)".
  //
  // Called as a method: papi storage entries are bound objects.
  const values = await storage.getValues(queryable.map(accountId => [accountId]));
  return Object.fromEntries(
    queryable.map((accountId, index) => [accountId, consumerIdentityService.toConsumerIdentity(accountId, values[index])]),
  );
}

export const consumerIdentityGateway = {
  readConsumers,
};
