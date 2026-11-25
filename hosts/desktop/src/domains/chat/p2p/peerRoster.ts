/**
 * The peer's device list, as a live handle the session reads through.
 *
 * `@novasamatech/statement-store` sessions take a `PeerRoster` rather than a snapshot: the
 * outgoing envelope is built against `current()` on every submit, and the incoming topic
 * set is re-derived whenever `subscribe` fires. That is what lets a `deviceAdded` /
 * `deviceRemoved` be applied to a live session instead of tearing it down and building a
 * new one, which is what this container did before.
 */

import { type DeviceTarget, type PeerRoster } from '@novasamatech/statement-store';

export type PeerRosterHandle = PeerRoster & {
  /** Publish a new device list to the session. Ignored when nothing actually changed. */
  set: (devices: DeviceTarget[]) => void;
};

const sameRoster = (a: DeviceTarget[], b: DeviceTarget[]): boolean => {
  if (a.length !== b.length) return false;

  return a.every((device, index) => {
    const other = b[index];
    if (!other) return false;

    return (
      device.statementAccountId.length === other.statementAccountId.length &&
      device.statementAccountId.every((byte, i) => byte === other.statementAccountId[i]) &&
      device.encryptionPublicKey.length === other.encryptionPublicKey.length &&
      device.encryptionPublicKey.every((byte, i) => byte === other.encryptionPublicKey[i])
    );
  });
};

export const createPeerRoster = (initial: DeviceTarget[]): PeerRosterHandle => {
  let devices = initial;
  const listeners = new Set<(devices: DeviceTarget[]) => void>();

  return {
    current: () => devices,
    subscribe(callback) {
      listeners.add(callback);

      return () => listeners.delete(callback);
    },
    set(next) {
      // A no-op update would re-open the store subscription for nothing; roster writes
      // arrive on every applied roster event, most of which change nothing.
      if (sameRoster(devices, next)) return;
      devices = next;
      for (const listener of listeners) listener(next);
    },
  };
};
