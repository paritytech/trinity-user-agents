import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { checkedPostingNode } from "./allowance-usable.js";

/**
 * The People chain's statement store over one JSON-RPC connection:
 * `statement_subscribeStatement`, `statement_unsubscribeStatement` and
 * `statement_submit`, the methods the core and the Polkadot app use.
 */

/** Which statements a subscription delivers: those with every topic, or with any. */
export type TopicFilter =
  | { matchAll: Uint8Array[] }
  | { matchAny: Uint8Array[] };

/**
 * How the store answered a submission. From a full node, `new` and `known`
 * mean it holds the statement; from this host's light client, `new` means at
 * least one peer took the broadcast, which may still refuse it.
 */
export interface SubmitResult {
  status: string;
  reason?: string;
}

/** A statement store a client subscribes to and submits to: {@link StatementStore}, or a stand-in. */
export interface StatementStoreLike {
  /** Check the actual submit connection, not another node behind the same endpoint. Absent means unknown. */
  supportsBestBlockPosting?(): Promise<boolean>;
  subscribe(
    filter: TopicFilter,
    onStatements: (statements: Uint8Array[]) => void,
  ): Promise<() => void>;
  submit(statement: Uint8Array): Promise<SubmitResult>;
  close(): void;
  /** Settles when the connection ends for any reason, including {@link close}. */
  readonly closed: Promise<void>;
}

/** Whether the answer counts as held here: `new` or `known`. Not recipient delivery proof. */
export const isStored = (result: SubmitResult): boolean =>
  result.status === "new" || result.status === "known";

/** Verify a fresh connection before lending it to the core; no product requests or payloads are inspected. */
export async function checkedPostingConnection(
  connection: JsonRpcConnection,
  genesis: string,
  timeoutMs = 5_000,
): Promise<JsonRpcConnection> {
  const responses = connection.responses()[Symbol.asyncIterator]();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const read = async () => {
    const answers = new Map<string, unknown>();
    for (const [id, method, params] of [
      ["posting-genesis", "chain_getBlockHash", [0]],
      ["posting-version", "system_version", []],
      ["posting-methods", "rpc_methods", []],
    ] as const)
      connection.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    while (answers.size < 3) {
      const next = await responses.next();
      if (next.done) throw new Error("the posting connection closed");
      const message = JSON.parse(next.value) as RpcMessage;
      if (typeof message.id !== "string" || message.error !== undefined)
        throw new Error("the posting connection could not be verified");
      answers.set(message.id, message.result);
    }
    if (
      !checkedPostingNode(
        genesis,
        answers.get("posting-genesis"),
        answers.get("posting-version"),
        answers.get("posting-methods"),
      )
    )
      throw new Error(
        "the posting node is not eligible for best-block posting",
      );
  };
  try {
    await Promise.race([
      read(),
      new Promise<never>((_resolve, reject) => {
        timer = setTimeout(
          () => reject(new Error("the posting node check timed out")),
          timeoutMs,
        );
      }),
    ]);
    return {
      send: (text) => connection.send(text),
      async *responses() {
        for (
          let next = await responses.next();
          !next.done;
          next = await responses.next()
        )
          yield next.value;
      },
      close: () => connection.close(),
    };
  } catch (error) {
    connection.close();
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

interface RpcMessage {
  id?: number | string;
  result?: unknown;
  error?: { message?: string };
  params?: { subscription?: string; result?: unknown };
}

interface Pending {
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
}

function filterJson(filter: TopicFilter): Record<string, string[]> {
  return "matchAll" in filter
    ? { matchAll: filter.matchAll.map(bytesToHex) }
    : { matchAny: filter.matchAny.map(bytesToHex) };
}

/** The statements of a `newStatements` notification, or none for anything else. */
export function statementsOf(result: unknown): Uint8Array[] {
  if (typeof result !== "object" || result === null) return [];
  const { event, data } = result as {
    event?: unknown;
    data?: { statements?: unknown };
  };
  if (event !== "newStatements" || !Array.isArray(data?.statements)) return [];
  return data.statements
    .filter((item) => typeof item === "string")
    .map(hexToBytes);
}

/**
 * Sees every JSON-RPC text this store sends and receives, exactly as it goes
 * to and comes from the connection, before it is parsed.
 */
export type RpcTap = (direction: "in" | "out", text: string) => void;

export class StatementStore {
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Map<
    string,
    (statements: Uint8Array[]) => void
  >();
  private isClosed = false;
  private markClosed!: () => void;
  /** Settles when the connection ends, from either side. */
  readonly closed = new Promise<void>((resolve) => {
    this.markClosed = resolve;
  });

  constructor(
    private readonly connection: JsonRpcConnection,
    private readonly tap?: RpcTap,
    private readonly postingGenesis?: string,
  ) {
    void this.pump().finally(() => this.markClosed());
  }

  private async pump(): Promise<void> {
    try {
      for await (const raw of this.connection.responses()) {
        this.tap?.("in", raw);
        const message = JSON.parse(raw) as RpcMessage;
        if (typeof message.id === "number") {
          const waiting = this.pending.get(message.id);
          this.pending.delete(message.id);
          if (message.error !== undefined)
            waiting?.reject(
              new Error(message.error.message ?? "the chain refused a request"),
            );
          else waiting?.resolve(message.result);
          continue;
        }
        const subscription = message.params?.subscription;
        if (subscription === undefined) continue;
        const statements = statementsOf(message.params?.result);
        if (statements.length > 0)
          this.listeners.get(subscription)?.(statements);
      }
    } catch (error) {
      this.failAll(error instanceof Error ? error : new Error(String(error)));
      return;
    }
    this.failAll(new Error("the People chain connection closed"));
  }

  private failAll(error: Error): void {
    for (const waiting of this.pending.values()) waiting.reject(error);
    this.pending.clear();
  }

  private call(
    method: string,
    params: unknown[],
    timeoutMs?: number,
  ): Promise<unknown> {
    if (this.isClosed)
      return Promise.reject(new Error("the statement store is closed"));
    const id = this.nextId++;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const answer = new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      if (timeoutMs !== undefined)
        timer = setTimeout(() => {
          this.pending.delete(id);
          reject(new Error("the node identity read timed out"));
        }, timeoutMs);
    });
    const request = JSON.stringify({ jsonrpc: "2.0", id, method, params });
    this.tap?.("out", request);
    try {
      this.connection.send(request);
    } catch (error) {
      const waiting = this.pending.get(id);
      this.pending.delete(id);
      waiting?.reject(
        error instanceof Error ? error : new Error(String(error)),
      );
    }
    return answer.finally(() => clearTimeout(timer));
  }

  /** Verify this store's own connection before a best-block post. Unknown or unsupported nodes fail closed. */
  async supportsBestBlockPosting(): Promise<boolean> {
    if (this.postingGenesis === undefined) return false;
    try {
      const [genesis, version, methods] = await Promise.all([
        this.call("chain_getBlockHash", [0], 5_000),
        this.call("system_version", [], 5_000),
        this.call("rpc_methods", [], 5_000),
      ]);
      return checkedPostingNode(this.postingGenesis, genesis, version, methods);
    } catch {
      return false;
    }
  }

  /**
   * Deliver the statements that pass `filter` as the connection sends them:
   * a full node replays what it holds first, the light client sends only
   * what peers send it. Returns the way to stop.
   */
  async subscribe(
    filter: TopicFilter,
    onStatements: (statements: Uint8Array[]) => void,
  ): Promise<() => void> {
    const subscription = await this.call("statement_subscribeStatement", [
      filterJson(filter),
    ]);
    if (typeof subscription !== "string")
      throw new Error("the statement store gave no subscription id");
    this.listeners.set(subscription, onStatements);
    return () => {
      if (!this.listeners.delete(subscription) || this.isClosed) return;
      void this.call("statement_unsubscribeStatement", [subscription]).catch(
        () => undefined,
      );
    };
  }

  async submit(statement: Uint8Array): Promise<SubmitResult> {
    const result = await this.call("statement_submit", [bytesToHex(statement)]);
    if (typeof result === "object" && result !== null && "status" in result)
      return result as SubmitResult;
    throw new Error(`the statement store answered ${JSON.stringify(result)}`);
  }

  close(): void {
    if (this.isClosed) return;
    this.isClosed = true;
    this.markClosed();
    this.listeners.clear();
    this.failAll(new Error("the statement store is closed"));
    this.connection.close();
  }
}
