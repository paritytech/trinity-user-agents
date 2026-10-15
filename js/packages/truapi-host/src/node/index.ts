// `@parity/truapi-host/node`: the pairing host for a Node process, such as a
// CLI that pairs with the user's wallet. The Node counterpart of `./web`: the
// same callbacks, options and runtime, with the core in a worker thread
// instead of a Web Worker.

import {
  createWebWorkerPairingHostRuntime,
  type CreateWebWorkerPairingHostRuntimeOptions,
  type WebWorkerHostCallbacks,
  type WorkerPairingHostRuntime,
} from "../web/create-worker-host-runtime.js";
import { ThreadWorker } from "./thread-worker.js";

export type {
  CreateWebWorkerPairingHostRuntimeOptions as CreateNodePairingHostRuntimeOptions,
  WebWorkerHostCallbacks as NodeHostCallbacks,
  WebWorkerHostConfig as NodeHostConfig,
  WorkerPairingHostRuntime,
} from "../web/create-worker-host-runtime.js";
export {
  headlessHostCallbacks,
  type HeadlessHostCallbacks,
} from "./default-callbacks.js";

/**
 * Start a pairing host whose core runs in a worker thread. Resolves once the
 * core is loaded; `dispose()` stops the thread.
 */
export function createNodePairingHostRuntime(
  callbacks: WebWorkerHostCallbacks,
  options: CreateWebWorkerPairingHostRuntimeOptions,
): Promise<WorkerPairingHostRuntime> {
  const thread = new ThreadWorker(new URL("./worker.js", import.meta.url));
  return createWebWorkerPairingHostRuntime(
    thread as unknown as Worker,
    callbacks,
    options,
  );
}
