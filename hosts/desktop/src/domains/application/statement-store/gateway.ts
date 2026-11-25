import { AccountId } from '@polkadot-api/substrate-bindings';
import * as v from 'valibot';

import { statementStoreSlotValueSchema } from './schemas';

// Minimal structural view of the papi unsafe api — the use case hands in the real
// one, specs hand in fakes. The storage item is optional because this pallet is not
// guaranteed on every People-chain runtime.
type StorageEntry = {
  getEntries?: (...args: unknown[]) => Promise<{ keyArgs: unknown[]; value: unknown }[]>;
};
type UnsafeQueryApi = { query: Record<string, Record<string, StorageEntry> | undefined> };

const accountIdCodec = AccountId();

/**
 * Raw 32-byte account ids holding a statement-store slot in `period`.
 *
 * Returns `null` when the pallet or storage item is absent — "we cannot tell".
 * Callers MUST NOT read that as "no slot": a renamed or missing storage item
 * would otherwise look like a universal allowance lapse and raise a prompt that
 * can never resolve. An empty array is the real "this period has no slots".
 */
async function getStatementStoreSlots(api: UnsafeQueryApi, period: number): Promise<Nullable<Uint8Array[]>> {
  const storage = api.query['Resources']?.['StatementStoreAllowances'];
  if (!storage?.getEntries) return null;

  // Called as a method: papi storage entries are bound objects.
  const entries = await storage.getEntries(period);

  const slots: Uint8Array[] = [];
  for (const entry of entries) {
    const parsed = v.safeParse(statementStoreSlotValueSchema, entry.value);
    if (!parsed.success) continue;
    try {
      // `safeParse` only proves this is a string — a malformed SS58 still throws
      // here. Skip the row rather than let one bad entry discard the period.
      slots.push(accountIdCodec.enc(parsed.output.account_id));
    } catch {
      continue;
    }
  }

  return slots;
}

export const localAllowanceGateway = { getStatementStoreSlots };
