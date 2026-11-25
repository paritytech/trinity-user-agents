/**
 * SCALE codecs for the device-sync wire format. Byte-for-byte parity with
 * Android (`SyncScale.kt`, `LocalMessageScale.kt`). Mismatched encoding here
 * silently corrupts sync. Variant ordinals are pinned below.
 *   SyncMessage  { Update=0, Ack=1 }
 *   SyncEntity   { Devices=0, ChatsAdded=1, ChatsRemoved=2, Messages=3 }
 *   ChatId       { Contact=0 }
 *   DeviceStatus { ACTIVE=0 }
 *   LocalStatus  { Outgoing(OutgoingStatus)=0, Incoming(IncomingStatus)=1 }
 *   OutgoingStatus { NEW=0, SENT=1, DELIVERED=2 }
 *   IncomingStatus { NEW=0, SEEN=1 }
 */

import { ChatMessage as ChatMessageStatementCodec } from '@novasamatech/host-chat/codec/message';
import { AccountId } from '@polkadot-api/substrate-bindings';
import { Bytes, Enum, Struct, Vector, _void, u32, u64 } from 'scale-ts';

// The chat-wire `ChatMessage` codec comes straight from the SDK (it carries
// the 3-variant iOSVoIP Platform), so device-sync envelopes decode identically to the
// live channels without the chat domain in between.

const AccountIdCodec = Bytes(32);
const EncrPublicKeyCodec = Bytes(32); // X25519 public key (CHAT-RFC-0004)

const ss58Codec = AccountId();

// SCALE-encode an SS58 address string to its 32-byte account id.
export function encodeAccountIdSs58(ss58: string): Uint8Array {
  return ss58Codec.enc(ss58);
}

export { ChatMessageStatementCodec };

export const ChatIdCodec = Enum({
  Contact: AccountIdCodec, // 0
});

const DeviceStatusCodec = Enum({
  ACTIVE: _void, // 0
});

const OutgoingStatusCodec = Enum({
  NEW: _void, // 0
  SENT: _void, // 1
  DELIVERED: _void, // 2
});

const IncomingStatusCodec = Enum({
  NEW: _void, // 0
  SEEN: _void, // 1
});

export const LocalStatusCodec = Enum({
  Outgoing: OutgoingStatusCodec, // 0
  Incoming: IncomingStatusCodec, // 1
});

const LocalDeviceCodec = Struct({
  statementAccountId: AccountIdCodec,
  encryptionPublicKey: EncrPublicKeyCodec,
  status: DeviceStatusCodec,
  lastUpdate: u64,
});

export const LocalMessageCodec = Struct({
  remote: ChatMessageStatementCodec,
  peerId: AccountIdCodec,
  status: LocalStatusCodec,
  order: u64,
});

export const SyncEntityCodec = Enum({
  Devices: Vector(LocalDeviceCodec), // 0
  ChatsAdded: Vector(ChatIdCodec), // 1
  ChatsRemoved: Vector(ChatIdCodec), // 2
  Messages: Vector(LocalMessageCodec), // 3
});

const SyncUpdateCodec = Struct({
  id: u32,
  entities: Vector(SyncEntityCodec),
  timePoint: u64,
});

const SyncUpdateAckCodec = Struct({
  id: u32,
});

export const SyncMessageCodec = Enum({
  Update: SyncUpdateCodec, // 0
  Ack: SyncUpdateAckCodec, // 1
});
