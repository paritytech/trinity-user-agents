// Evaluate one built workload file in this Bun process the way an engine
// would: install __truapi_frames, evaluate dist/w-<name>.js, then drive it.
//   worker:      W1 (eval to idle) and W2 (corpus replay, byte-for-byte)
//   pingpong-*:  echo every frame back
//   codec, rxjs: wait for the BENCH_RESULT line
// Prints one JSON line with the outcome; exits non-zero on a mismatch.
import { readFileSync } from "node:fs";
import { MESSAGE_TYPE_START, decodeWireMessage } from "@parity/truapi";
import { RENDERER_RENDER } from "@parity/truapi/wire-table";
import { decodeCorpus } from "./corpus-format";
import type { FrameChannel } from "./engine-globals";

const name = process.argv[2];
if (!name) throw new Error("usage: bun src/run-bundle.ts <workload>");
const source = readFileSync(new URL(`../dist/w-${name}.js`, import.meta.url), "utf8");

const listeners = new Set<(frame: Uint8Array) => void>();
let sent: Uint8Array[] = [];
const echo = name.startsWith("pingpong");
const channel: FrameChannel = {
  send(frame) {
    if (echo) {
      const copy = frame.slice();
      queueMicrotask(() => deliver(copy));
    } else {
      sent.push(frame.slice());
    }
  },
  subscribe(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};
const deliver = (frame: Uint8Array) => {
  for (const listener of [...listeners]) listener(frame);
};
globalThis.__truapi_frames = channel;

const result = new Promise<Record<string, unknown>>((resolve) => {
  const log = console.log;
  console.log = (...args: unknown[]) => {
    const line = args.map(String).join(" ");
    if (line.startsWith("BENCH_RESULT ")) resolve(JSON.parse(line.slice("BENCH_RESULT ".length)));
    else log(...args);
  };
});

/** Drain timers and promise jobs; used once after evaluation. */
const settle = async () => {
  for (let round = 0; round < 3; round++) await new Promise((resolve) => setTimeout(resolve, 0));
};

/**
 * Wait until the worker has sent `expected` frames, draining promise jobs
 * first so a step's timing is the worker's work, not a timer tick.
 */
const awaitReplies = async (expected: number) => {
  for (let job = 0; job < 64 && sent.length < expected; job++) await Promise.resolve();
  if (sent.length < expected) await settle();
};

const evalStarted = performance.now();
(0, eval)(source);

if (name !== "worker") {
  console.info(JSON.stringify({ engine: "bun", ...(await result) }));
  process.exit(0);
}

await settle();
const coldStartMs = performance.now() - evalStarted;

const steps = decodeCorpus(readFileSync(new URL("../corpus/corpus.bin", import.meta.url)));
const renderMs: number[] = [];
let mismatches = 0;
const equal = (left: Uint8Array, right: Uint8Array) =>
  left.length === right.length && left.every((byte, index) => byte === right[index]);
for (const step of steps) {
  const started = performance.now();
  deliver(step.inbound.slice());
  await awaitReplies(step.outbound.length);
  const elapsed = performance.now() - started;
  if (sent.length !== step.outbound.length || !step.outbound.every((frame, index) => equal(frame, sent[index]))) {
    mismatches += 1;
  }
  const { traitId, messageType } = decodeWireMessage(step.inbound)._unsafeUnwrap().payload;
  if (traitId === RENDERER_RENDER.trait && messageType === MESSAGE_TYPE_START) renderMs.push(elapsed);
  sent = [];
}
renderMs.sort((left, right) => left - right);
const percentile = (fraction: number) => renderMs[Math.min(renderMs.length - 1, Math.floor(renderMs.length * fraction))];
console.info(
  JSON.stringify({
    engine: "bun",
    workload: "worker",
    coldStartMs,
    steps: steps.length,
    mismatches,
    renderSamples: renderMs.length,
    renderP50Ms: percentile(0.5),
    renderP95Ms: percentile(0.95),
  }),
);
process.exit(mismatches === 0 ? 0 : 1);
