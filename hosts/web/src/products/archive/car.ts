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

function checkHeaderSize(size: number): void {
  if (size === 0 || size > MAX_HEADER_BYTES)
    throw new Error("CAR header has an unreasonable size");
}

function readHeader(bytes: Uint8Array): { roots: Cid[]; next: number } {
  const size = readVarint(bytes, 0);
  checkHeaderSize(size.value);
  const end = size.next + size.value;
  if (end > bytes.length) throw new Error("truncated CAR header");
  return { roots: decodeHeader(bytes.subarray(size.next, end)), next: end };
}

function decodeHeader(bytes: Uint8Array): Cid[] {
  const header = readCbor(bytes, 0, 0).value;
  if (!(header instanceof Map) || header.get("version") !== 1)
    throw new Error("not a CARv1 archive");
  const listed = header.get("roots");
  if (!Array.isArray(listed)) throw new Error("CAR header has no roots");
  return listed.map((root) => {
    const tagged = root as { tag?: number; value?: unknown };
    if (
      tagged.tag !== 42 ||
      !(tagged.value instanceof Uint8Array) ||
      tagged.value[0] !== 0
    )
      throw new Error("CAR root is not a CID link");
    return readCid(tagged.value, 1).cid;
  });
}

/** A block of an archive, checked against its identifier. */
export interface CarBlock {
  cid: Cid;
  data: Uint8Array;
}

/** Read one section's identifier and block, and check the block against it. */
function readSection(section: Uint8Array, limits: CarLimits): CarBlock {
  const { cid, next } = readCid(section, 0);
  const data = section.subarray(next);
  if (data.length > limits.maxBlockBytes)
    throw new Error("the archive holds a block larger than this host accepts");
  if (!verifyBlock(cid, data))
    throw new Error("a block in the archive does not match its identifier");
  return { cid, data };
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
    const { cid, data } = readSection(
      bytes.subarray(section.next, end),
      limits,
    );
    blocks.set(cidKey(cid), data);
    at = end;
  }
  return { roots: header.roots, blocks };
}

/** Room for a section's identifier on top of its block. Real CIDs are under 40 bytes. */
const MAX_CID_BYTES = 128;
/** Bytes enough for any varint `readVarint` accepts. */
const MAX_VARINT_PEEK = 8;

/**
 * Read a CARv1 stream of the DAG under `root` section by section, checking
 * every block against its identifier as it arrives. A header that does not
 * name `root` as its only root, a section that does not match, or one that
 * takes the stream past `limits`, ends the read with an error; the blocks
 * yielded before it stand. `maxBytes` bounds the block bytes it yields; a
 * section is checked against `maxBlockBytes` before it is buffered.
 */
export async function* readCarStream(
  chunks: AsyncIterable<Uint8Array>,
  root: Cid,
  limits: CarLimits,
): AsyncGenerator<CarBlock> {
  const source = chunks[Symbol.asyncIterator]();
  const queue = new ByteQueue();
  /** Buffer at least `length` bytes; false when the stream ends first. */
  async function fill(length: number): Promise<boolean> {
    while (queue.length < length) {
      const next = await source.next();
      if (next.done) return false;
      queue.push(next.value);
    }
    return true;
  }
  async function varint(): Promise<number> {
    for (;;) {
      const head = queue.peek(MAX_VARINT_PEEK);
      try {
        const { value, next } = readVarint(head, 0);
        queue.take(next);
        return value;
      } catch (error) {
        if (head.length >= MAX_VARINT_PEEK || !(await fill(head.length + 1)))
          throw error;
      }
    }
  }
  async function take(length: number, what: string): Promise<Uint8Array> {
    if (!(await fill(length))) throw new Error(`truncated CAR ${what}`);
    return queue.take(length);
  }

  try {
    if (!(await fill(1))) throw new Error("truncated CAR header");
    const headerSize = await varint();
    checkHeaderSize(headerSize);
    const roots = decodeHeader(await take(headerSize, "header"));
    if (roots.length !== 1 || cidKey(roots[0]) !== cidKey(root))
      throw new Error("the archive is not of the content asked for");
    let count = 0;
    let bytes = 0;
    while (await fill(1)) {
      const length = await varint();
      if (length === 0) throw new Error("truncated CAR section");
      if (length > limits.maxBlockBytes + MAX_CID_BYTES)
        throw new Error(
          "the archive holds a block larger than this host accepts",
        );
      count += 1;
      if (count > limits.maxBlocks)
        throw new Error("the archive holds more blocks than this host accepts");
      const block = readSection(await take(length, "section"), limits);
      bytes += block.data.length;
      if (bytes > limits.maxBytes)
        throw new Error("the archive is larger than this host accepts");
      yield block;
    }
  } finally {
    await source.return?.();
  }
}

/** Bytes from a stream, taken from the front in pieces of a known length. */
class ByteQueue {
  private chunks: Uint8Array[] = [];
  /** Bytes of `chunks[0]` already taken. */
  private offset = 0;
  length = 0;

  push(chunk: Uint8Array): void {
    if (chunk.length === 0) return;
    this.chunks.push(chunk);
    this.length += chunk.length;
  }

  /** Up to `count` bytes from the front, left in place. */
  peek(count: number): Uint8Array {
    const out = new Uint8Array(Math.min(count, this.length));
    let at = 0;
    let offset = this.offset;
    for (const chunk of this.chunks) {
      if (at === out.length) break;
      const piece = chunk.subarray(offset, offset + out.length - at);
      out.set(piece, at);
      at += piece.length;
      offset = 0;
    }
    return out;
  }

  /** Exactly `count` bytes from the front; the caller has checked they are here. */
  take(count: number): Uint8Array {
    const first = this.chunks[0];
    if (first !== undefined && first.length - this.offset >= count) {
      const out = first.subarray(this.offset, this.offset + count);
      this.advance(count);
      return out;
    }
    const out = new Uint8Array(count);
    let at = 0;
    while (at < count) {
      const chunk = this.chunks[0];
      const piece = chunk.subarray(
        this.offset,
        this.offset + Math.min(count - at, chunk.length - this.offset),
      );
      out.set(piece, at);
      at += piece.length;
      this.advance(piece.length);
    }
    return out;
  }

  private advance(count: number): void {
    this.length -= count;
    this.offset += count;
    if (this.offset === this.chunks[0].length) {
      this.chunks.shift();
      this.offset = 0;
    }
  }
}
