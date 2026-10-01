import { type Cid, cidKey, readCid, readVarint, verifyBlock } from "./cid.js";

/** What a CARv1 archive may contain before it is refused. */
export interface CarLimits {
  maxBytes: number;
  maxBlocks: number;
  maxBlockBytes: number;
}

/** The blocks of an archive, each checked against its own identifier. */
export interface Car {
  roots: Cid[];
  /** Block bytes by `cidKey`. */
  blocks: Map<string, Uint8Array>;
}

const MAX_HEADER_BYTES = 4096;
const CBOR_MAX_DEPTH = 4;

type Cbor =
  | number
  | Uint8Array
  | string
  | Cbor[]
  | Map<string, Cbor>
  | { tag: number; value: Cbor };

function readCbor(
  bytes: Uint8Array,
  offset: number,
  depth: number,
): { value: Cbor; next: number } {
  if (depth > CBOR_MAX_DEPTH)
    throw new Error("CAR header is nested too deeply");
  const initial = bytes[offset];
  if (initial === undefined) throw new Error("truncated CAR header");
  const major = initial >> 5;
  const info = initial & 31;
  let at = offset + 1;
  let length: number;
  if (info < 24) length = info;
  else if (info < 28) {
    const width = 1 << (info - 24);
    if (width > 4) throw new Error("CAR header number is too large");
    length = 0;
    for (let index = 0; index < width; index += 1) {
      const byte = bytes[at + index];
      if (byte === undefined) throw new Error("truncated CAR header");
      length = length * 256 + byte;
    }
    at += width;
  } else throw new Error("unsupported CAR header encoding");
  switch (major) {
    case 0:
      return { value: length, next: at };
    case 2:
    case 3: {
      const end = at + length;
      if (end > bytes.length) throw new Error("truncated CAR header");
      const slice = bytes.slice(at, end);
      return {
        value: major === 2 ? slice : new TextDecoder().decode(slice),
        next: end,
      };
    }
    case 4: {
      const items: Cbor[] = [];
      for (let index = 0; index < length; index += 1) {
        const item = readCbor(bytes, at, depth + 1);
        items.push(item.value);
        at = item.next;
      }
      return { value: items, next: at };
    }
    case 5: {
      const entries = new Map<string, Cbor>();
      for (let index = 0; index < length; index += 1) {
        const key = readCbor(bytes, at, depth + 1);
        if (typeof key.value !== "string")
          throw new Error("CAR header key is not text");
        const item = readCbor(bytes, key.next, depth + 1);
        entries.set(key.value, item.value);
        at = item.next;
      }
      return { value: entries, next: at };
    }
    case 6: {
      const item = readCbor(bytes, at, depth + 1);
      return { value: { tag: length, value: item.value }, next: item.next };
    }
    default:
      throw new Error("unsupported CAR header encoding");
  }
}

function readHeader(bytes: Uint8Array): { roots: Cid[]; next: number } {
  const size = readVarint(bytes, 0);
  if (size.value === 0 || size.value > MAX_HEADER_BYTES)
    throw new Error("CAR header has an unreasonable size");
  const end = size.next + size.value;
  if (end > bytes.length) throw new Error("truncated CAR header");
  const header = readCbor(bytes.subarray(size.next, end), 0, 0).value;
  if (!(header instanceof Map) || header.get("version") !== 1)
    throw new Error("not a CARv1 archive");
  const listed = header.get("roots");
  if (!Array.isArray(listed)) throw new Error("CAR header has no roots");
  const roots = listed.map((root) => {
    const tagged = root as { tag?: number; value?: unknown };
    if (
      tagged.tag !== 42 ||
      !(tagged.value instanceof Uint8Array) ||
      tagged.value[0] !== 0
    )
      throw new Error("CAR root is not a CID link");
    return readCid(tagged.value, 1).cid;
  });
  return { roots, next: end };
}

/**
 * Read a CARv1 archive, verifying every block against the identifier it
 * carries. A block that does not match, or an archive over `limits`, is an
 * error, never a partial result.
 */
export function parseCar(bytes: Uint8Array, limits: CarLimits): Car {
  if (bytes.length > limits.maxBytes)
    throw new Error("the archive is larger than this host accepts");
  const header = readHeader(bytes);
  const blocks = new Map<string, Uint8Array>();
  let at = header.next;
  let count = 0;
  while (at < bytes.length) {
    const section = readVarint(bytes, at);
    const end = section.next + section.value;
    if (section.value === 0 || end > bytes.length)
      throw new Error("truncated CAR section");
    count += 1;
    if (count > limits.maxBlocks)
      throw new Error("the archive holds more blocks than this host accepts");
    const { cid, next } = readCid(bytes.subarray(0, end), section.next);
    const data = bytes.subarray(next, end);
    if (data.length > limits.maxBlockBytes)
      throw new Error(
        "the archive holds a block larger than this host accepts",
      );
    if (!verifyBlock(cid, data))
      throw new Error("a block in the archive does not match its identifier");
    blocks.set(cidKey(cid), data);
    at = end;
  }
  return { roots: header.roots, blocks };
}
