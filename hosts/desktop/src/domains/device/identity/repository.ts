import * as v from 'valibot';

import { type DeviceIdentityRow, database } from '@/shared/database';

import { deviceIdentityRowSchema } from './schemas';

// The store holds exactly one row: a device has one identity, and a second would
// silently split this install into two peers.
const ROW_ID = 'self';

async function read(): Promise<DeviceIdentityRow | null> {
  const row = await database.deviceIdentity.get(ROW_ID);
  if (!row) return null;

  // A malformed row reads as a miss so the caller mints a fresh identity. Throwing
  // instead would strand the install: nothing else ever rewrites this row.
  if (!v.is(deviceIdentityRowSchema, row)) {
    console.warn('[device-identity] dropped a malformed persisted identity');
    await database.deviceIdentity.delete(ROW_ID);

    return null;
  }

  return row;
}

/**
 * Persist a freshly minted identity, keeping whichever one landed first.
 *
 * `add` rather than `put`: two callers racing on first launch must converge on one
 * identity, and the loser has to adopt the winner's rather than overwrite it — a
 * device that changes its statement account mid-session is a new peer to everyone
 * who already knows it.
 */
async function create(row: DeviceIdentityRow): Promise<DeviceIdentityRow> {
  try {
    await database.deviceIdentity.add(row);

    return row;
  } catch {
    return (await read()) ?? row;
  }
}

async function clear(): Promise<void> {
  await database.deviceIdentity.delete(ROW_ID);
}

export const deviceIdentityRepository = {
  ROW_ID,
  read,
  create,
  clear,
};
