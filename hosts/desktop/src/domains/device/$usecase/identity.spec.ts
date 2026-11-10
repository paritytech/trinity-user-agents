import 'fake-indexeddb/auto';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type DeviceIdentityRow, database } from '@/shared/database';

import { deviceIdentityUseCase } from './identity';

// The repository runs for real on the fake IndexedDB imported above: what these cases
// pin is the one-identity invariant, which lives in the row the table ends up holding.
const table = database.deviceIdentity;

const storedRow = (seed: Uint8Array, encryptionPrivateKey = new Uint8Array(32).fill(9)) => ({
  id: 'self',
  statementAccountSeed: seed,
  encryptionPrivateKey,
  createdAt: 1,
});

describe('deviceIdentityUseCase.getDeviceIdentity', () => {
  beforeEach(async () => {
    // The module memoizes the identity for the process, which is the behaviour under
    // test; the reset drops that memo and the row so each case starts from an empty store.
    await deviceIdentityUseCase.resetDeviceIdentity();
    vi.restoreAllMocks();
  });

  it('mints and persists an identity on first use', async () => {
    const identity = await deviceIdentityUseCase.getDeviceIdentity();

    expect(identity.statementAccountSeed).toHaveLength(64);
    expect(identity.statementAccountPublicKey).toHaveLength(32);
    expect(identity.encryptionPrivateKey).toHaveLength(32);
    expect(identity.encryptionPublicKey).toHaveLength(32);
    expect(await table.count()).toBe(1);
  });

  it('mints exactly once even when callers race', async () => {
    const add = vi.spyOn(table, 'add');

    const [a, b] = await Promise.all([deviceIdentityUseCase.getDeviceIdentity(), deviceIdentityUseCase.getDeviceIdentity()]);

    expect(add).toHaveBeenCalledTimes(1);
    expect(a.statementAccountSeed).toEqual(b.statementAccountSeed);
  });

  it('adopts the stored identity rather than minting a second', async () => {
    const seed = new Uint8Array(64).fill(7);
    await table.put(storedRow(seed));
    const add = vi.spyOn(table, 'add');

    const identity = await deviceIdentityUseCase.getDeviceIdentity();

    expect(identity.statementAccountSeed).toEqual(seed);
    expect(add).not.toHaveBeenCalled();
  });

  // A short seed would reach `createSr25519Prover` and sign statements no peer can
  // verify, so a malformed row must read as a miss.
  it('replaces a malformed persisted row', async () => {
    await table.put(storedRow(new Uint8Array(8), new Uint8Array(32)));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const identity = await deviceIdentityUseCase.getDeviceIdentity();

    expect(warn).toHaveBeenCalled();
    expect(identity.statementAccountSeed).toHaveLength(64);
    expect((await table.get('self'))?.statementAccountSeed).toHaveLength(64);
    expect(await table.count()).toBe(1);
  });

  it('keeps the winner when a concurrent process already wrote one', async () => {
    const seed = new Uint8Array(64).fill(3);
    // Another process lands its row between our read and our write, so the `add`
    // hits the primary key it just took.
    const realAdd = table.add.bind(table);
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- Dexie's `add` promise type is wider than the async fn's
    vi.spyOn(table, 'add').mockImplementationOnce((async (row: DeviceIdentityRow) => {
      await realAdd(storedRow(seed, new Uint8Array(32).fill(4)));

      return realAdd(row);
    }) as never);

    const identity = await deviceIdentityUseCase.getDeviceIdentity();

    expect(identity.statementAccountSeed).toEqual(seed);
  });
});

it('exposes the use case from the module', () => {
  expect(deviceIdentityUseCase.getDeviceIdentity).toBeTypeOf('function');
});
