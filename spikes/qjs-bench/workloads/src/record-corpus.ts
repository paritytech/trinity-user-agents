// T2: build corpus/corpus.bin by replaying the generated host frames against
// the bench worker under Bun, recording every frame it answers with.
import { mkdirSync, writeFileSync } from "node:fs";
import { encodeCorpus } from "./corpus-format";
import { hostFrames } from "./host-frames";
import { replay } from "./replay";

const steps = await replay(hostFrames());
const corpus = encodeCorpus(steps);
const out = new URL("../corpus/corpus.bin", import.meta.url);
mkdirSync(new URL(".", out), { recursive: true });
writeFileSync(out, corpus);

const outboundFrames = steps.reduce((sum, step) => sum + step.outbound.length, 0);
console.log(`corpus: ${steps.length} inbound frames, ${outboundFrames} outbound frames, ${corpus.length} bytes`);
