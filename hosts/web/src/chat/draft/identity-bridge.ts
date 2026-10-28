import type { NovaIdentity } from "../nova/identity.js";

/**
 * DRAFT, NOT TRUAPI. The page-local bridge over which the host lends one
 * chat product worker the operations of its Nova chat identity
 * (`../nova/identity.ts`), without handing it a key.
 *
 * The product frame posts {@link NovaIdentityConnect} to its parent with one
 * `MessagePort`. The host answers on that port only while the user has
 * switched chat to that worker (`tools/demos/worker-mode.ts`); otherwise every call is
 * refused and the port is closed. The core sees none of this and grants
 * nothing through it.
 *
 * Values cross as structured clones: bytes as `Uint8Array`, expiries as
 * `bigint`. The host checks every value's type and length before it acts.
 */

/** Version of the bridge. Bumped on any change to its messages. */
export const NOVA_IDENTITY_DRAFT = "truapi-nova-identity-draft/0";
/** `type` of the window message a product frame sends to hand the host its port. */
export const NOVA_IDENTITY_CONNECT = "truapi-nova-identity-draft";

export interface NovaIdentityConnect {
  type: typeof NOVA_IDENTITY_CONNECT;
  protocol: typeof NOVA_IDENTITY_DRAFT;
}

/** The calls a product makes, with their request and answer. */
export interface IdentityCalls {
  hello: [
    Record<string, never>,
    { identity: Uint8Array; chatPublic: Uint8Array },
  ];
  openRequest: [{ data: Uint8Array }, Uint8Array];
  identityProofMatches: [
    {
      peerChatKey: Uint8Array;
      identity: Uint8Array;
      signer: Uint8Array;
      proof: Uint8Array;
    },
    boolean,
  ];
  sessionTopics: [
    { peer: Uint8Array; peerChatKey: Uint8Array },
    { incoming: Uint8Array; outgoing: Uint8Array },
  ];
  openSession: [{ peerChatKey: Uint8Array; sealed: Uint8Array }, Uint8Array];
  sessionStatement: [
    {
      peer: Uint8Array;
      peerChatKey: Uint8Array;
      plaintext: Uint8Array;
      expiry: bigint;
    },
    Uint8Array,
  ];
  requestStatement: [
    {
      peer: Uint8Array;
      peerChatKey: Uint8Array;
      id: string;
      timestampMs: bigint;
      welcome: string | null;
      expiry: bigint;
      day: bigint;
    },
    { statement: Uint8Array; plaintext: Uint8Array },
  ];
  /**
   * The host's own chat record, as JSON, when the user asked to copy it to
   * this worker; null otherwise. The host hands it out once per grant and
   * keeps its own copy.
   */
  takeHistory: [Record<string, never>, string | null];
  /**
   * The account behind a reference the host made when the user chose someone
   * to start a chat with (`start_conversation`). The Nova wire addresses,
   * seals and signs a request to that account, so the engine needs it. A
   * reference resolves once, in the grant it was made in.
   */
  resolveContact: [{ reference: string }, { account: Uint8Array }];
}

export type IdentityMethod = keyof IdentityCalls;

export type IdentityEnvelope =
  | { kind: "call"; id: number; method: IdentityMethod; request: unknown }
  | { kind: "result"; id: number; ok: true; value: unknown }
  | { kind: "result"; id: number; ok: false; reason: string };

/** One end of the bridge. */
export interface IdentityPort {
  post(message: IdentityEnvelope): void;
  listen(handler: (message: IdentityEnvelope) => void): void;
  close(): void;
}

export function identityPortOver(port: MessagePort): IdentityPort {
  return {
    post: (message) => port.postMessage(message),
    listen: (handler) => {
      port.onmessage = (event: MessageEvent<IdentityEnvelope>) => {
        if (typeof event.data === "object" && event.data !== null)
          handler(event.data);
      };
      port.start();
    },
    close: () => port.close(),
  };
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

// --- Host end -----------------------------------------------------------------

function field(request: unknown, name: string): unknown {
  return typeof request === "object" && request !== null
    ? (request as Record<string, unknown>)[name]
    : undefined;
}

function bytes(request: unknown, name: string, length?: number): Uint8Array {
  const value = field(request, name);
  if (!(value instanceof Uint8Array)) throw new Error(`${name} must be bytes.`);
  if (length !== undefined && value.length !== length)
    throw new Error(`${name} must be ${length} bytes.`);
  return value;
}

function big(request: unknown, name: string): bigint {
  const value = field(request, name);
  if (typeof value !== "bigint" || value < 0n || value >= 2n ** 64n)
    throw new Error(`${name} must be a u64.`);
  return value;
}

function textOf(request: unknown, name: string): string {
  const value = field(request, name);
  if (typeof value !== "string") throw new Error(`${name} must be text.`);
  return value;
}

export interface IdentityServiceDeps {
  identity: NovaIdentity;
  /** Resolve a start reference to its account; throws when it names no one now. */
  resolveContact?(reference: string): Uint8Array;
  /** The host's chat record to copy, once, when the user asked for it. */
  history?: () => string | null;
  /** Told of each call the service answered, by method, for the developer view. */
  onCall?: (method: IdentityMethod, ok: boolean) => void;
}

/** A running service, until {@link close}. */
export interface IdentityService {
  close(): void;
}

/**
 * Answer the product's calls on `port` with `identity`. Every request is
 * checked here; `identity` builds what it signs. {@link IdentityService.close}
 * ends the service: the port is closed and an answer still on its way is
 * dropped.
 */
export function serveNovaIdentity(
  port: IdentityPort,
  deps: IdentityServiceDeps,
): IdentityService {
  const { identity } = deps;
  let open = true;
  let historyTaken = false;
  const run = (method: IdentityMethod, request: unknown): Promise<unknown> => {
    switch (method) {
      case "hello":
        return Promise.resolve({
          identity: identity.identity,
          chatPublic: identity.chatPublic,
        });
      case "openRequest":
        return identity.openRequest(bytes(request, "data"));
      case "identityProofMatches":
        return identity.identityProofMatches(
          bytes(request, "peerChatKey", 32),
          {
            identity: bytes(request, "identity", 32),
            signer: bytes(request, "signer", 32),
            proof: bytes(request, "proof", 32),
          },
        );
      case "sessionTopics":
        return identity.sessionTopics(
          bytes(request, "peer", 32),
          bytes(request, "peerChatKey", 32),
        );
      case "openSession":
        return identity.openSession(
          bytes(request, "peerChatKey", 32),
          bytes(request, "sealed"),
        );
      case "sessionStatement":
        return identity.sessionStatement({
          peer: bytes(request, "peer", 32),
          peerChatKey: bytes(request, "peerChatKey", 32),
          plaintext: bytes(request, "plaintext"),
          expiry: big(request, "expiry"),
        });
      case "requestStatement": {
        const welcome = field(request, "welcome");
        if (welcome !== null && typeof welcome !== "string")
          throw new Error("welcome must be text or null.");
        return identity.requestStatement({
          peer: bytes(request, "peer", 32),
          peerChatKey: bytes(request, "peerChatKey", 32),
          id: textOf(request, "id"),
          timestampMs: big(request, "timestampMs"),
          welcome,
          expiry: big(request, "expiry"),
          day: big(request, "day"),
        });
      }
      case "resolveContact": {
        if (deps.resolveContact === undefined)
          throw new Error("This host made no start reference.");
        return Promise.resolve({
          account: deps.resolveContact(textOf(request, "reference")),
        });
      }
      case "takeHistory": {
        if (historyTaken) return Promise.resolve(null);
        historyTaken = true;
        return Promise.resolve(deps.history?.() ?? null);
      }
      default:
        throw new Error(`There is no method ${String(method)}.`);
    }
  };
  port.listen((message) => {
    if (!open || message.kind !== "call") return;
    const { id, method } = message;
    void (async () => run(method, message.request))()
      .then(
        (value) => ({ kind: "result" as const, id, ok: true as const, value }),
        (error: unknown) => ({
          kind: "result" as const,
          id,
          ok: false as const,
          reason: errorText(error),
        }),
      )
      .then((result) => {
        if (!open) return;
        deps.onCall?.(method, result.ok);
        port.post(result);
      });
  });
  return {
    close() {
      if (!open) return;
      open = false;
      port.close();
    },
  };
}

/** Answer every call on `port` with `reason`, then close it: the frame has no grant. */
export function refuseNovaIdentity(port: IdentityPort, reason: string): void {
  port.listen((message) => {
    if (message.kind !== "call") return;
    port.post({ kind: "result", id: message.id, ok: false, reason });
    port.close();
  });
}

// --- Product end --------------------------------------------------------------

/** How long the product waits for one answer before it gives up. */
const CALL_TIMEOUT_MS = 15_000;

/** The product's view of the bridge. */
export interface NovaIdentityClient {
  identity: NovaIdentity;
  /** The host's chat record, when the user asked to copy it; see {@link IdentityCalls.takeHistory}. */
  takeHistory(): Promise<string | null>;
  /** See {@link IdentityCalls.resolveContact}. */
  resolveContact(reference: string): Promise<Uint8Array>;
  close(): void;
}

/**
 * Connect over `port` and ask for the identity's public keys. Rejects when
 * the host refuses: the user has not switched chat to this worker.
 */
export async function connectNovaIdentity(
  port: IdentityPort,
  timeoutMs = CALL_TIMEOUT_MS,
): Promise<NovaIdentityClient> {
  let nextId = 1;
  let closed = false;
  const waiting = new Map<
    number,
    { resolve(value: unknown): void; reject(error: Error): void }
  >();
  port.listen((message) => {
    if (message.kind !== "result") return;
    const call = waiting.get(message.id);
    waiting.delete(message.id);
    if (message.ok) call?.resolve(message.value);
    else call?.reject(new Error(message.reason));
  });
  const call = <M extends IdentityMethod>(
    method: M,
    request: IdentityCalls[M][0],
  ): Promise<IdentityCalls[M][1]> => {
    if (closed)
      return Promise.reject(new Error("The chat identity is disconnected."));
    const id = nextId++;
    return new Promise<IdentityCalls[M][1]>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (waiting.delete(id))
          reject(new Error("The host did not answer for the chat identity."));
      }, timeoutMs);
      waiting.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value as IdentityCalls[M][1]);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      port.post({ kind: "call", id, method, request });
    });
  };
  const keys = await call("hello", {});
  return {
    identity: {
      identity: keys.identity,
      chatPublic: keys.chatPublic,
      openRequest: (data) => call("openRequest", { data }),
      identityProofMatches: (peerChatKey, proof) =>
        call("identityProofMatches", { peerChatKey, ...proof }),
      sessionTopics: (peer, peerChatKey) =>
        call("sessionTopics", { peer, peerChatKey }),
      openSession: (peerChatKey, sealed) =>
        call("openSession", { peerChatKey, sealed }),
      sessionStatement: (request) => call("sessionStatement", request),
      requestStatement: (request) => call("requestStatement", request),
    },
    takeHistory: () => call("takeHistory", {}),
    resolveContact: async (reference) =>
      (await call("resolveContact", { reference })).account,
    close() {
      closed = true;
      port.close();
      for (const pending of waiting.values())
        pending.reject(new Error("The chat identity is disconnected."));
      waiting.clear();
    },
  };
}

/** Hand the host this frame's end of the bridge, and connect over it. */
export function requestNovaIdentity(
  parent: Window = window.parent,
  hostOrigin: string = location.origin,
): Promise<NovaIdentityClient> {
  const channel = new MessageChannel();
  parent.postMessage(
    {
      type: NOVA_IDENTITY_CONNECT,
      protocol: NOVA_IDENTITY_DRAFT,
    } satisfies NovaIdentityConnect,
    hostOrigin,
    [channel.port2],
  );
  return connectNovaIdentity(identityPortOver(channel.port1));
}
