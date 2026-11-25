import { blake2b } from '@noble/hashes/blake2.js';
import { toHex } from 'polkadot-api/utils';
import { type CodecType, Bytes, Struct } from 'scale-ts';

import { type DeviceRosterEvent } from './schemas';
import { type Contact, type Device } from './types';

const hasKnownDevices = (contact: Contact): boolean => contact.devices.length > 0;

const addDevice = (contact: Contact, device: Device): Contact => {
  const without = contact.devices.filter(d => d.statementAccountId !== device.statementAccountId);
  return {
    ...contact,
    devices: [...without, device],
  };
};

const removeDevice = (contact: Contact, statementAccountId: string): Contact => ({
  ...contact,
  devices: contact.devices.filter(d => d.statementAccountId !== statementAccountId),
});

// Translates a decoded DeviceRosterEvent (bytes) into the equivalent
// hex-string mutation on a Contact. The codec deals in Uint8Array because
// SCALE bytes are bytes; the persisted Contact deals in hex strings to
// match the rest of the chat domain.
const applyRosterEvent = (contact: Contact, event: CodecType<typeof DeviceRosterEvent>): Contact => {
  switch (event.tag) {
    case 'DeviceAdded':
      return addDevice(contact, {
        statementAccountId: toHex(event.value.statementAccountId),
        encryptionPublicKey: toHex(event.value.encryptionPublicKey),
      });
    case 'DeviceRemoved':
      return removeDevice(contact, toHex(event.value.statementAccountId));
  }
};

// ── Roster topic derivation ─────────────────────────────────────────────────
// Each user broadcasts their own DeviceAdded / DeviceRemoved events on a topic
// keyed on their identity sr25519 accountId; contacts subscribe (matchAny) to
// the roster topics of every known contact. Wire format mirrors V1 chat-request
// topics — context-prefixed SCALE-encoded blake2b256 — with a distinct context
// string so roster events don't collide with chat-request topics:
//   blake2b-256( SCALE({ context: "device-roster", accountId: userAccountId }) )
// NOTE: the exact context string / SCALE encoding aren't pinned by the HackMD
// spec — confirm with iOS/Android before shipping.
const ROSTER_TOPIC_CONTEXT = new TextEncoder().encode('device-roster');
const RosterTopicInput = Struct({ context: Bytes(), accountId: Bytes() });

const computeRosterTopic = (userAccountId: Uint8Array): Uint8Array => {
  const encoded = RosterTopicInput.enc({ context: ROSTER_TOPIC_CONTEXT, accountId: userAccountId });
  return blake2b(encoded, { dkLen: 32 });
};

/** The matchAny topic set a recipient subscribes to to track all known contacts' roster events. */
const computeRosterSubscriptionTopics = (contactAccountIds: Uint8Array[]): Uint8Array[] =>
  contactAccountIds.map(computeRosterTopic);

export const contactService = {
  hasKnownDevices,
  addDevice,
  removeDevice,
  applyRosterEvent,
  computeRosterTopic,
  computeRosterSubscriptionTopics,
};
