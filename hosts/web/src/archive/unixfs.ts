import { type Cid, DAG_PB, RAW, readCid, readVarint } from "./cid.js";

/** What an unpacked site may contain before it is refused. */
export interface UnpackLimits {
  maxFiles: number;
  /**
   * Blocks the traversal may read, counted once per reference. Empty nodes and
   * directories count, so repeated links cannot multiply work for free.
   */
  maxNodes: number;
  /** Total block bytes read, counted once per reference to a block. */
  maxTotalBytes: number;
  maxDirectoryDepth: number;
  maxFileTreeDepth: number;
  maxPathBytes: number;
}

/**
 * A block by identifier, already checked against it by the implementation.
 * `signal` aborts the read when the load is cancelled or runs out of time.
 */
export type GetBlock = (cid: Cid, signal?: AbortSignal) => Promise<Uint8Array>;

const DIRECTORY = 1;
const FILE = 2;
const HAMT_SHARD = 5;
const RAW_TYPE = 0;
const MAX_NAME_BYTES = 255;

interface Field {
  number: number;
  wire: number;
  varint: number;
  bytes: Uint8Array;
}

function* readFields(bytes: Uint8Array): Generator<Field> {
  let at = 0;
  while (at < bytes.length) {
    const key = readVarint(bytes, at);
    const number = Math.floor(key.value / 8);
    const wire = key.value % 8;
    at = key.next;
    if (wire === 0) {
      const value = readVarint(bytes, at);
      at = value.next;
      yield { number, wire, varint: value.value, bytes: bytes.subarray(0, 0) };
    } else if (wire === 2) {
      const length = readVarint(bytes, at);
      const end = length.next + length.value;
      if (end > bytes.length) throw new Error("truncated protobuf field");
      yield {
        number,
        wire,
        varint: 0,
        bytes: bytes.subarray(length.next, end),
      };
      at = end;
    } else throw new Error("unsupported protobuf wire type");
  }
}

interface Link {
  cid: Cid;
  name: string;
}

interface Node {
  links: Link[];
  data: Uint8Array;
}

function decodeNode(bytes: Uint8Array): Node {
  const links: Link[] = [];
  let data: Uint8Array = new Uint8Array(0);
  for (const field of readFields(bytes)) {
    if (field.number === 1 && field.wire === 2) data = field.bytes;
    else if (field.number === 2 && field.wire === 2) {
      let cid: Cid | undefined;
      let name = "";
      for (const part of readFields(field.bytes)) {
        if (part.number === 1 && part.wire === 2) {
          const read = readCid(part.bytes, 0);
          if (read.next !== part.bytes.length)
            throw new Error("trailing bytes after a link's CID");
          cid = read.cid;
        } else if (part.number === 2 && part.wire === 2)
          name = new TextDecoder("utf-8", { fatal: true }).decode(part.bytes);
      }
      if (cid === undefined) throw new Error("a link has no target");
      links.push({ cid, name });
    }
  }
  return { links, data };
}

interface UnixfsData {
  type: number;
  data: Uint8Array;
  fileSize: number | undefined;
  blockSizes: number[];
}

function decodeUnixfs(bytes: Uint8Array): UnixfsData {
  const result: UnixfsData = {
    type: RAW_TYPE,
    data: new Uint8Array(0),
    fileSize: undefined,
    blockSizes: [],
  };
  for (const field of readFields(bytes)) {
    if (field.number === 1 && field.wire === 0) result.type = field.varint;
    else if (field.number === 2 && field.wire === 2) result.data = field.bytes;
    else if (field.number === 3 && field.wire === 0)
      result.fileSize = field.varint;
    else if (field.number === 4 && field.wire === 0)
      result.blockSizes.push(field.varint);
  }
  return result;
}

/** Whether `name` may be one path segment of an unpacked site. */
export function isSafeName(name: string): boolean {
  return (
    name !== "" &&
    name !== "." &&
    name !== ".." &&
    !/[/\\\0]/.test(name) &&
    new TextEncoder().encode(name).length <= MAX_NAME_BYTES
  );
}

/** Read the nodes of a dag-pb block as UnixFS, or `undefined` for a raw block. */
function readUnixfsNode(
  cid: Cid,
  block: Uint8Array,
): { node: Node; unixfs: UnixfsData } | undefined {
  if (cid.codec === RAW) return undefined;
  if (cid.codec !== DAG_PB)
    throw new Error(`unsupported block codec 0x${cid.codec.toString(16)}`);
  const node = decodeNode(block);
  return { node, unixfs: decodeUnixfs(node.data) };
}

/**
 * One load's reads. Every reference the traversal follows is charged here, so
 * a crafted DAG cannot expand past the limits or outlast the signal.
 */
class Traversal {
  private nodes = 0;
  private bytes = 0;

  constructor(
    private readonly getBlock: GetBlock,
    private readonly limits: Pick<UnpackLimits, "maxNodes" | "maxTotalBytes">,
    private readonly signal: AbortSignal | undefined,
  ) {}

  async visit(cid: Cid): Promise<Uint8Array> {
    this.checkSignal();
    this.nodes += 1;
    if (this.nodes > this.limits.maxNodes)
      throw new Error("the content has more blocks than this host accepts");
    let block: Uint8Array;
    try {
      block = await this.getBlock(cid, this.signal);
    } catch (error) {
      this.checkSignal();
      throw error;
    }
    this.checkSignal();
    this.bytes += block.length;
    if (this.bytes > this.limits.maxTotalBytes)
      throw new Error("the content is larger than this host accepts");
    return block;
  }

  private checkSignal(): void {
    if (this.signal?.aborted)
      throw new Error("loading the content took too long or was cancelled");
  }
}

async function readFileInto(
  traversal: Traversal,
  cid: Cid,
  depth: number,
  maxDepth: number,
  out: Uint8Array[],
): Promise<number> {
  if (depth > maxDepth) throw new Error("a file is nested too deeply");
  return readFileFrom(
    traversal,
    cid,
    await traversal.visit(cid),
    depth,
    maxDepth,
    out,
  );
}

/** Read the file whose block, `block`, the traversal already fetched. */
async function readFileFrom(
  traversal: Traversal,
  cid: Cid,
  block: Uint8Array,
  depth: number,
  maxDepth: number,
  out: Uint8Array[],
): Promise<number> {
  const parsed = readUnixfsNode(cid, block);
  if (parsed === undefined) {
    out.push(block);
    return block.length;
  }
  const { node, unixfs } = parsed;
  if (unixfs.type !== FILE && unixfs.type !== RAW_TYPE)
    throw new Error("expected a file");
  let length = 0;
  if (unixfs.data.length > 0) {
    out.push(unixfs.data);
    length += unixfs.data.length;
  }
  for (const [index, link] of node.links.entries()) {
    const read = await readFileInto(
      traversal,
      link.cid,
      depth + 1,
      maxDepth,
      out,
    );
    const declared = unixfs.blockSizes[index];
    if (declared !== undefined && declared !== read)
      throw new Error("a file chunk is not the size its parent records");
    length += read;
  }
  if (unixfs.fileSize !== undefined && unixfs.fileSize !== length)
    throw new Error("a file is not the size its node records");
  return length;
}

function concat(parts: Uint8Array[], length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  let at = 0;
  for (const part of parts) {
    bytes.set(part, at);
    at += part.length;
  }
  return bytes;
}

/** The bytes of the UnixFS file or raw block `cid` names. */
export async function readFile(
  cid: Cid,
  getBlock: GetBlock,
  limits: Pick<UnpackLimits, "maxNodes" | "maxTotalBytes" | "maxFileTreeDepth">,
  signal?: AbortSignal,
): Promise<Uint8Array> {
  const parts: Uint8Array[] = [];
  const length = await readFileInto(
    new Traversal(getBlock, limits, signal),
    cid,
    0,
    limits.maxFileTreeDepth,
    parts,
  );
  return concat(parts, length);
}

/**
 * Every file under the UnixFS directory `root`, by path from the root with no
 * leading slash.
 *
 * Names are checked as path segments, so a name cannot climb out of the site or
 * hide a separator. A sharded directory, a symlink or any other node type is an
 * error: the site would otherwise be silently incomplete.
 */
export async function unpackDirectory(
  root: Cid,
  getBlock: GetBlock,
  limits: UnpackLimits,
  signal?: AbortSignal,
): Promise<Map<string, Uint8Array>> {
  const files = new Map<string, Uint8Array>();
  const traversal = new Traversal(getBlock, limits, signal);

  /** Walk the directory `cid` whose block, `block`, the traversal already fetched. */
  async function walk(
    cid: Cid,
    block: Uint8Array,
    path: string,
    depth: number,
  ): Promise<void> {
    if (depth > limits.maxDirectoryDepth)
      throw new Error("the site's directories are nested too deeply");
    const parsed = readUnixfsNode(cid, block);
    if (parsed === undefined || parsed.unixfs.type !== DIRECTORY) {
      if (parsed?.unixfs.type === HAMT_SHARD)
        throw new Error("a sharded directory is not supported");
      throw new Error(`${path || "the root"} is not a directory`);
    }
    const seen = new Set<string>();
    for (const link of parsed.node.links) {
      if (!isSafeName(link.name))
        throw new Error(`unsafe file name in ${path || "the root"}`);
      if (seen.has(link.name))
        throw new Error(`duplicate name ${link.name} in ${path || "the root"}`);
      seen.add(link.name);
      const child = path === "" ? link.name : `${path}/${link.name}`;
      if (new TextEncoder().encode(child).length > limits.maxPathBytes)
        throw new Error("a file path is too long");
      const target = await traversal.visit(link.cid);
      const kind = readUnixfsNode(link.cid, target);
      if (kind?.unixfs.type === DIRECTORY || kind?.unixfs.type === HAMT_SHARD)
        await walk(link.cid, target, child, depth + 1);
      else {
        if (files.size >= limits.maxFiles)
          throw new Error("the site has more files than this host accepts");
        const parts: Uint8Array[] = [];
        const length = await readFileFrom(
          traversal,
          link.cid,
          target,
          0,
          limits.maxFileTreeDepth,
          parts,
        );
        files.set(child, concat(parts, length));
      }
    }
  }

  await walk(root, await traversal.visit(root), "", 0);
  return files;
}
