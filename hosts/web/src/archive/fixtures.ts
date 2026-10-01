import { type Cid, DAG_PB, RAW, cidFor, cidKey } from "./cid.js";

/** Test-only encoders for the formats an archive is made of. */

function varint(value: number): number[] {
  const out: number[] = [];
  let rest = value;
  while (rest >= 0x80) {
    out.push((rest % 0x80) | 0x80);
    rest = Math.floor(rest / 0x80);
  }
  out.push(rest);
  return out;
}

function field(number: number, wire: number, payload: number[]): number[] {
  return [...varint(number * 8 + wire), ...payload];
}

function lengthDelimited(number: number, bytes: ArrayLike<number>): number[] {
  return field(number, 2, [...varint(bytes.length), ...Array.from(bytes)]);
}

export interface UnixfsOptions {
  type: number;
  data?: Uint8Array;
  fileSize?: number;
  blockSizes?: number[];
}

/** A dag-pb node holding UnixFS metadata and links. */
export function dagPb(
  unixfs: UnixfsOptions,
  links: { cid: Cid; name: string }[] = [],
): Uint8Array {
  const meta = [
    ...field(1, 0, varint(unixfs.type)),
    ...(unixfs.data ? lengthDelimited(2, unixfs.data) : []),
    ...(unixfs.fileSize !== undefined
      ? field(3, 0, varint(unixfs.fileSize))
      : []),
    ...(unixfs.blockSizes ?? []).flatMap((size) => field(4, 0, varint(size))),
  ];
  return Uint8Array.from([
    ...links.flatMap((link) =>
      lengthDelimited(2, [
        ...lengthDelimited(1, link.cid.bytes),
        ...lengthDelimited(2, new TextEncoder().encode(link.name)),
      ]),
    ),
    ...lengthDelimited(1, meta),
  ]);
}

/** A set of blocks and the CID of each, in the order they were added. */
export class Blocks {
  readonly all = new Map<string, { cid: Cid; data: Uint8Array }>();

  add(codec: number, data: Uint8Array): Cid {
    const cid = cidFor(codec, data);
    this.all.set(cidKey(cid), { cid, data });
    return cid;
  }

  /** A file as raw leaves of `chunkSize` under one file node. */
  file(bytes: Uint8Array, chunkSize = 1024): Cid {
    if (bytes.length <= chunkSize && chunkSize === Infinity)
      return this.add(RAW, bytes);
    const leaves: { cid: Cid; name: string }[] = [];
    const sizes: number[] = [];
    for (let at = 0; at < bytes.length; at += chunkSize) {
      const chunk = bytes.slice(at, at + chunkSize);
      leaves.push({ cid: this.add(RAW, chunk), name: "" });
      sizes.push(chunk.length);
    }
    return this.add(
      DAG_PB,
      dagPb({ type: 2, fileSize: bytes.length, blockSizes: sizes }, leaves),
    );
  }

  directory(entries: Record<string, Cid>): Cid {
    return this.add(
      DAG_PB,
      dagPb(
        { type: 1 },
        Object.entries(entries).map(([name, cid]) => ({ cid, name })),
      ),
    );
  }

  /** The directory tree for `files`, keyed by slash-separated path. */
  tree(files: Record<string, string | Uint8Array>): Cid {
    const nested: Record<string, unknown> = {};
    for (const [path, content] of Object.entries(files)) {
      const parts = path.split("/");
      let level = nested;
      for (const part of parts.slice(0, -1))
        level = (level[part] ??= {}) as Record<string, unknown>;
      level[parts.at(-1) as string] =
        typeof content === "string"
          ? new TextEncoder().encode(content)
          : content;
    }
    const build = (level: Record<string, unknown>): Cid => {
      const entries: Record<string, Cid> = {};
      for (const [name, value] of Object.entries(level))
        entries[name] =
          value instanceof Uint8Array
            ? this.file(value)
            : build(value as Record<string, unknown>);
      return this.directory(entries);
    };
    return build(nested);
  }
}

function cborHead(major: number, value: number): number[] {
  if (value < 24) return [(major << 5) | value];
  if (value < 256) return [(major << 5) | 24, value];
  return [(major << 5) | 25, value >> 8, value & 0xff];
}

/** A CARv1 archive of `blocks` with `roots`. */
export function car(roots: Cid | Cid[], blocks: Blocks): Uint8Array {
  const text = (value: string) => [
    ...cborHead(3, value.length),
    ...new TextEncoder().encode(value),
  ];
  const listed = Array.isArray(roots) ? roots : [roots];
  const header = [
    ...cborHead(5, 2),
    ...text("roots"),
    ...cborHead(4, listed.length),
    ...listed.flatMap((root) => {
      const link = [0, ...root.bytes];
      return [...cborHead(6, 42), ...cborHead(2, link.length), ...link];
    }),
    ...text("version"),
    ...cborHead(0, 1),
  ];
  const sections = [...blocks.all.values()].flatMap(({ cid, data }) => [
    ...varint(cid.bytes.length + data.length),
    ...cid.bytes,
    ...Array.from(data),
  ]);
  return Uint8Array.from([...varint(header.length), ...header, ...sections]);
}
