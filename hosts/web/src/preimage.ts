import type { PreimageHost } from "@parity/truapi-host";
import { RAW, cidToString } from "./archive/cid.js";
import { type FetchLike, readCapped } from "./archive/gateway.js";
import { subscription } from "./subscription.js";

/** Multihash code of blake2b-256, the hash a preimage key is, as its varint bytes. */
const BLAKE2B_256 = [0xa0, 0xe4, 0x02];
const KEY_BYTES = 32;
const LOOKUP_TIMEOUT_MS = 30_000;
/** The largest preimage this host reads. Product icons and catalogs are far smaller. */
const MAX_PREIMAGE_BYTES = 8 * 1024 * 1024;

/** The CID Bulletin serves a preimage under: CIDv1, raw, with the key as its blake2b-256 digest. */
export function preimageCid(key: Uint8Array): string {
  if (key.length !== KEY_BYTES)
    throw new Error(`a preimage key is ${KEY_BYTES} bytes, not ${key.length}`);
  return cidToString({
    bytes: Uint8Array.from([1, RAW, ...BLAKE2B_256, KEY_BYTES, ...key]),
    codec: RAW,
    hashCode: 0xb220,
    digest: key,
  });
}

/**
 * Preimage lookups read from the network's content gateway, as the native
 * hosts read them. A lookup fetches once and emits the bytes, or a miss when
 * the gateway does not have them or cannot be reached. The core checks the
 * bytes against the key, so the gateway is only a transport.
 */
export function gatewayPreimages(
  gateway: string,
  fetchFn: FetchLike = fetch,
): Required<PreimageHost> {
  return {
    lookupPreimage: (key) =>
      subscription<Uint8Array | undefined>((emit) => {
        const cancel = new AbortController();
        fetchPreimage(gateway, key, cancel.signal, fetchFn).then(emit, () =>
          emit(undefined),
        );
        return () => cancel.abort();
      }),
  };
}

async function fetchPreimage(
  gateway: string,
  key: Uint8Array,
  signal: AbortSignal,
  fetchFn: FetchLike,
): Promise<Uint8Array | undefined> {
  const url = new URL(`/ipfs/${preimageCid(key)}`, gateway);
  const response = await fetchFn(url.href, {
    signal: AbortSignal.any([signal, AbortSignal.timeout(LOOKUP_TIMEOUT_MS)]),
  });
  if (!response.ok) {
    await response.body?.cancel();
    return undefined;
  }
  return readCapped(response, MAX_PREIMAGE_BYTES);
}
