// corpus.bin: the conversation a benchmark replays.
//
//   "QJSC" | u32 version | u32 step count | steps…
//   step:  u32 inbound length | inbound bytes | u32 outbound count | (u32 length | bytes)…
//
// Each step is one inbound frame and every frame the worker sent before it
// went idle again. All integers are little-endian.

export interface Step {
  inbound: Uint8Array;
  outbound: Uint8Array[];
}

const MAGIC = new TextEncoder().encode("QJSC");
const VERSION = 1;

export function encodeCorpus(steps: Step[]): Uint8Array {
  const parts: Uint8Array[] = [MAGIC, u32(VERSION), u32(steps.length)];
  for (const step of steps) {
    parts.push(u32(step.inbound.length), step.inbound, u32(step.outbound.length));
    for (const frame of step.outbound) parts.push(u32(frame.length), frame);
  }
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function decodeCorpus(source: Uint8Array): Step[] {
  // A Node Buffer's slice is a view into the whole file, and scale-ts decodes
  // from `bytes.buffer` without the view's offset, so copy into a plain array
  // whose slices are copies too.
  const bytes = new Uint8Array(source);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let offset = 0;
  const read = () => {
    const value = view.getUint32(offset, true);
    offset += 4;
    return value;
  };
  const take = (len: number) => {
    const slice = bytes.slice(offset, offset + len);
    offset += len;
    return slice;
  };
  if (new TextDecoder().decode(take(4)) !== "QJSC") throw new Error("not a QJSC corpus");
  if (read() !== VERSION) throw new Error("unsupported corpus version");
  const steps: Step[] = [];
  for (let count = read(); count > 0; count--) {
    const inbound = take(read());
    const outbound: Uint8Array[] = [];
    for (let frames = read(); frames > 0; frames--) outbound.push(take(read()));
    steps.push({ inbound, outbound });
  }
  return steps;
}

function u32(value: number): Uint8Array {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value, true);
  return out;
}
