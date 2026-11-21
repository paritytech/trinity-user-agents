// Run the bench worker in this JS runtime against a list of inbound frames,
// collecting what it sends after each one.
import type { WireProvider } from "@parity/truapi";
import { startBenchWorker } from "./bench-worker";
import type { Step } from "./corpus-format";

export async function replay(inbound: Uint8Array[]): Promise<Step[]> {
  const listeners = new Set<(frame: Uint8Array) => void>();
  let sent: Uint8Array[] = [];
  const provider: WireProvider = {
    postMessage: (frame) => sent.push(frame.slice()),
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    dispose: () => listeners.clear(),
  };
  startBenchWorker(provider);
  await settle();
  if (sent.length > 0) throw new Error(`worker sent ${sent.length} frames before any inbound frame`);

  const steps: Step[] = [];
  for (const frame of inbound) {
    for (const listener of [...listeners]) listener(frame.slice());
    await settle();
    steps.push({ inbound: frame, outbound: sent });
    sent = [];
  }
  return steps;
}

/** Drain promise jobs and any zero-delay timers they schedule. */
async function settle(): Promise<void> {
  for (let round = 0; round < 3; round++) await new Promise((resolve) => setTimeout(resolve, 0));
}
