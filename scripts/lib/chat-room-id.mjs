// The id of the chat room that a product creates with a room id source. It is
// `ProductId::chat_room_id` of the truapi crate: the blake2b-256 hash of the
// SCALE encoding of `("product", product id, source)`, in `0x` hex. Both values
// must be in the normalized form that the core uses.
//
// Only the simulator launchers load this module. They run after the workspace
// install, which supplies `@noble/hashes`.

import { blake2b } from "@noble/hashes/blake2.js";

// Product ids and sources are at most 256 bytes, so a length always fits the
// one-byte or the two-byte compact mode.
function compactLength(length) {
  if (length < 1 << 6) return [length << 2];
  if (length < 1 << 14) {
    const value = (length << 2) | 0b01;
    return [value & 0xff, value >> 8];
  }
  throw new Error(`a chat identifier of ${length} bytes is too long`);
}

function scaleString(text) {
  const bytes = new TextEncoder().encode(text);
  return [...compactLength(bytes.length), ...bytes];
}

export function chatRoomId(productId, roomIdSource) {
  const preimage = Uint8Array.from([
    ...scaleString("product"),
    ...scaleString(productId),
    ...scaleString(roomIdSource),
  ]);
  return `0x${Buffer.from(blake2b(preimage, { dkLen: 32 })).toString("hex")}`;
}
