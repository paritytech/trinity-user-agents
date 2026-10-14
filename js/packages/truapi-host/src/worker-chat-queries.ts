// Worker half of the chat queries a host asks a product. The worker calls the
// core directly, as for the render subscription, because the frame path
// carries product requests only.

import { startWasmChatQuery, type ChatQueryName } from "./chat-queries.js";
import { errorMessage } from "./error.js";
import type { WorkerProductRuntime } from "./wasm-module.js";
import type { WorkerToMain } from "./worker-protocol.js";

type PostToMain = (msg: WorkerToMain) => void;

/**
 * Open chat queries, keyed by the query id of the main thread. The core id
 * goes with each entry, so that the disposal of one core stops only its own
 * queries.
 */
export type ChatQueries = Map<number, { coreId: number; stop: () => void }>;

/**
 * Start one chat query on the core and send its one answer to the main
 * thread. A core that refuses to start the query gives `chatQueryFailure`.
 */
export function handleChatQueryStart(
  core: WorkerProductRuntime | undefined,
  postToMain: PostToMain,
  queries: ChatQueries,
  coreId: number,
  queryId: number,
  name: ChatQueryName,
  request: Uint8Array,
): void {
  if (!core) {
    postToMain({
      kind: "chatQueryFailure",
      queryId,
      error: `${name} received for unknown core ${coreId}`,
    });
    return;
  }
  try {
    const stop = startWasmChatQuery(core, name, request, {
      onAnswer: (item) => {
        queries.delete(queryId);
        postToMain({ kind: "chatQueryAnswer", queryId, item });
      },
      onRefusal: (error) => {
        queries.delete(queryId);
        postToMain({ kind: "chatQueryRefusal", queryId, error });
      },
      onEnd: () => {
        queries.delete(queryId);
        postToMain({ kind: "chatQueryEnd", queryId });
      },
      onFailure: (error) => {
        queries.delete(queryId);
        postToMain({ kind: "chatQueryFailure", queryId, error });
      },
    });
    queries.set(queryId, { coreId, stop });
  } catch (err) {
    postToMain({ kind: "chatQueryFailure", queryId, error: errorMessage(err) });
  }
}

/** Stop one chat query. A second call has no effect. */
export function stopChatQuery(queries: ChatQueries, queryId: number): void {
  const entry = queries.get(queryId);
  if (!entry) return;
  queries.delete(queryId);
  entry.stop();
}

/** Stop every chat query of one core, before the worker frees that core. */
export function stopChatQueriesForCore(
  queries: ChatQueries,
  coreId: number,
): void {
  for (const [queryId, entry] of [...queries]) {
    if (entry.coreId === coreId) stopChatQuery(queries, queryId);
  }
}
