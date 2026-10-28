import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";

/**
 * The People chain's statement store over one JSON-RPC connection:
 * `statement_subscribeStatement`, `statement_unsubscribeStatement` and
 * `statement_submit`, the methods the core and the Polkadot app use.
 */

/** Which statements a subscription delivers: those with every topic, or with any. */
export type TopicFilter =
  | { matchAll: Uint8Array[] }
  | { matchAny: Uint8Array[] };

/** How the store answered a submission. `new` and `known` mean it holds the statement. */
export interface SubmitResult {
  status: string;
  reason?: string;
}

export const isStored = (result: SubmitResult): boolean =>
  result.status === "new" ||
  result.status === "known" ||
  result.status === "knownExpired";

interface RpcMessage {
  id?: number;
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
  ) {
    void this.pump().finally(() => this.markClosed());
  }

  private async pump(): Promise<void> {
    try {
      for await (const raw of this.connection.responses()) {
        this.tap?.("in", raw);
        const message = JSON.parse(raw) as RpcMessage;
        if (message.id !== undefined) {
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

  private call(method: string, params: unknown[]): Promise<unknown> {
    if (this.isClosed)
      return Promise.reject(new Error("the statement store is closed"));
    const id = this.nextId++;
    const answer = new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    const request = JSON.stringify({ jsonrpc: "2.0", id, method, params });
    this.tap?.("out", request);
    this.connection.send(request);
    return answer;
  }

  /** Deliver every stored and every new statement that passes `filter`. Returns the way to stop. */
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
