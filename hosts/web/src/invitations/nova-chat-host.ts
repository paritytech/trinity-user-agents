import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import type { ContactHandle } from "@parity/truapi";
import type { HandleNames } from "../contacts/contacts.js";
import type { ChatV2ProviderOptions } from "../chat/draft/provider.js";
import {
  NovaBoundary,
  isRefused,
  type PeerId,
  type WorkerFrame,
} from "../chat/nova-chat/boundary.js";
import {
  readNovaCall,
  type NovaCall,
  type NovaHostMessage,
  type NovaPeer,
  type NovaResult,
  type NovaState,
  type OpenedFrame,
  type WireMessage,
  type WireSignedStatement,
  type WireStatement,
} from "../chat/nova-chat/contract.js";
import { claimPeriod } from "../chat/nova-chat/topics.js";
import type { ChatMessage } from "../chat/nova-chat/wire.js";
import type {
  HostNotification,
  NotificationStore,
} from "../notifications/notifications.js";
import type { SentRequest } from "../notifications/screen.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  type SignedStatement,
  type Statement,
} from "../platform/statement.js";
import {
  isStored,
  type StatementStoreLike,
} from "../platform/statement-store.js";
import type { ContactChatHost } from "./contact-chat-host.js";
import type { AcceptPrompt, SendPrompt } from "./invitation-service.js";
import type { ConversationAllowance, Product } from "./ports.js";
import type { IdentityProblem } from "./published-keys.js";

/**
 * PROTOTYPE. Nova Chat's host side for one signed-in user: the small part of
 * the official Nova apps' chat that needs the wallet's keys. It is not the
 * host request service (`host-requests/1`) and shares no wire with it: its
 * requests are the apps' own V2 chat requests, on the acceptor's request
 * topics, and its acceptance is the apps' `DeviceChatAccepted` (kind 20).
 *
 * It reads the user's request topics with its own Statement Store client,
 * opens and checks each request (`../chat/nova-chat/boundary.ts`), and
 * lists it in the Inbox. Nothing leaves without the user: a request only
 * after Send, an acceptance only after Accept. Each is posted once, signed by
 * a notification account the core registers for the day
 * (`//chat-request//<id>//<period>`, `//chat-accept//<id>//<period>`, as the
 * apps derive them). A pending request's acceptance is read on the identity
 * route the same way.
 *
 * Nova Chat's worker gets the rest over `nova-chat/1`
 * (`../chat/nova-chat/contract.ts`): it knows people by contact handle, reads
 * and posts its conversations' statements through its own TrUAPI Statement
 * Store, and asks the host to seal and open each frame. A blocked person's
 * frames are neither sealed nor opened.
 *
 * Supported: the apps' normal path. Not handled: crossed requests, re-invites
 * from an established contact, more than one device per identity, device
 * changes, push, calls, files, and reposting a request or acceptance on a
 * later day.
 */

/** A notification account: the sr25519 key the host derives for one post on one day. */
export interface NotificationSigner {
  public: Uint8Array;
  sign(payload: Uint8Array): Uint8Array;
}

export interface NovaChatHostOptions {
  product: Product;
  /** Where this user's Nova Chat state is kept: peers and pending requests, no secret. */
  storageKey: string;
  boundary: NovaBoundary;
  handles: HandleNames;
  /** The host's name of the person with identity `account`, when it knows them now. */
  nameOf(account: string): { name: string; handle: string } | undefined;
  /** The name the chain gives the identity `account`, or null. */
  lookupName(account: string): Promise<string | null>;
  /** Calls back whenever a name the host draws may have changed, besides the handles. */
  onNamesChange?(listener: () => void): () => void;
  /** The identity accounts the user blocked, lowercase or not. */
  blocked(): readonly string[];
  notifications: NotificationStore;
  /** Why the wallet's keys may not prove this user, or null when they may. */
  verify(): Promise<IdentityProblem | null>;
  openStore(): Promise<StatementStoreLike>;
  /** The notification account of `kind` for `requestId` on `period`. */
  notificationSigner(
    kind: "chat-request" | "chat-accept",
    requestId: string,
    period: number,
  ): NotificationSigner;
  /**
   * Register `account` to post one statement today; answers a line for the
   * log, and the claim's slot for {@link notificationSettled}, or throws why
   * not. The same account on the same day claims nothing new.
   */
  claimNotification(
    account: Uint8Array,
    period: number,
  ): Promise<{
    line: string;
    seq: number;
    collection?: string;
    blockHash?: string;
  }>;
  /**
   * Resolves once `account`'s allowance holds at the People chain's
   * finalized head, or rejects saying why not. A node evicts statements whose
   * allowance is only in a best block. Absent: the claim's answer is enough
   * (the simulated store of the invitation demo).
   */
  notificationSettled?(
    account: Uint8Array,
    claim: {
      period: number;
      seq: number;
      collection?: string;
      blockHash?: string;
    },
  ): Promise<void>;
  /** The allowance of the worker's conversation accounts. */
  accounts: ConversationAllowance;
  confirmSend(prompt: SendPrompt): Promise<boolean>;
  confirmAccept(prompt: AcceptPrompt): Promise<boolean>;
  showChat(): void;
  /** Calls back when the people of {@link ContactChatHost.people} changed. */
  onSenders?(): void;
  log(line: string): void;
  now?(): number;
  /** A new request or acceptance id. Defaults to a random UUID. */
  newId?(): string;
  /** Run `task` after `delayMs`; answers a cancel. Defaults to `setTimeout`. */
  schedule?(task: () => void, delayMs: number): () => void;
}

interface Outgoing {
  peer: PeerId;
  requestId: string;
  welcome: string | null;
  createdAt: number;
  /** Why it is not posted, or null once it is. */
  problem: string | null;
}

interface Saved {
  peers: string;
  outgoing: Outgoing[];
  /** Requests the user declined, `<peer>:<requestId>`: a repost of one is not listed again. */
  declined?: string[];
  /** The conversation account's index per peer: the first the worker named, the only one it may use. */
  indexes?: { [peer: PeerId]: string };
}

const INBOX_PREFIX = "nova-chat:";
const SENT_PREFIX = "nova-chat-sent:";
const NAMING_TIMEOUT_MS = 10_000;
const RETRY_MS = 30_000;
/** How often a request that was not posted is tried again, at most, before it waits for the user. */
const POST_ATTEMPTS = 5;
const DECLINED_MAX = 500;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const toWire = (statement: Statement): WireStatement => ({
  expiry: statement.expiry.toString(),
  channel: statement.channel === null ? null : bytesToHex(statement.channel),
  topics: statement.topics.map((topic) => bytesToHex(topic)),
  data: bytesToHex(statement.data),
});

const fromWire = (statement: WireSignedStatement): SignedStatement => ({
  expiry: BigInt(statement.expiry),
  channel:
    statement.channel === null
      ? null
      : hexToBytes(statement.channel as HexString),
  topics: statement.topics.map((topic) => hexToBytes(topic as HexString)),
  data: hexToBytes(statement.data as HexString),
  signer: hexToBytes(statement.signer as HexString),
  signature: hexToBytes(statement.signature as HexString),
});

function wireMessage(message: ChatMessage): WireMessage {
  const content = message.content;
  return {
    id: message.id,
    timestamp: message.timestamp.toString(),
    content:
      content.kind === "DeviceChatAccepted"
        ? { kind: "Unsupported", index: 20 }
        : content,
  };
}

const wireFrame = (frame: WorkerFrame): OpenedFrame =>
  frame.kind === "Response"
    ? frame
    : {
        kind: "Request",
        requestId: frame.requestId,
        messages: frame.messages.map(wireMessage),
      };

export function startNovaChatHost(
  options: NovaChatHostOptions,
): ContactChatHost {
  const now = options.now ?? (() => Date.now());
  const newId = options.newId ?? (() => crypto.randomUUID());
  const { boundary, handles, product } = options;
  let state: "checking" | "running" | "off" | "stopped" = "checking";
  let problem: IdentityProblem | null = null;
  let store: StatementStoreLike | null = null;
  let port: MessagePort | null = null;
  let pendingPort: MessagePort | null = null;
  let stopRequests = () => {};
  const acceptanceWatches = new Map<PeerId, () => void>();
  let cancelRetry = () => {};
  const sentListeners = new Set<() => void>();
  const startListeners = new Set<() => void>();
  let outgoing: Outgoing[] = [];
  let declined: string[] = [];
  const schedule =
    options.schedule ??
    ((task: () => void, delayMs: number) => {
      const timer = setTimeout(task, delayMs);
      return () => clearTimeout(timer);
    });
  const retries = new Map<PeerId, () => void>();
  /** Posts run one at a time, so two never race for one notification slot. */
  let posting: Promise<unknown> = Promise.resolve();
  let indexes: { [peer: PeerId]: string } = {};

  // Every effect after an await checks the host still runs: a host stopped by
  // a sign-out or a wallet switch must not write over the next one's state
  // under the same key, nor list a request in the next wallet's Inbox.
  const save = () =>
    state !== "stopped" &&
    localStorage.setItem(
      options.storageKey,
      JSON.stringify({
        peers: boundary.snapshot(),
        outgoing,
        declined,
        indexes,
      } satisfies Saved),
    );
  try {
    const saved = JSON.parse(
      localStorage.getItem(options.storageKey) ?? "null",
    ) as Saved | null;
    if (saved !== null) {
      boundary.restore(saved.peers);
      outgoing = saved.outgoing.filter(
        (item) => boundary.state(item.peer) === "outgoing",
      );
      indexes = saved.indexes ?? {};
      declined = saved.declined ?? [];
    }
  } catch {
    options.log(
      `${product.name}: its saved state does not read; it starts empty.`,
    );
  }

  const blocked = (peer: PeerId) =>
    options.blocked().some((account) => account.toLowerCase() === peer);
  const sentChanged = () => {
    for (const listener of [...sentListeners]) listener();
  };
  const startChanged = () => {
    for (const listener of [...startListeners]) listener();
  };
  let peopleCount = -1;
  const people = () => boundary.known();
  const peopleChanged = () => {
    const count = people().length;
    if (count === peopleCount) return;
    peopleCount = count;
    options.onSenders?.();
  };

  const post = (message: NovaHostMessage) => port?.postMessage(message);
  const stateOf = (peer: PeerId): NovaState => {
    if (blocked(peer)) return "blocked";
    return boundary.state(peer) ?? "gone";
  };
  const tell = (peer: PeerId) => {
    const handle = handles.handleFor(peer);
    if (handle === null) return;
    post({
      kind: "state",
      peer: { handle: handle.toLowerCase(), state: stateOf(peer) },
    });
  };

  /**
   * Sign `statement` with the notification account of `kind` for
   * `requestId` and today, claimed for today, and submit it once its
   * allowance is final. The account is the same for every try of one request
   * on one day (the apps' `//chat-request//<id>//<period>`,
   * `//chat-accept//<id>//<period>`, with the peer's request id for an
   * acceptance), so a try again claims no new slot. Posts run one at a time.
   */
  const postAs = (
    kind: "chat-request" | "chat-accept",
    requestId: string,
    statement: Statement,
  ): Promise<void> => {
    const run = async () => {
      if (store === null) throw new Error("the Statement Store is not open");
      const period = claimPeriod(now());
      const signer = options.notificationSigner(kind, requestId, period);
      const claim = await options.claimNotification(signer.public, period);
      options.log(
        `${product.name}: ${kind} ${requestId.slice(0, 8)}… ${claim.line}`,
      );
      await options.notificationSettled?.(signer.public, { period, ...claim });
      if (state !== "running") throw new Error(`${product.name} stopped`);
      const result = await store.submit(
        encodeSigned(statement, signer.public, signer.sign),
      );
      if (!isStored(result))
        throw new Error(
          `the Statement Store answered ${result.status}${result.reason === undefined ? "" : ` (${result.reason})`}`,
        );
    };
    const next = posting.then(run, run);
    posting = next.catch(() => {});
    return next;
  };

  // ---- Incoming requests ----------------------------------------------------------

  const inboxId = (peer: PeerId, requestId: string) =>
    `${INBOX_PREFIX}${peer}:${requestId}`;
  const requestOfItem = (id: string): string | null =>
    peerOfItem(id) === null
      ? null
      : id.slice(INBOX_PREFIX.length + 66 + 1) || null;
  const peerOfItem = (id: string): PeerId | null => {
    if (!id.startsWith(INBOX_PREFIX)) return null;
    const peer = id.slice(INBOX_PREFIX.length).split(":")[0] ?? "";
    return /^0x[0-9a-f]{64}$/.test(peer) ? peer : null;
  };

  const heardRequest = async (encoded: Uint8Array) => {
    let statement: SignedStatement;
    try {
      statement = decodeStatement(encoded);
    } catch {
      return;
    }
    const opened = await boundary.openRequest(statement);
    if (state !== "running") return;
    if (isRefused(opened)) {
      if (opened.refused !== "not for us")
        options.log(
          `${product.name}: a request was refused: ${opened.refused}.`,
        );
      return;
    }
    if (
      blocked(opened.peer) ||
      declined.includes(`${opened.peer}:${opened.requestId}`)
    ) {
      boundary.decline(opened.peer);
      save();
      return;
    }
    save();
    peopleChanged();
    if (opened.duplicate) return;
    const name =
      options.nameOf(opened.peer)?.name ??
      (await options.lookupName(opened.peer)) ??
      "Someone";
    if (state !== "running") return;
    options.notifications.add({
      id: inboxId(opened.peer, opened.requestId),
      kind: "invitation",
      sender: { name, handle: name },
      product: { id: product.id, name: product.name },
      purpose: opened.welcome ?? "Wants to chat with you.",
      receivedAt: now(),
      expiresAt: null,
      seenAt: null,
      state: { tag: "pending" },
    });
    options.log(`${product.name}: a chat request arrived from ${name}.`);
    tell(opened.peer);
  };

  /** The user declined a request in the Inbox: forget it here, and do not list its reposts again. */
  const stopAnswers = options.notifications.onAnswer((item) => {
    const peer = peerOfItem(item.id);
    const requestId = requestOfItem(item.id);
    if (
      state === "stopped" ||
      peer === null ||
      requestId === null ||
      item.state.tag !== "declined"
    )
      return;
    if (boundary.state(peer) === "incoming") boundary.decline(peer);
    declined = [...declined, `${peer}:${requestId}`].slice(-DECLINED_MAX);
    save();
    tell(peer);
  });

  // ---- Our requests' acceptances -------------------------------------------------------

  const watchAcceptance = (peer: PeerId) => {
    const topic = boundary.acceptanceTopic(peer);
    if (store === null || topic === null || acceptanceWatches.has(peer)) return;
    let stop = () => {};
    let stopped = false;
    acceptanceWatches.set(peer, () => {
      stopped = true;
      stop();
    });
    store
      .subscribe({ matchAll: [topic] }, (statements) => {
        for (const encoded of statements) {
          let statement: SignedStatement;
          try {
            statement = decodeStatement(encoded);
          } catch {
            continue;
          }
          if (state !== "running" || blocked(peer)) return;
          const result = boundary.openAcceptance(peer, statement);
          if (isRefused(result)) continue;
          options.log(`${product.name}: your request was accepted.`);
          outgoing = outgoing.filter((item) => item.peer !== peer);
          save();
          acceptanceWatches.get(peer)?.();
          acceptanceWatches.delete(peer);
          sentChanged();
          tell(peer);
          return;
        }
      })
      .then(
        (unsubscribe) => (stopped ? unsubscribe() : (stop = unsubscribe)),
        (error: unknown) =>
          options.log(
            `${product.name}: the acceptance of a request cannot be read: ${errorText(error)}`,
          ),
      );
  };

  // ---- The worker's calls ------------------------------------------------------------

  const failure = (reason: string): NovaResult => ({ ok: false, reason });
  /** A refusal that trying again does not change. */
  const refusal = (reason: string): NovaResult => ({
    ok: false,
    reason,
    final: true,
  });
  const peerOf = (handle: string): PeerId | null =>
    handles.accountFor(handle)?.toLowerCase() ?? null;

  const request = async (
    handle: string,
    welcome: string | null,
  ): Promise<NovaResult> => {
    const peer = peerOf(handle);
    if (peer === null) return failure("The host cannot name this contact.");
    if (blocked(peer)) return failure("You blocked this person.");
    const name =
      options.nameOf(peer)?.name ??
      (await options.lookupName(peer)) ??
      "Contact";
    if (state !== "running") return refusal(`${product.name} stopped.`);
    const sent = await options.confirmSend({
      product,
      to: { name, handle: name },
      purpose: welcome ?? "Wants to chat with you.",
    });
    if (!sent) return failure("You did not send the request.");
    if (state !== "running") return refusal(`${product.name} stopped.`);
    if (blocked(peer)) return refusal("You blocked this person.");
    const requestId = newId();
    const built = await boundary.request(hexToBytes(peer as HexString), {
      requestId,
      nowMs: now(),
      expiry: expiryAt(now()),
      welcome,
    });
    if (state !== "running") return refusal(`${product.name} stopped.`);
    if (isRefused(built)) return failure(`Not sent: ${built.refused}.`);
    const item: Outgoing = {
      peer,
      requestId,
      welcome,
      createdAt: now(),
      problem: "posting",
    };
    outgoing = [...outgoing.filter((each) => each.peer !== peer), item];
    save();
    peopleChanged();
    sentChanged();
    watchAcceptance(peer);
    tell(peer);
    await postRequest(item, POST_ATTEMPTS);
    return item.problem === null ? { ok: true } : failure(item.problem);
  };

  /**
   * Post `item`'s request, built again with the same id, so a try again on
   * the same day signs with the same notification account and claims no new
   * slot. A failed try is tried again after {@link RETRY_MS} while `attempts`
   * last; then it waits, paused, until the host starts again.
   */
  async function postRequest(item: Outgoing, attempts: number): Promise<void> {
    retries.get(item.peer)?.();
    retries.delete(item.peer);
    if (state !== "running" || !outgoing.includes(item)) return;
    if (blocked(item.peer)) {
      item.problem = "You blocked this person.";
      save();
      return sentChanged();
    }
    try {
      const built = await boundary.request(hexToBytes(item.peer as HexString), {
        requestId: item.requestId,
        nowMs: now(),
        expiry: expiryAt(now()),
        welcome: item.welcome,
      });
      if (isRefused(built)) throw new Error(built.refused);
      await postAs("chat-request", item.requestId, built.statement);
      if (state !== "running") return;
      item.problem = null;
      options.log(`${product.name}: your request is posted.`);
    } catch (error) {
      if (state !== "running") return;
      const again = attempts > 1 && outgoing.includes(item);
      item.problem = again
        ? `Not posted yet, trying again in ${RETRY_MS / 1000} s: ${errorText(error)}`
        : `Not posted: ${errorText(error)}`;
      options.log(`${product.name}: ${item.problem}`);
      if (again)
        retries.set(
          item.peer,
          schedule(() => void postRequest(item, attempts - 1), RETRY_MS),
        );
    }
    save();
    sentChanged();
  }

  const call = async (message: NovaCall): Promise<NovaResult> => {
    if (message.call === "list") {
      const listed: NovaPeer[] = [];
      for (const peer of boundary.known()) {
        const handle = handles.handleFor(peer);
        if (handle !== null)
          listed.push({ handle: handle.toLowerCase(), state: stateOf(peer) });
      }
      return { ok: true, value: listed };
    }
    if (message.call === "request")
      return request(message.handle, message.welcome);
    const peer = peerOf(message.handle);
    if (peer === null) return refusal("The host cannot name this contact.");
    if (blocked(peer)) return refusal("You blocked this person.");
    switch (message.call) {
      case "routes": {
        const routes = boundary.routes(peer);
        return isRefused(routes)
          ? refusal(routes.refused)
          : {
              ok: true,
              value: {
                outgoing: bytesToHex(routes.outgoing),
                incoming: routes.incoming.map((topic) => bytesToHex(topic)),
              },
            };
      }
      case "seal": {
        const frame =
          message.frame.kind === "Request"
            ? {
                kind: "Request" as const,
                requestId: message.frame.requestId,
                messages: message.frame.messages.map((each) =>
                  hexToBytes(each as HexString),
                ),
              }
            : message.frame;
        const sealed = boundary.sealFrame(peer, frame, BigInt(message.expiry));
        return isRefused(sealed)
          ? refusal(sealed.refused)
          : { ok: true, value: toWire(sealed) };
      }
      case "open": {
        const opened = boundary.openFrame(peer, fromWire(message.statement));
        return isRefused(opened)
          ? failure(opened.refused)
          : { ok: true, value: wireFrame(opened) };
      }
      case "conversationAccount": {
        if (boundary.state(peer) !== "established")
          return refusal("Not an accepted contact.");
        const bound = indexes[peer];
        if (bound !== undefined && bound !== message.index)
          return refusal("This chat already has another account.");
        if (bound === undefined) {
          indexes[peer] = message.index;
          save();
        }
        try {
          const allowed = await options.accounts.allow(
            product.id,
            hexToBytes(message.index as HexString),
          );
          if (state !== "running") return refusal(`${product.name} stopped.`);
          return { ok: true, value: allowed };
        } catch (error) {
          // The core's claim errors reach the host as text, so a missing
          // personhood, no free slot and an unreachable chain are told apart
          // only by the reason: each may pass later.
          return failure(errorText(error));
        }
      }
      default:
        return failure("Unknown call.");
    }
  };

  const serve = (next: MessagePort) => {
    port?.close();
    port = next;
    next.onmessage = (event: MessageEvent<unknown>) => {
      const message = readNovaCall(event.data);
      if (message === null) {
        const id = (event.data as { id?: unknown } | null)?.id;
        if (typeof id === "number")
          next.postMessage({
            kind: "result",
            id,
            result: failure("The host does not read this call."),
          } satisfies NovaHostMessage);
        return;
      }
      void call(message).then(
        (result) =>
          next.postMessage({
            kind: "result",
            id: message.id,
            result,
          } satisfies NovaHostMessage),
        (error: unknown) =>
          next.postMessage({
            kind: "result",
            id: message.id,
            result: failure(errorText(error)),
          } satisfies NovaHostMessage),
      );
    };
    next.start();
  };

  // ---- Lifecycle ------------------------------------------------------------------------

  const openReading = async () => {
    try {
      store = await options.openStore();
    } catch (error) {
      if (state !== "running") return;
      options.log(
        `${product.name}: the Statement Store cannot be opened, trying again in ${RETRY_MS / 1000} s: ${errorText(error)}`,
      );
      const timer = setTimeout(() => void openReading(), RETRY_MS);
      cancelRetry = () => clearTimeout(timer);
      return;
    }
    if (state !== "running") return store.close();
    const unsubscribe = await store.subscribe(
      { matchAny: boundary.requestTopics(now()) },
      (statements) => {
        for (const encoded of statements) void heardRequest(encoded);
      },
    );
    if (state !== "running") return unsubscribe();
    stopRequests = unsubscribe;
    for (const item of outgoing) {
      watchAcceptance(item.peer);
      if (item.problem !== null) void postRequest(item, POST_ATTEMPTS);
    }
  };

  const check = async (): Promise<void> => {
    const found = await options.verify();
    if (state !== "checking") return;
    problem = found;
    if (found?.again === true) {
      options.log(`${product.name} waits: ${found.reason}`);
      const timer = setTimeout(() => void check(), RETRY_MS);
      cancelRetry = () => clearTimeout(timer);
    } else if (found !== null) {
      options.log(`${product.name} stays off: ${found.reason}`);
      state = "off";
      pendingPort?.close();
      pendingPort = null;
    } else {
      options.log(
        `${product.name} is on: Nova chat requests to and from this identity are read.`,
      );
      state = "running";
      peopleChanged();
      if (pendingPort !== null) serve(pendingPort);
      pendingPort = null;
      await openReading();
    }
    startChanged();
  };

  const stop = () => {
    if (state === "stopped") return;
    state = "stopped";
    for (const cancel of retries.values()) cancel();
    retries.clear();
    stopAnswers();
    cancelRetry();
    stopRequests();
    for (const unwatch of acceptanceWatches.values()) unwatch();
    acceptanceWatches.clear();
    store?.close();
    store = null;
    port?.close();
    port = null;
    pendingPort?.close();
    pendingPort = null;
  };

  const owns = (id: string) => {
    const peer = peerOfItem(id);
    return peer !== null && options.notifications.get(id) !== undefined;
  };

  const accept = async (item: HostNotification): Promise<void> => {
    const peer = peerOfItem(item.id);
    if (peer === null || boundary.state(peer) !== "incoming")
      throw new Error("This request is no longer pending.");
    if (blocked(peer)) throw new Error("You blocked this person.");
    const agreed = await options.confirmAccept({
      product,
      from: item.sender,
      purpose: item.purpose,
    });
    if (!agreed) throw new Error("You did not accept.");
    if (state !== "running") throw new Error(`${product.name} stopped.`);
    if (blocked(peer)) throw new Error("You blocked this person.");
    const theirs = requestOfItem(item.id);
    if (theirs === null) throw new Error("This request is no longer pending.");
    const statement = boundary.accept(peer, {
      messageId: newId(),
      nowMs: now(),
      expiry: expiryAt(now()),
    });
    if (isRefused(statement)) throw new Error(statement.refused);
    try {
      await postAs("chat-accept", theirs, statement);
    } catch (error) {
      // Not posted: the request stays pending, and Accept may be tried again.
      throw new Error(`The acceptance was not posted: ${errorText(error)}`);
    }
    if (state !== "running")
      throw new Error(
        `${product.name} stopped before the acceptance was kept.`,
      );
    boundary.accepted(peer);
    save();
    options.notifications.accept(item.id);
    options.log(`${product.name}: you accepted ${item.sender.name}'s request.`);
    tell(peer);
  };

  const handleOnceNamed = (account: string) =>
    new Promise<ContactHandle | null>((resolve) => {
      const done = (value: ContactHandle | null) => {
        clearTimeout(timer);
        unsubscribe();
        resolve(value);
      };
      const look = () => {
        const handle = handles.handleFor(account);
        if (handle !== null) done({ bytes: handle } as ContactHandle);
      };
      const unsubscribe = handles.onChange(look);
      const timer = setTimeout(() => done(null), NAMING_TIMEOUT_MS);
      look();
    });
  const starting = new Set<string>();

  const chatOptions: ChatV2ProviderOptions = {
    contacts: {
      resolve: (handle) => {
        const account = handles.accountFor(handle.bytes);
        const person = account === null ? undefined : options.nameOf(account);
        return account === null || person === undefined
          ? null
          : { account, name: person.name, seed: person.handle };
      },
      handleFor: (account) => {
        const handle = handles.handleFor(account);
        return handle === null ? null : ({ bytes: handle } as ContactHandle);
      },
    },
    onNamesChange: (listener) => {
      const stops = [
        handles.onChange(listener),
        options.onNamesChange?.(listener) ?? (() => {}),
      ];
      return () => stops.forEach((each) => each());
    },
    start: {
      target: async (account) => {
        starting.add(account.toLowerCase());
        options.onSenders?.();
        const handle = await handleOnceNamed(account);
        if (handle === null)
          throw new Error(`${product.name} cannot reach this person.`);
        return { handle };
      },
      blocked: () => {
        if (state === "running") return null;
        if (state === "checking")
          return `${product.name} is checking your identity on chain.`;
        return `${product.name} is off${problem === null ? "." : `: ${problem.reason}`}`;
      },
      onChange: (listener) => {
        startListeners.add(listener);
        return () => startListeners.delete(listener);
      },
    },
  };

  return {
    product,
    chatOptions,
    attachChat: (next) => {
      if (state === "running") serve(next);
      else if (state === "checking") pendingPort = next;
      else next.close();
    },
    detachChat: () => {
      pendingPort = null;
      port?.close();
      port = null;
    },
    owns,
    accept,
    open: (item) => {
      if (item.state.tag === "accepted") options.showChat();
    },
    people: () => [...new Set([...people(), ...starting])],
    sent: (): SentRequest[] =>
      outgoing
        .map((item) => ({
          id: `${SENT_PREFIX}${item.peer}`,
          to: options.nameOf(item.peer) ?? {
            name: "Contact",
            handle: item.peer,
          },
          product,
          purpose: item.welcome ?? "Wants to chat with you.",
          createdAt: item.createdAt,
          paused: item.problem,
        }))
        .sort((left, right) => right.createdAt - left.createdAt),
    cancel: (id) => {
      if (!id.startsWith(SENT_PREFIX)) return false;
      const peer = id.slice(SENT_PREFIX.length);
      if (!boundary.withdraw(peer)) return false;
      outgoing = outgoing.filter((item) => item.peer !== peer);
      acceptanceWatches.get(peer)?.();
      acceptanceWatches.delete(peer);
      save();
      sentChanged();
      tell(peer);
      return true;
    },
    onSentChange: (listener) => {
      sentListeners.add(listener);
      return () => sentListeners.delete(listener);
    },
    start: check,
    stop,
  };
}
