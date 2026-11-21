// W3 to W5: the micro workloads that explain the gate result when it is close.
import {
  VersionedProductRendererRenderItem,
  VersionedProductRendererRenderRequest,
  decodeWireMessage,
  encodeWireMessage,
} from "@parity/truapi";
import { RENDERER_RENDER } from "@parity/truapi/wire-table";
import { Subject, filter, lastValueFrom, map, scan, timeout } from "rxjs";
import { ChatDiagnosis, WORKER_DIAGNOSIS_METHODS } from "../../../../playground/worker/diagnosis";
import { reportResult, type BenchConfig } from "./engine-globals";
import { renderRequest } from "./host-frames";

export async function runMicro(config: Exclude<BenchConfig, { workload: "worker" }>): Promise<void> {
  switch (config.workload) {
    case "codec":
      return codec(config.iterations);
    case "rxjs":
      return rxjs(config.events);
    case "pingpong":
      return pingPong(config.bytes, config.iterations);
  }
}

/**
 * W3: the scale-ts work of one render round trip, both directions: decode an
 * inbound render request frame, encode the renderer tree reply, and decode
 * that reply back.
 */
function codec(iterations: number): void {
  const diagnosis = new ChatDiagnosis();
  WORKER_DIAGNOSIS_METHODS.slice(0, 4).forEach((id) => diagnosis.pass(id, "codec workload"));
  const tree = diagnosis.rendererNode();
  const inbound = renderRequest("h:1", 1);
  let bytes = 0;
  const started = performance.now();
  for (let index = 0; index < iterations; index++) {
    const message = decodeWireMessage(inbound)._unsafeUnwrap();
    VersionedProductRendererRenderRequest.dec(message.payload.value);
    const reply = encodeWireMessage({
      requestId: message.requestId,
      payload: {
        traitId: RENDERER_RENDER.trait,
        methodId: RENDERER_RENDER.method,
        messageType: 1,
        value: VersionedProductRendererRenderItem.enc({ tag: "V1", value: tree }),
      },
    })._unsafeUnwrap();
    VersionedProductRendererRenderItem.dec(decodeWireMessage(reply)._unsafeUnwrap().payload.value);
    bytes += inbound.length + reply.length;
  }
  const elapsedMs = performance.now() - started;
  reportResult({ workload: "codec", iterations, elapsedMs, roundTripsPerSecond: (iterations / elapsedMs) * 1000, bytes });
}

/**
 * W4: the operator chain shape the playground worker uses (filter, map,
 * a timeout, awaiting the stream as a promise), plus a running fold, over a
 * Subject. `lastValueFrom` keeps the chain subscribed for every event.
 */
async function rxjs(events: number): Promise<void> {
  const source = new Subject<number>();
  const last = lastValueFrom(
    source.pipe(
      filter((value) => value % 3 !== 0),
      map((value) => ({ value, label: `event-${value}` })),
      scan((count, event) => count + event.label.length, 0),
      timeout(60_000),
    ),
  );
  const started = performance.now();
  for (let value = 0; value < events; value++) source.next(value);
  source.complete();
  const folded = await last.catch(() => -1);
  const elapsedMs = performance.now() - started;
  reportResult({ workload: "rxjs", events, elapsedMs, eventsPerSecond: (events / elapsedMs) * 1000, folded });
}

/** W5: send a frame, wait for the host to echo it, repeat. */
async function pingPong(size: number, iterations: number): Promise<void> {
  const frame = new Uint8Array(size).map((_, index) => index & 0xff);
  const samples: number[] = [];
  for (let index = 0; index < iterations; index++) {
    const started = performance.now();
    await new Promise<void>((resolve) => {
      const unsubscribe = globalThis.__truapi_frames.subscribe(() => {
        unsubscribe();
        resolve();
      });
      globalThis.__truapi_frames.send(frame);
    });
    samples.push(performance.now() - started);
  }
  samples.sort((left, right) => left - right);
  const percentile = (fraction: number) => samples[Math.min(samples.length - 1, Math.floor(samples.length * fraction))];
  reportResult({ workload: "pingpong", bytes: size, iterations, p50Ms: percentile(0.5), p95Ms: percentile(0.95) });
}
