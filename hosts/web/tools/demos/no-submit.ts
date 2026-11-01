import type { JsonRpcConnection } from "@parity/truapi-host";
import type { Network } from "../../src/platform/network.js";

/**
 * DEV ONLY. A network that submits nothing, for a tab that only validates
 * claims: every JSON-RPC call that would submit an extrinsic or a statement,
 * or broadcast a transaction, is answered with an error here and never
 * reaches the light client. Reads, and the runtime's validation of a
 * transaction, go through.
 *
 * What it saw is kept in its record: each method it let through, counted,
 * and each one it refused.
 */

/** Methods that would put something on the network. */
const SUBMITS = /submit|broadcast/i;
/** How each refusal's message ends, after the method. */
const REFUSAL = " refused: this tab submits nothing";

/** Whether an RPC error is one of this guard's refusals, by its message. */
export const isNoSubmitRefusal = (error: { message?: string }): boolean =>
  error.message?.endsWith(REFUSAL) === true;

export interface NoSubmitRecord {
  /** Methods let through, by name, with how often. */
  passed: Record<string, number>;
  /** Methods refused, in order. */
  refused: string[];
}

/** `network`, refusing every submission, with what it saw. */
export function noSubmitNetwork(
  network: Network,
  log: (line: string) => void,
): { network: Network; record: NoSubmitRecord } {
  const record: NoSubmitRecord = { passed: {}, refused: [] };
  return {
    record,
    network: {
      ...network,
      chain: {
        async connect(genesisHash) {
          return guarded(await network.chain.connect(genesisHash), record, log);
        },
      },
    },
  };
}

function guarded(
  inner: JsonRpcConnection,
  record: NoSubmitRecord,
  log: (line: string) => void,
): JsonRpcConnection {
  /**
   * Every answer, the light client's and the refusals made here, in order.
   * One reader drains the light client, so a reader that stops never leaves
   * a read pending that would take the next answer.
   */
  const queue: string[] = [];
  let wake = () => {};
  let ended = false;
  const push = (text: string) => {
    queue.push(text);
    wake();
  };
  void (async () => {
    try {
      for await (const text of inner.responses()) push(text);
    } finally {
      ended = true;
      wake();
    }
  })();
  return {
    send(request) {
      let parsed: { id?: unknown; method?: unknown } = {};
      try {
        parsed = JSON.parse(request) as typeof parsed;
      } catch {
        // Not JSON: the light client answers it.
      }
      const method = typeof parsed.method === "string" ? parsed.method : "";
      if (!SUBMITS.test(method)) {
        record.passed[method] = (record.passed[method] ?? 0) + 1;
        inner.send(request);
        return;
      }
      record.refused.push(method);
      log(`Refused ${method}: this tab submits nothing.`);
      push(
        JSON.stringify({
          jsonrpc: "2.0",
          id: parsed.id ?? null,
          error: {
            code: -32000,
            message: `${method}${REFUSAL}`,
          },
        }),
      );
    },
    async *responses() {
      for (;;) {
        if (queue.length > 0) {
          yield queue.shift()!;
          continue;
        }
        if (ended) return;
        await new Promise<void>((resolve) => (wake = resolve));
      }
    },
    close() {
      inner.close();
    },
  };
}
