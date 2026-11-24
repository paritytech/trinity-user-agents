import { bytesToHex, type HexString } from "./scale.js";

const RING_COLLECTION_ID_BYTES = 32;
const SPACE = 0x20;

/**
 * Encode a ring collection name as its `CollectionId` junction value.
 *
 * A collection identifier is the UTF-8 name padded with spaces to 32 bytes, so
 * `"pop:polkadot.network/people"` is stored as
 * `"pop:polkadot.network/people     "`. Throws a `RangeError` for a name longer
 * than 32 bytes.
 */
export function ringCollectionId(name: string): HexString {
  const encoded = new TextEncoder().encode(name);
  if (encoded.length > RING_COLLECTION_ID_BYTES) {
    throw new RangeError(
      `Ring collection name ${JSON.stringify(name)} is ${encoded.length} bytes, longer than ${RING_COLLECTION_ID_BYTES}`,
    );
  }
  const identifier = new Uint8Array(RING_COLLECTION_ID_BYTES).fill(SPACE);
  identifier.set(encoded);
  return bytesToHex(identifier);
}
