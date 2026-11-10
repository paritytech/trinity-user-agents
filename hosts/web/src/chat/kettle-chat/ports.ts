import type { ProductId, TrUApiClient } from "@parity/truapi";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { concat, utf8 } from "../../platform/codec.js";
import { blake2b256 } from "../../platform/crypto.js";
import { encodeSigned, statementHash } from "../../platform/statement.js";
import { KETTLE_GLUE, KETTLE_WASM, type IrohRelays } from "./route.js";

/**
 * PROTOTYPE. What Kettle Chat's worker gives Kettle's engine
 * (`kettle-chat-wasm/`): the ports of one conversation account, over the
 * product's own TrUAPI.
 *
 * - Statements are signed by the core with `statementStore.createProof`, as
 *   the product's own account at the conversation's `Raw` index, and
 *   submitted with `statementStore.submit`; each submission first makes sure
 *   the host allows that account to post (`chatAccount`). A subscription is
 *   `statementStore.subscribe`, and hands Kettle every signed statement as
 *   it came, proof and all: Kettle checks the proofs itself.
 * - Entropy is the product's `entropy.derive`, under a context bound to the
 *   conversation's index, so each conversation account has its own MLS
 *   identity and its own sealed state.
 * - Kettle's sealed state is kept in the product's TrUAPI storage, under
 *   keys named by the index.
 */

/** A statement as Kettle hands it over to be signed and submitted. */
export interface KettleUnsigned {
  expiry: bigint;
  channel?: HexString;
  topics: HexString[];
  data?: HexString;
}

export interface KettleSubmitAnswer {
  status: "new" | "known" | "knownExpired" | "rejected" | "invalid";
  reason?: string;
}

export interface KettlePorts {
  submit(statement: KettleUnsigned): Promise<KettleSubmitAnswer>;
  subscribe(
    filter: { kind: "any" | "matchAll" | "matchAny"; topics: HexString[] },
    deliver: (statement: unknown) => void,
    ended: (reason: string) => void,
  ): () => void;
  entropy(context: Uint8Array): Promise<Uint8Array>;
  requestAllowance(): Promise<"allocated" | "rejected" | "notAvailable">;
  read(name: string): Promise<Uint8Array | undefined>;
  write(name: string, value: Uint8Array): Promise<void>;
  remove(name: string): Promise<void>;
}

/** An event of a Kettle node, as `kettle-chat-wasm/src/events.rs` makes it. */
export type KettleEvent = { type: string } & Record<string, unknown>;

/**
 * What a node runs of Kettle's log transfer over iroh: off, on through
 * `relay` as `endpoint`, or unavailable, when the endpoint did not reach the
 * relay and the node runs on the Statement Store alone.
 */
export type IrohStatus =
  | { state: "off" }
  | {
      state: "on";
      relays: IrohRelays["relays"];
      /** The home relay its address names. */
      relay: string;
      endpoint: string;
    }
  | {
      state: "unavailable";
      /** Absent: what the worker asked for did not read. */
      relays?: IrohRelays["relays"];
      /** The custom relay it did not reach. */
      relay?: string;
      reason: string;
    };

/** A log transfer, or a peer's outbox that tells the node what it misses, as `kettle-chat-wasm/src/log_transfer.rs` makes it. */
export type KettleTrace =
  | {
      type: "outboxReceived";
      signer: string;
      envelopes: [number, number] | null;
      headSeq: number;
      bytes: number;
    }
  | { type: "fetchStarted"; sender: string; from: number }
  | { type: "fetchFinished"; sender: string; count?: number; error?: string };

/** What starts a node. */
export interface KettleStartOptions {
  /** Kettle's short intervals, for tests that run in real time. */
  fast?: boolean;
  /** Run Kettle's log transfer on these iroh relays. Absent: no endpoint. */
  iroh?: IrohRelays & { onlineTimeoutMs?: number };
  onTrace?(trace: KettleTrace): void;
}

/** A running Kettle node of `kettle-chat-wasm`. */
export interface KettleNode {
  account(): string;
  iroh(): IrohStatus;
  sendText(
    peer: string,
    text: string,
  ): Promise<{
    group: string;
    number: number;
    messageId?: string;
    timeMs: number;
  }>;
  directGroup(peer: string): Promise<string | null>;
  leaveGroup(group: string): Promise<void>;
  saveState(): Promise<void>;
  inspect(group: string): Promise<string | null>;
  stop(): Promise<void>;
  free(): void;
}

/** The `kettle-chat-wasm` module, once its WebAssembly is loaded. */
export interface KettleModule {
  revision: string;
  protocol: number;
  start(
    account: string,
    ports: KettlePorts,
    onEvent: (event: KettleEvent) => void,
    options: KettleStartOptions,
  ): Promise<KettleNode>;
  /** Remove the sealed state a node of `account` keeps through `ports`, while no node of it runs. */
  removeState(account: string, ports: KettlePorts): Promise<void>;
}

/** The glue `wasm-pack --target web` writes, as far as this worker uses it. */
interface KettleGlue {
  default(options: { module_or_path: URL }): Promise<unknown>;
  kettleRevision(): string;
  kettleProtocol(): number;
  KettleNode: {
    start: KettleModule["start"];
    removeState: KettleModule["removeState"];
  };
}

/** Load the module from the worker's own archive. */
export async function loadKettle(base: string): Promise<KettleModule> {
  const glue = (await import(
    /* @vite-ignore */ new URL(KETTLE_GLUE, base).href
  )) as KettleGlue;
  await glue.default({ module_or_path: new URL(KETTLE_WASM, base) });
  return {
    revision: glue.kettleRevision(),
    protocol: glue.kettleProtocol(),
    start: (account, ports, onEvent, options) =>
      glue.KettleNode.start(account, ports, onEvent, options),
    removeState: (account, ports) =>
      glue.KettleNode.removeState(account, ports),
  };
}

const errorText = (error: unknown): string => {
  if (error instanceof Error) return error.message;
  const reason = (error as { value?: { reason?: unknown } } | null)?.value
    ?.reason;
  return typeof reason === "string" ? reason : JSON.stringify(error);
};

/** The store's rejection a submit error names, as Kettle reads it. */
const REJECTIONS = [
  "noAllowance",
  "accountFull",
  "channelPriorityTooLow",
  "dataTooLarge",
  "storeFull",
] as const;

type PortsClient = Pick<
  TrUApiClient,
  "statementStore" | "entropy" | "localStorage"
>;

export interface ConversationPortsOptions {
  client: PortsClient;
  productId: ProductId;
  /** The conversation's raw derivation index, `0x` hex. */
  index: string;
  /** Null once the host allows the account to post now, or why not. */
  allow(): Promise<string | null>;
  /** The product's current lease and whether its node still owns this account. */
  lease?(): { until: number; provisional: boolean; running: boolean };
  schedule?(task: () => void, delayMs: number): () => void;
  now?(): number;
  /** Told what a port could not do. */
  report(reason: string): void;
  /** Told each submission, signed, with the store's answer. */
  observe?(submission: KettleSubmission): void;
}

/** One statement a conversation account submitted, as the developer tools record it. */
export interface KettleSubmission {
  at: string;
  signer: string;
  /** BLAKE2b-256 of the signed statement's encoding, as the store names it; null when it has no data. */
  hash: string | null;
  channel: string | null;
  topics: string[];
  expiry: string;
  dataBytes: number;
  outcome: string;
}

/** The store's name for a signed sr25519 statement, or null when it cannot be encoded here. */
function hashOf(
  statement: KettleUnsigned,
  signer: HexString,
  signature: HexString,
): string | null {
  if (statement.data === undefined) return null;
  const encoded = encodeSigned(
    {
      expiry: statement.expiry,
      channel:
        statement.channel === undefined ? null : hexToBytes(statement.channel),
      topics: statement.topics.map((topic) => hexToBytes(topic)),
      data: hexToBytes(statement.data),
    },
    hexToBytes(signer),
    () => hexToBytes(signature),
  );
  return bytesToHex(statementHash(encoded));
}

/** Where a conversation's Kettle state is kept in the product's storage. */
export const nodeStorageKey = (index: string, name: string) =>
  `kettle-chat:node:${index.slice(2, 18)}:${name}`;

export function conversationPorts(
  options: ConversationPortsOptions,
): KettlePorts {
  const { client, productId, index } = options;
  const productAccountId = {
    dotNsIdentifier: productId,
    derivationIndex: { tag: "Raw" as const, value: index as HexString },
  };
  const boundContext = (context: Uint8Array) =>
    blake2b256(
      concat(
        utf8("kettle-chat/conversation/v1"),
        hexToBytes(index as HexString),
        context,
      ),
    );
  const pending = new Map<
    string,
    {
      identity: string;
      signed: Parameters<TrUApiClient["statementStore"]["submit"]>[0];
    }
  >();
  const renewals = new Map<string, { token: symbol; cancel(): void }>();
  const running = () => options.lease?.().running !== false;
  const ports: KettlePorts = {
    async submit(statement) {
      if (!running()) throw new Error("This Kettle node stopped.");
      const refused = await options.allow();
      if (refused !== null) {
        options.report(`post: ${refused}`);
        return { status: "rejected", reason: "noAllowance" };
      }
      if (!running()) throw new Error("This Kettle node stopped.");
      const channel = statement.channel ?? "none";
      const identity = JSON.stringify({
        ...statement,
        expiry: statement.expiry.toString(),
      });
      let cached = pending.get(channel);
      if (cached?.identity !== identity) {
        const proof = await client.statementStore.createProof({
          productAccountId,
          statement,
        });
        if (proof.isErr())
          throw new Error(`createProof: ${errorText(proof.error)}`);
        cached = {
          identity,
          signed: { ...statement, proof: proof.value.proof },
        };
        pending.set(channel, cached);
      }
      if (!running()) throw new Error("This Kettle node stopped.");
      const submitted = await client.statementStore.submit(cached.signed);
      const signed = cached.signed.proof;
      const record = (outcome: string) =>
        options.observe?.({
          at: new Date().toISOString(),
          signer: "signer" in signed.value ? signed.value.signer : "",
          hash:
            signed.tag === "Sr25519"
              ? hashOf(statement, signed.value.signer, signed.value.signature)
              : null,
          channel: statement.channel ?? null,
          topics: statement.topics,
          expiry: statement.expiry.toString(),
          dataBytes:
            statement.data === undefined ? 0 : (statement.data.length - 2) / 2,
          outcome,
        });
      if (submitted.isOk()) {
        record("new");
        renewals.get(channel)?.cancel();
        renewals.delete(channel);
        const lease = options.lease?.();
        if (lease?.provisional && lease.running) {
          const token = Symbol();
          const schedule =
            options.schedule ??
            ((task: () => void, delay: number) => {
              const timer = setTimeout(task, delay);
              return () => clearTimeout(timer);
            });
          const cancel = schedule(
            () => {
              if (renewals.get(channel)?.token !== token || !running()) return;
              renewals.delete(channel);
              void (async () => {
                const why = await options.allow();
                if (why !== null) throw new Error(why);
                if (!running() || pending.get(channel) !== cached) return;
                if (options.lease?.().provisional)
                  throw new Error(
                    "The provisional claim is still unconfirmed; pending keys/outbox wait for Kettle's next retry.",
                  );
                const result = await ports.submit({
                  ...statement,
                  expiry: statement.expiry + 1n,
                });
                if (result.status !== "new" && result.status !== "known")
                  throw new Error(result.reason ?? result.status);
              })().catch((error: unknown) =>
                options.report(`Pending Kettle post: ${errorText(error)}`),
              );
            },
            Math.max(0, lease.until - (options.now ?? Date.now)()) + 1,
          );
          renewals.set(channel, { token, cancel });
        }
        return { status: "new" };
      }
      const reason = errorText(submitted.error);
      record(`error: ${reason}`);
      const rejection = REJECTIONS.find((name) => reason.includes(name));
      if (rejection !== undefined)
        return { status: "rejected", reason: rejection };
      throw new Error(`submit: ${reason}`);
    },
    subscribe(filter, deliver, ended) {
      if (filter.kind === "any") {
        queueMicrotask(() => ended("the host matches topics only"));
        return () => {};
      }
      // Kettle drops its ends of `deliver` and `ended` once it stops the
      // subscription: neither may be called after that.
      let stopped = false;
      const subscription = client.statementStore
        .subscribe({
          request: {
            tag: filter.kind === "matchAll" ? "MatchAll" : "MatchAny",
            value: filter.topics,
          },
        })
        .subscribe({
          next: (page) => {
            for (const statement of page.statements)
              if (!stopped) deliver(statement);
          },
          error: (error) => {
            if (stopped) return;
            options.report(`read: ${errorText(error)}`);
            ended(errorText(error));
          },
          complete: () => {
            if (!stopped) ended("the subscription ended");
          },
        });
      return () => {
        stopped = true;
        subscription.unsubscribe();
      };
    },
    async entropy(context) {
      const derived = await client.entropy.derive({
        context: bytesToHex(boundContext(context)),
      });
      if (derived.isErr())
        throw new Error(`entropy: ${errorText(derived.error)}`);
      return hexToBytes(derived.value.entropy);
    },
    async requestAllowance() {
      return (await options.allow()) === null ? "allocated" : "rejected";
    },
    async read(name) {
      const read = await client.localStorage.read({
        key: nodeStorageKey(index, name),
      });
      if (read.isErr()) throw new Error(`storage: ${errorText(read.error)}`);
      return read.value.value === undefined
        ? undefined
        : hexToBytes(read.value.value);
    },
    async write(name, value) {
      const written = await client.localStorage.write({
        key: nodeStorageKey(index, name),
        value: bytesToHex(value),
      });
      if (written.isErr())
        throw new Error(`storage: ${errorText(written.error)}`);
    },
    async remove(name) {
      const cleared = await client.localStorage.clear({
        key: nodeStorageKey(index, name),
      });
      if (cleared.isErr())
        throw new Error(`storage: ${errorText(cleared.error)}`);
    },
  };
  return ports;
}
