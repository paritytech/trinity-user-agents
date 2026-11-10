import { AccountId } from '@polkadot-api/substrate-bindings';
import { describe, expect, it } from 'vitest';

import { localAllowanceGateway } from './gateway';

const PUBLIC_KEY = new Uint8Array(32).fill(1);
const SS58 = AccountId().dec(PUBLIC_KEY);

function fakeApi(overrides: object = {}) {
  return {
    query: {
      Resources: {
        StatementStoreAllowances: {
          getEntries: async () => [{ keyArgs: [123, '0xaa'], value: { account_id: SS58, seq: 0, since: 1n } }],
        },
      },
      ...overrides,
    },
  };
}

describe('localAllowanceGateway.getStatementStoreSlots', () => {
  it('returns the raw account bytes of the period slots', async () => {
    const slots = await localAllowanceGateway.getStatementStoreSlots(fakeApi(), 123);

    expect(slots).toEqual([PUBLIC_KEY]);
  });

  it('passes the period through as the partial key', async () => {
    let seen: unknown = null;
    const api = fakeApi({
      Resources: {
        StatementStoreAllowances: {
          getEntries: async (period: unknown) => {
            seen = period;

            return [];
          },
        },
      },
    });

    await localAllowanceGateway.getStatementStoreSlots(api, 456);

    expect(seen).toBe(456);
  });

  it('returns null — not [] — when the pallet is absent', async () => {
    expect(await localAllowanceGateway.getStatementStoreSlots({ query: {} }, 123)).toBeNull();
  });

  it('returns null — not [] — when the storage item is absent', async () => {
    expect(await localAllowanceGateway.getStatementStoreSlots(fakeApi({ Resources: {} }), 123)).toBeNull();
  });

  it('returns [] when the period genuinely has no slots', async () => {
    const api = fakeApi({ Resources: { StatementStoreAllowances: { getEntries: async () => [] } } });

    expect(await localAllowanceGateway.getStatementStoreSlots(api, 123)).toEqual([]);
  });

  it('skips a well-shaped entry whose address does not decode, keeping the rest', async () => {
    const api = fakeApi({
      Resources: {
        StatementStoreAllowances: {
          getEntries: async () => [
            { keyArgs: [123], value: { account_id: 'not-an-ss58-address' } },
            { keyArgs: [123], value: { account_id: SS58 } },
          ],
        },
      },
    });

    expect(await localAllowanceGateway.getStatementStoreSlots(api, 123)).toEqual([PUBLIC_KEY]);
  });

  it('skips entries whose value shape mismatches', async () => {
    const api = fakeApi({
      Resources: { StatementStoreAllowances: { getEntries: async () => [{ keyArgs: [123], value: { bogus: true } }] } },
    });

    expect(await localAllowanceGateway.getStatementStoreSlots(api, 123)).toEqual([]);
  });
});
