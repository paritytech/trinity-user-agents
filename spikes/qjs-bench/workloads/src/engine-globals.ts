// What every engine provides to the bundle before it runs: the guest prelude
// in QuickJS, the webview shim in WKWebView, the harness under Bun.
import type { WireProvider } from "@parity/truapi";

export interface FrameChannel {
  send(frame: Uint8Array): void;
  subscribe(listener: (frame: Uint8Array) => void): () => void;
}

/** Which workload the bundle runs, set by the host before evaluating it. */
export type BenchConfig =
  | { workload: "worker" }
  | { workload: "codec"; iterations: number }
  | { workload: "rxjs"; events: number }
  | { workload: "pingpong"; bytes: number; iterations: number };

declare global {
  var __truapi_frames: FrameChannel;
  var __BENCH: BenchConfig | undefined;
}

export function frameProvider(channel: FrameChannel): WireProvider {
  return {
    postMessage: (frame) => channel.send(frame),
    subscribe: (listener) => channel.subscribe(listener),
    dispose: () => {},
  };
}

/** Results go out as one log line so every host can pick them up the same way. */
export function reportResult(result: Record<string, unknown>): void {
  console.log(`BENCH_RESULT ${JSON.stringify(result)}`);
}
