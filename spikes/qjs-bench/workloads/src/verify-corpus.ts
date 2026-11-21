// T2 done-when: a fresh replay under Bun reproduces every recorded outbound
// frame byte for byte, every frame decodes with the generated wire decoder,
// and each render request is answered with exactly one renderer tree.
import { readFileSync } from "node:fs";
import { MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_RESPONSE, decodeWireMessage } from "@parity/truapi";
import { RENDERER_RENDER, SYSTEM_HANDSHAKE } from "@parity/truapi/wire-table";
import { decodeCorpus } from "./corpus-format";
import { RENDER_REQUESTS } from "./host-frames";
import { replay } from "./replay";

const recorded = decodeCorpus(readFileSync(new URL("../corpus/corpus.bin", import.meta.url)));
const fresh = await replay(recorded.map((step) => step.inbound));

const failures: string[] = [];
const equal = (left: Uint8Array, right: Uint8Array) =>
  left.length === right.length && left.every((byte, index) => byte === right[index]);

recorded.forEach((step, index) => {
  const again = fresh[index].outbound;
  if (again.length !== step.outbound.length || !step.outbound.every((frame, i) => equal(frame, again[i]))) {
    failures.push(`step ${index}: replay sent ${again.length} frames, recording has ${step.outbound.length}, or bytes differ`);
  }
});

let renderTrees = 0;
let handshakeAnswers = 0;
let largest = 0;
for (const frame of recorded.flatMap((step) => step.outbound)) {
  largest = Math.max(largest, frame.length);
  const decoded = decodeWireMessage(frame);
  if (decoded.isErr()) {
    failures.push(`undecodable outbound frame: ${decoded.error.message}`);
    continue;
  }
  const { traitId, methodId, messageType } = decoded.value.payload;
  if (traitId === RENDERER_RENDER.trait && methodId === RENDERER_RENDER.method && messageType === MESSAGE_TYPE_RECEIVE) {
    renderTrees += 1;
  } else if (traitId === SYSTEM_HANDSHAKE.trait && methodId === SYSTEM_HANDSHAKE.method && messageType === MESSAGE_TYPE_RESPONSE) {
    handshakeAnswers += 1;
  } else {
    failures.push(`unexpected outbound frame (${traitId}, ${methodId}, ${messageType})`);
  }
}
if (renderTrees !== RENDER_REQUESTS) failures.push(`expected ${RENDER_REQUESTS} renderer trees, got ${renderTrees}`);
if (handshakeAnswers !== 1) failures.push(`expected 1 handshake answer, got ${handshakeAnswers}`);

if (failures.length > 0) {
  console.error(failures.slice(0, 10).join("\n"));
  process.exit(1);
}
console.log(
  `verified: ${recorded.length} steps reproduce byte for byte; ${handshakeAnswers} handshake answer, ${renderTrees} renderer trees, largest frame ${largest} bytes`,
);
