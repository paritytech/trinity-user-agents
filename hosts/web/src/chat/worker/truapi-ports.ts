import type {
  HexString,
  RemoteChainHeadFollowItem,
  SignedStatement,
  TrUApiClient,
} from "@parity/truapi";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { utf8 } from "../nova/codec.js";
import {
  accountNamesKey,
  allowanceKey,
  consumerKey,
  decodeAccountNames,
  decodePeopleConsumer,
  liteLabelOwnerKey,
  usernameOwnerKey,
  type ChainDirectory,
} from "../nova/directory.js";
import type { StatementStoreLike } from "../nova/engine.js";
import { decodeStatement, encodeSigned } from "../nova/statement.js";
import type { SubmitResult, TopicFilter } from "../nova/statement-store.js";

/**
 * The Nova engine's ports over the product's own TrUAPI client, so the worker
 * reaches the network and its storage the way any product does: the core
 * scopes storage to the product and asks the user before the first
 * statement is submitted.
 *
 * `statement_store.submit` takes a `SignedStatement` and the core encodes it
 * again as the store's field list, sorted by tag, with the proof unchanged
 * (`rust/crates/truapi/src/host_logic/statement_store/statement.rs`,
 * `signed_statement_to_scale`). For an sr25519 statement the engine built,
 * that is the same bytes and the same signer: the core does not sign again.
 * Subscriptions deliver decoded statements, which are encoded here again the
 * same way, so the engine sees the bytes the store holds.
 */

/** The store's limit on `matchAny` topics per subscription. */
const MAX_MATCH_ANY_TOPICS = 128;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : JSON.stringify(error);

/** A statement the engine built, as the TrUAPI `SignedStatement` it submits. */
export function toSignedStatement(encoded: Uint8Array): SignedStatement {
  const statement = decodeStatement(encoded);
  return {
    proof: {
      tag: "Sr25519",
      value: {
        signature: bytesToHex(statement.signature),
        signer: bytesToHex(statement.signer),
      },
    },
    expiry: statement.expiry,
    ...(statement.channel === null
      ? {}
      : { channel: bytesToHex(statement.channel) }),
    topics: statement.topics.map(bytesToHex),
    data: bytesToHex(statement.data),
  };
}

/**
 * A delivered statement as the store's bytes, or null for a shape the Nova
 * wire never uses: another proof kind, a decryption key, no expiry or no data.
 */
export function fromSignedStatement(
  statement: SignedStatement,
): Uint8Array | null {
  const proof = statement.proof;
  if (
    proof.tag !== "Sr25519" ||
    statement.decryptionKey !== undefined ||
    statement.expiry === undefined ||
    statement.data === undefined
  )
    return null;
  return encodeSigned(
    {
      expiry: statement.expiry,
      channel:
        statement.channel === undefined ? null : hexToBytes(statement.channel),
      topics: statement.topics.map(hexToBytes),
      data: hexToBytes(statement.data),
    },
    hexToBytes(proof.value.signer),
    () => hexToBytes(proof.value.signature),
  );
}

type StatementClient = Pick<TrUApiClient, "statementStore">;

/** The Statement Store through `statement_store.subscribe` and `submit`. */
export function truapiStatementStore(
  client: StatementClient,
): StatementStoreLike {
  let open = true;
  let markClosed!: () => void;
  const closed = new Promise<void>((resolve) => (markClosed = resolve));
  const stops = new Set<() => void>();
  const end = () => {
    if (!open) return;
    open = false;
    for (const stop of [...stops]) stop();
    markClosed();
  };

  /** Subscribe to one group of topics. A failed or ended subscription closes the store. */
  const subscribeOne = (
    request: { tag: "MatchAll" | "MatchAny"; value: HexString[] },
    onStatements: (statements: Uint8Array[]) => void,
  ): (() => void) => {
    const subscription = client.statementStore
      .subscribe({ request })
      .subscribe({
        next(item) {
          const statements = item.statements
            .map(fromSignedStatement)
            .filter((entry): entry is Uint8Array => entry !== null);
          if (open && statements.length > 0) onStatements(statements);
        },
        // The engine connects again once the store is closed.
        error: end,
        complete: end,
      });
    const stop = () => {
      if (stops.delete(stop)) subscription.unsubscribe();
    };
    stops.add(stop);
    return stop;
  };

  return {
    closed,
    async subscribe(filter: TopicFilter, onStatements) {
      if (!open) throw new Error("the statement store is closed");
      const groups =
        "matchAll" in filter
          ? [
              {
                tag: "MatchAll" as const,
                value: filter.matchAll.map(bytesToHex),
              },
            ]
          : Array.from(
              {
                length: Math.ceil(
                  filter.matchAny.length / MAX_MATCH_ANY_TOPICS,
                ),
              },
              (_, index) => ({
                tag: "MatchAny" as const,
                value: filter.matchAny
                  .slice(
                    index * MAX_MATCH_ANY_TOPICS,
                    (index + 1) * MAX_MATCH_ANY_TOPICS,
                  )
                  .map(bytesToHex),
              }),
            );
      const ended = groups.map((group) => subscribeOne(group, onStatements));
      return () => {
        for (const stop of ended) stop();
      };
    },
    async submit(encoded): Promise<SubmitResult> {
      if (!open) throw new Error("the statement store is closed");
      const result = await client.statementStore.submit(
        toSignedStatement(encoded),
      );
      return result.isOk()
        ? { status: "new" }
        : { status: "rejected", reason: errorText(result.error) };
    },
    close: end,
  };
}

type ChainClient = Pick<TrUApiClient, "chain">;

/** How long one storage read may take. */
const READ_TIMEOUT_MS = 30_000;

/**
 * Read `keys` at the newest finalized block of the chain with `genesisHash`,
 * through `chain.followHeadSubscribe` and `chain.getHeadStorage`. A key with
 * no value reads as null.
 */
export function readStorage(
  client: ChainClient,
  genesisHash: HexString,
  keys: HexString[],
  timeoutMs = READ_TIMEOUT_MS,
): Promise<(HexString | null)[]> {
  return new Promise((resolve, reject) => {
    const found = new Map<string, HexString>();
    const done = new Set<string>();
    let operation: string | null = null;
    let settled = false;
    const finish = (error?: Error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      subscription.unsubscribe();
      if (error !== undefined) reject(error);
      else resolve(keys.map((key) => found.get(key.toLowerCase()) ?? null));
    };
    const timer = setTimeout(
      () => finish(new Error(`no storage answer after ${timeoutMs / 1000}s`)),
      timeoutMs,
    );
    const mine = (id: string) => operation === id;
    const handle = (item: RemoteChainHeadFollowItem) => {
      switch (item.tag) {
        case "Initialized": {
          const hash = item.value.finalizedBlockHashes.at(-1);
          if (hash === undefined)
            return finish(new Error("the chain has no finalized block"));
          void client.chain
            .getHeadStorage({
              genesisHash,
              followSubscriptionId: subscription.subscriptionId,
              hash,
              items: keys.map((key) => ({ key, queryType: "Value" as const })),
            })
            .then((result) => {
              if (result.isErr())
                return finish(new Error(errorText(result.error)));
              if (result.value.operation.tag !== "Started")
                return finish(new Error("the chain is busy, try again"));
              operation = result.value.operation.value.operationId;
              if (done.has(operation)) finish();
            });
          return;
        }
        case "OperationStorageItems":
          for (const entry of item.value.items)
            if (entry.value !== undefined)
              found.set(entry.key.toLowerCase(), entry.value);
          return;
        case "OperationStorageDone":
          done.add(item.value.operationId);
          if (mine(item.value.operationId)) finish();
          return;
        case "OperationError":
          if (mine(item.value.operationId)) finish(new Error(item.value.error));
          return;
        case "OperationInaccessible":
          if (mine(item.value.operationId))
            finish(new Error("the storage is not reachable now"));
          return;
        case "Stop":
          return finish(new Error("the chain stopped following"));
        default:
          return;
      }
    };
    const subscription = client.chain
      .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
      .subscribe({
        next: handle,
        error: (error) => finish(new Error(errorText(error.reason ?? error))),
        complete: () => finish(new Error("the chain stopped following")),
      });
  });
}

/** The chat directory (`../nova/directory.ts`) over the product's chain access. */
export function truapiDirectory(
  client: ChainClient,
  genesis: { assetHub: HexString; people: HexString },
): ChainDirectory {
  const one = async (genesisHash: HexString, key: HexString) =>
    (await readStorage(client, genesisHash, [key]))[0];
  const account = (value: HexString | null) => {
    if (value === null) return null;
    const bytes = hexToBytes(value);
    if (bytes.length !== 32) throw new Error("the owner is not an account");
    return bytes;
  };
  return {
    async names(owner) {
      const value = await one(genesis.assetHub, accountNamesKey(owner));
      return value === null ? null : decodeAccountNames(hexToBytes(value));
    },
    async consumer(owner) {
      const value = await one(genesis.people, consumerKey(owner));
      return value === null ? null : decodePeopleConsumer(hexToBytes(value));
    },
    async owners(label) {
      const [assetHub, people] = await Promise.all([
        one(genesis.assetHub, liteLabelOwnerKey(label)),
        one(genesis.people, usernameOwnerKey(label)),
      ]);
      return { assetHub: account(assetHub), people: account(people) };
    },
    async hasAllowance(owner) {
      return (await one(genesis.people, allowanceKey(owner))) !== null;
    },
  };
}

type StorageClient = Pick<TrUApiClient, "localStorage">;

/**
 * A `Storage` the engine reads and writes at once, kept in the product's
 * TrUAPI storage. {@link ProductStorage.load} reads the keys first; each
 * write goes to the host in order, and {@link ProductStorage.flush} waits for
 * them. The host keeps the value per product, so it outlives the worker.
 */
export class ProductStorage implements Storage {
  private readonly items = new Map<string, string>();
  private writes: Promise<void> = Promise.resolve();

  private constructor(
    private readonly client: StorageClient,
    private readonly onError: (reason: string) => void,
  ) {}

  static async load(
    client: StorageClient,
    keys: string[],
    onError: (reason: string) => void,
  ): Promise<ProductStorage> {
    const storage = new ProductStorage(client, onError);
    for (const key of keys) {
      const read = await client.localStorage.read({ key });
      if (read.isErr())
        throw new Error(
          `The worker's storage could not be read: ${errorText(read.error)}`,
        );
      const value = read.value.value;
      if (value !== undefined)
        storage.items.set(key, new TextDecoder().decode(hexToBytes(value)));
    }
    return storage;
  }

  get length(): number {
    return this.items.size;
  }

  key(index: number): string | null {
    return [...this.items.keys()][index] ?? null;
  }

  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    if (this.items.get(key) === value) return;
    this.items.set(key, value);
    this.queue(async () => {
      const written = await this.client.localStorage.write({
        key,
        value: bytesToHex(utf8(value)),
      });
      if (written.isErr()) this.onError(errorText(written.error));
    });
  }

  removeItem(key: string): void {
    if (!this.items.delete(key)) return;
    this.queue(async () => {
      const cleared = await this.client.localStorage.clear({ key });
      if (cleared.isErr()) this.onError(errorText(cleared.error));
    });
  }

  clear(): void {
    for (const key of [...this.items.keys()]) this.removeItem(key);
  }

  /** Resolves once every write so far reached the host. */
  flush(): Promise<void> {
    return this.writes;
  }

  private queue(write: () => Promise<void>): void {
    this.writes = this.writes
      .then(write)
      .catch((error: unknown) => this.onError(errorText(error)));
  }
}
