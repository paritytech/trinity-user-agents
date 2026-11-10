import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import type { ContactHandle } from "@parity/truapi";
import type { HandleNames } from "../../contacts/contacts.js";
import type { ChatV2ProviderOptions } from "../../chat/draft/provider.js";
import {
  NovaBoundary,
  isRefused,
  type PeerId,
  type WorkerFrame,
} from "./boundary.js";
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
} from "../../../bridges/nova-chat/contract.js";
import { claimPeriod } from "./topics.js";
import type { ChatMessage } from "../../../bridges/nova-chat/wire.js";
import type {
  HostNotification,
  NotificationStore,
} from "../../notifications/notifications.js";
import type { SentRequest } from "../../notifications/screen.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  type SignedStatement,
  type Statement,
} from "../../../bridges/primitives/statement.js";
import {
  isStored,
  type StatementStoreLike,
} from "../../platform/statement-store.js";
import type { AllowanceGuard } from "../../platform/allowance-usable.js";
import { FINALITY_TIMEOUT_MS } from "../../platform/allowance-finality.js";
import { abortion } from "../../platform/people-chain.js";
import {
  postWhenUsable,
  submitUntilAllowed,
  type FinalOutcome,
} from "../../platform/guarded-post.js";
import type { ContactChatHost } from "../../invitations/contact-chat-host.js";
import type {
  AcceptPrompt,
  SendPrompt,
} from "../../invitations/invitation-service.js";
import type {
  ConversationAllowance,
  Product,
} from "../../invitations/ports.js";
import type { IdentityProblem } from "../../invitations/published-keys.js";

/**
 * PROTOTYPE. Nova Chat's host side for one signed-in user: the small part of
 * the official Nova apps' chat that needs the wallet's keys. It is not the
 * host request service (`host-requests/1`) and shares no wire with it: its
 * requests are the apps' own V2 chat requests, on the acceptor's request
 * topics, and its acceptance is the apps' `DeviceChatAccepted` (kind 20).
 *
 * It reads the user's request topics with its own Statement Store client,
 * opens and checks each request (`./nova/boundary.ts`), and
 * lists it in the Inbox. Nothing leaves without the user: a request only
 * after Send, an acceptance only after Accept. Each is posted once, signed by
 * a notification account the core registers for the day
 * (`//chat-request//<id>//<period>`, `//chat-accept//<id>//<period>`, as the
 * apps derive them), once that account may post by the network's rule
 * (`../platform/guarded-post.ts`). A pending request's acceptance is read on
 * the identity route the same way.
 *
 * Nova Chat's worker gets the rest over `nova-chat/1`
 * (`bridges/nova-chat/contract.ts`): it knows people by contact handle, reads
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
   * log, and the claim's slot for {@link notificationGuard}, or throws why
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
   * Resolves once a claimed notification account may post, by the
   * network's rule: at a best block after its claim, or at the finalized
   * head; rejects saying why not.
   */
  notificationGuard: AllowanceGuard;
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
  /** Why it is not posted, or null once it is. A request not posted is posted again at the next start. */
  problem: string | null;
  attemptsLeft?: number;
}

/**
 * A Send the user agreed to in New chat before the product was asked, for
 * one request to one person with this welcome. The product's `request`
 * spends it. It is live until its start ended; a request that spends it is
 * kept only while it is.
 */
interface Agreed {
  welcome: string | null;
  live: boolean;
}

interface Saved {
  peers: string;
  outgoing: Outgoing[];
  /** Requests the user declined, `<peer>:<requestId>`: a repost of one is not listed again. */
  declined?: string[];
  /** The conversation account's index per peer: the first the worker named, the only one it may use. */
  indexes?: { [peer: PeerId]: string };
  posts?: Record<string, CachedPost>;
  acceptances?: Record<string, { peer: PeerId; requestId: string }>;
}

interface CachedPost {
  period: number;
  bytes: string;
  peer?: PeerId;
  claim?: { seq: number; collection?: string; blockHash?: string };
  needsFinal?: boolean;
  renewed?: boolean;
  /** Only explicitly provisional submissions are checked again after restart. */
  confirmation?: { until: number; submitted: boolean };
}

const INBOX_PREFIX = "nova-chat:";
const SENT_PREFIX = "nova-chat-sent:";
const NAMING_TIMEOUT_MS = 10_000;
const RETRY_MS = 30_000;
/** How often a request that was not posted is tried again, at most, before it waits for the user. */
const POST_ATTEMPTS = 5;
/** Why a request just kept is not posted yet. */
const NOT_POSTED = "Not posted yet.";
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
  /** Requests whose post runs now: the claim, the allowance wait and the submit. */
  const inFlight = new Set<Outgoing>();
  const requestAttempts = new Map<Outgoing, symbol>();
  const confirmations = new Map<Outgoing, AbortController>();
  /** Sends the user agreed to in New chat, by person, until the product's request spends them. */
  const agreed = new Map<PeerId, Agreed>();
  /** Closes the host's open prompts once it stops. */
  const prompts = new AbortController();
  /** Posts run one at a time, so two never race for one notification slot. */
  let posting: Promise<unknown> = Promise.resolve();
  let indexes: { [peer: PeerId]: string } = {};
  let posts: Record<string, CachedPost> = {};
  let acceptances: NonNullable<Saved["acceptances"]> = {};
  const accepting = new Map<string, Promise<void>>();

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
        posts,
        acceptances,
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
      posts = saved.posts ?? {};
      acceptances = saved.acceptances ?? {};
      let pausedInvalid = false;
      for (const item of outgoing) {
        const invalidBudget =
          Object.hasOwn(item, "attemptsLeft") &&
          (typeof item.attemptsLeft !== "number" ||
            !Number.isInteger(item.attemptsLeft) ||
            item.attemptsLeft < 0 ||
            item.attemptsLeft > POST_ATTEMPTS);
        const cached = posts[`chat-request:${item.requestId}`];
        const pending = cached?.confirmation;
        const invalidConfirmation =
          pending !== undefined &&
          (!Number.isSafeInteger(pending?.until) ||
            pending.until <= 0 ||
            typeof pending.submitted !== "boolean");
        if (!invalidBudget && !invalidConfirmation) continue;
        item.attemptsLeft = 0;
        item.problem =
          "Paused: invalid saved retry budget or confirmation. Cancel this request to start another.";
        if (cached !== undefined) {
          cached.needsFinal = true;
          delete cached.confirmation;
        }
        pausedInvalid = true;
      }
      if (pausedInvalid) save();
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
   * `requestId` and today, claimed for today, and submit it once that
   * account may post. The account is the same for every try of one request
   * on one day (the apps' `//chat-request//<id>//<period>`,
   * `//chat-accept//<id>//<period>`, with the peer's request id for an
   * acceptance), so a try again claims no new slot. Posts run one at a time.
   * `unwanted` says why the post is no longer wanted, or null; it is asked
   * before each step. Answers how the post ends once its claim's finality is
   * known, for a post that went at a best block.
   */
  const postAs = (
    kind: "chat-request" | "chat-accept",
    requestId: string,
    statement: Statement,
    unwanted: () => string | null = () => null,
    boundPeer?: PeerId,
  ): Promise<{ final: Promise<FinalOutcome> }> => {
    const run = async () => {
      if (store === null) throw new Error("the Statement Store is not open");
      const period = claimPeriod(now());
      const signer = options.notificationSigner(kind, requestId, period);
      const account = bytesToHex(signer.public);
      const key = `${kind}:${requestId}`;
      let cached = posts[key];
      if (
        kind === "chat-accept" &&
        cached !== undefined &&
        cached.peer !== boundPeer
      )
        throw new Error(
          "This saved acceptance is not bound to this peer. No new claim was submitted.",
        );
      if (cached !== undefined && cached.period !== period)
        throw new Error("the day changed; this post needs a new user action");
      if (cached === undefined) {
        cached = {
          period,
          ...(boundPeer === undefined ? {} : { peer: boundPeer }),
          bytes: bytesToHex(
            encodeSigned(statement, signer.public, signer.sign),
          ),
        };
        posts[key] = cached;
        save();
      }
      if (
        bytesToHex(decodeStatement(hexToBytes(cached.bytes)).signer) !== account
      )
        throw new Error(
          "The saved statement names another notification account.",
        );
      let postingStore: StatementStoreLike | null = null;
      const posted = await postWhenUsable(
        {
          claim: async () => {
            if (cached.needsFinal && cached.claim === undefined)
              throw new Error(
                "the original claim answer is missing; no recovery claim was submitted",
              );
            if (cached.claim !== undefined)
              return {
                account,
                period,
                ...cached.claim,
                ...(cached.needsFinal ? { blockHash: undefined } : {}),
              };
            // An interrupted acceptance claim must not spend another slot on retry.
            if (kind === "chat-accept") {
              cached.needsFinal = true;
              save();
            }
            const claim = await options.claimNotification(
              signer.public,
              period,
            );
            options.log(
              `${product.name}: ${kind} ${requestId.slice(0, 8)}… ${claim.line}`,
            );
            cached.claim = claim;
            delete cached.needsFinal;
            save();
            return { account, period, ...claim };
          },
          bytes: hexToBytes(cached.bytes),
          sign: cached.renewed ? undefined : signer.sign,
          retryUnconfirmed: cached.needsFinal,
          replaced: (bytes) => {
            cached.bytes = bytesToHex(bytes);
            cached.renewed = true;
            save();
          },
          submit: (bytes) => {
            if (postingStore === null)
              throw new Error("the Statement Store is not open");
            return postingStore.submit(bytes).then((result) => {
              if (isStored(result) && cached.confirmation !== undefined) {
                cached.confirmation.submitted = true;
                save();
              }
              return result;
            });
          },
          unwanted: () =>
            state !== "running"
              ? `${product.name} stopped`
              : claimPeriod(now()) !== period
                ? "the day changed"
                : unwanted(),
        },
        {
          guard: async (claim, request) => {
            postingStore = store;
            if (postingStore === null)
              throw new Error("the Statement Store is not open");
            const checked =
              request?.wait !== "finalized" &&
              (await postingStore.supportsBestBlockPosting?.()) === true;
            const usable = await options.notificationGuard(claim, {
              ...request,
              signal: prompts.signal,
              ...(!checked ||
              cached.needsFinal ||
              cached.confirmation !== undefined
                ? { wait: "finalized" }
                : {}),
            });
            if (usable.usableAt === "best") {
              cached.confirmation ??= {
                until: Math.min(
                  now() + FINALITY_TIMEOUT_MS,
                  (period + 1) * 86_400_000,
                  Number(statement.expiry >> 32n) * 1000,
                ),
                submitted: false,
              };
              save();
            }
            return usable;
          },
          sleep: (ms) => new Promise((resolve) => schedule(resolve, ms)),
          now,
        },
      );
      const { result } = posted;
      if (!isStored(result))
        throw new Error(
          `the Statement Store answered ${result.status}${result.reason === undefined ? "" : ` (${result.reason})`}`,
        );
      return {
        final: posted.final.then((outcome) => {
          if (
            outcome.tag === "unconfirmed" &&
            unwanted() === null &&
            state === "running" &&
            posts[key] === cached
          ) {
            cached.needsFinal = true;
            save();
          }
          return outcome;
        }),
      };
    };
    const next = posting.then(run, run);
    posting = next.catch(() => {});
    return next;
  };

  /** What a post that went at a best block came to, for the log; the problem it leaves, or null. */
  const afterFinal = (what: string, outcome: FinalOutcome): string | null => {
    if (outcome.tag === "final" || state !== "running") return null;
    if (outcome.tag === "reposted") {
      options.log(
        `${product.name}: ${what} was posted again, once its account held at the finalized head: ${outcome.why}.`,
      );
      return null;
    }
    const problem = `Posted at a best block, but ${outcome.why}.`;
    options.log(`${product.name}: ${what}: ${problem}`);
    return problem;
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
    delete acceptances[item.id];
    delete posts[`chat-accept:${requestId}`];
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
          for (const item of outgoing.filter((item) => item.peer === peer)) {
            confirmations.get(item)?.abort();
            delete posts[`chat-request:${item.requestId}`];
            requestAttempts.delete(item);
          }
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

  /** Ask the user to send a request to `peer`. False, unasked, once the host stopped. */
  const askSend = async (
    peer: PeerId,
    welcome: string | null,
  ): Promise<boolean> => {
    const name =
      options.nameOf(peer)?.name ??
      (await options.lookupName(peer)) ??
      "Contact";
    if (state !== "running") return false;
    return options.confirmSend({
      product,
      to: { name, handle: name },
      purpose: welcome ?? "Wants to chat with you.",
      signal: prompts.signal,
    });
  };

  /**
   * The worker's request to `handle`. It spends a Send the user agreed to in
   * New chat for this person and welcome; with none, the user is asked here.
   * It answers once the request is kept, and posts it after
   * ({@link postRequest}).
   */
  const request = async (
    handle: string,
    welcome: string | null,
  ): Promise<NovaResult> => {
    const peer = peerOf(handle);
    if (peer === null) return failure("The host cannot name this contact.");
    if (blocked(peer)) return failure("You blocked this person.");
    const grant = agreed.get(peer);
    const spent = grant?.welcome === welcome ? grant : null;
    if (spent !== null) agreed.delete(peer);
    else {
      const sent = await askSend(peer, welcome);
      if (state !== "running") return refusal(`${product.name} stopped.`);
      if (!sent) return failure("You did not send the request.");
    }
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
    if (spent?.live === false)
      return failure(
        "New chat stopped waiting before the request was kept. Nothing was sent.",
      );
    const item: Outgoing = {
      peer,
      requestId,
      welcome,
      createdAt: now(),
      problem: NOT_POSTED,
    };
    outgoing = [...outgoing.filter((each) => each.peer !== peer), item];
    save();
    peopleChanged();
    sentChanged();
    watchAcceptance(peer);
    tell(peer);
    // The allowance wait can outlast the product's start query, so the
    // request is answered once it is kept, and Sent shows how its post goes.
    void postRequest(item).catch((error: unknown) =>
      options.log(`${product.name}: ${errorText(error)}`),
    );
    return { ok: true };
  };

  /**
   * Post `item`'s request, built again with the same id, so a try again on
   * the same day signs with the same notification account and claims no new
   * slot. A failed try is tried again after {@link RETRY_MS} while attempts
   * last, including across restarts; then it waits, paused.
   */
  async function postRequest(item: Outgoing): Promise<void> {
    if (inFlight.has(item)) return;
    retries.get(item.peer)?.();
    retries.delete(item.peer);
    if (state !== "running" || !outgoing.includes(item)) return;
    if (blocked(item.peer)) {
      item.problem = "You blocked this person.";
      save();
      return sentChanged();
    }
    const cached = posts[`chat-request:${item.requestId}`];
    const confirming = cached?.confirmation !== undefined;
    if (!confirming && (item.attemptsLeft ?? POST_ATTEMPTS) === 0) return;
    if (!confirming)
      item.attemptsLeft = (item.attemptsLeft ?? POST_ATTEMPTS) - 1;
    save();
    inFlight.add(item);
    const attempt = Symbol();
    requestAttempts.set(item, attempt);
    sentChanged();
    const failed = (error: unknown) => {
      if (
        state !== "running" ||
        !outgoing.includes(item) ||
        requestAttempts.get(item) !== attempt
      )
        return;
      const again =
        (item.attemptsLeft ?? 0) > 0 &&
        !blocked(item.peer) &&
        claimPeriod(now()) === claimPeriod(item.createdAt);
      item.problem = again
        ? `Not posted yet, trying again in ${RETRY_MS / 1000} s: ${errorText(error)}`
        : `Not posted: ${errorText(error)}`;
      options.log(`${product.name}: ${item.problem}`);
      if (again)
        retries.set(
          item.peer,
          schedule(() => void postRequest(item), RETRY_MS),
        );
      const current = posts[`chat-request:${item.requestId}`];
      if (current?.confirmation !== undefined) {
        current.needsFinal = true;
        delete current.confirmation;
      }
      save();
      sentChanged();
    };
    try {
      if (claimPeriod(now()) !== claimPeriod(item.createdAt)) {
        item.attemptsLeft = 0;
        throw new Error("the request's day ended; cancel it to start another");
      }
      if (confirming) {
        await confirmRequest(item, cached);
        return;
      }
      const built = await boundary.request(hexToBytes(item.peer as HexString), {
        requestId: item.requestId,
        nowMs: now(),
        expiry: expiryAt(now()),
        welcome: item.welcome,
      });
      if (isRefused(built)) throw new Error(built.refused);
      const posted = await postAs(
        "chat-request",
        item.requestId,
        built.statement,
        () =>
          !outgoing.includes(item)
            ? "the request was cancelled"
            : blocked(item.peer)
              ? "You blocked this person."
              : null,
      );
      if (state !== "running") return;
      item.problem = null;
      options.log(`${product.name}: your request is posted.`);
      void posted.final.then((outcome) => {
        const late = afterFinal("your request", outcome);
        if (
          state !== "running" ||
          !outgoing.includes(item) ||
          requestAttempts.get(item) !== attempt
        )
          return;
        if (late !== null) failed(late);
        else {
          const current = posts[`chat-request:${item.requestId}`];
          if (current !== undefined) delete current.confirmation;
          save();
        }
      });
    } catch (error) {
      failed(error);
    } finally {
      inFlight.delete(item);
      sentChanged();
    }
    save();
    sentChanged();

    async function confirmRequest(
      item: Outgoing,
      cached: CachedPost,
    ): Promise<void> {
      const pending = cached.confirmation!;
      const statement = decodeStatement(hexToBytes(cached.bytes));
      const check = () => {
        if (state !== "running" || !outgoing.includes(item))
          throw new Error("the request was cancelled");
        if (blocked(item.peer)) throw new Error("You blocked this person.");
        if (claimPeriod(now()) !== cached.period) {
          item.attemptsLeft = 0;
          throw new Error("the request's day ended");
        }
        if (now() >= Number(statement.expiry >> 32n) * 1000) {
          item.attemptsLeft = 0;
          throw new Error("the statement has expired");
        }
      };
      check();
      if (now() >= pending.until) {
        item.attemptsLeft = 0;
        throw new Error(
          "the saved finality deadline ended; cancel the request to start another",
        );
      }
      if (cached.claim?.blockHash === undefined)
        throw new Error("the saved inclusion block is missing");
      const stop = new AbortController();
      confirmations.set(item, stop);
      const signal = AbortSignal.any([prompts.signal, stop.signal]);
      const cancel = schedule(() => stop.abort(), pending.until - now());
      const { aborted, release } = abortion(signal);
      let inclusionFinal = false;
      try {
        const account = bytesToHex(statement.signer);
        const confirmed = await Promise.race([
          options.notificationGuard(
            { account, period: cached.period, ...cached.claim },
            { wait: "finalized", signal },
          ),
          aborted,
        ]);
        check();
        if (!confirmed.claimFinal)
          throw new Error("the saved inclusion block is not finalized");
        if (now() >= pending.until)
          throw new Error("the saved finality deadline ended");
        inclusionFinal = true;
        if (!pending.submitted) {
          item.problem = NOT_POSTED;
          delete cached.confirmation;
          save();
          // A crash before the submit answer permits only an identical-byte retry.
          if (store === null)
            throw new Error("the Statement Store is not open");
          const connection = store;
          const sent = await submitUntilAllowed(
            {
              bytes: hexToBytes(cached.bytes),
              submit: (bytes) => connection.submit(bytes),
              unwanted: () => {
                try {
                  check();
                  signal.throwIfAborted();
                  return null;
                } catch (error) {
                  return errorText(error);
                }
              },
            },
            (ms) => new Promise((resolve) => schedule(resolve, ms)),
          );
          if (!isStored(sent.result))
            throw new Error(
              `the Statement Store answered ${sent.result.status}`,
            );
        }
        check();
        item.problem = null;
        delete cached.confirmation;
        save();
      } catch (error) {
        check();
        if (signal.aborted || now() >= pending.until) {
          item.attemptsLeft = 0;
          throw new Error(
            "the saved finality deadline ended; cancel the request to start another",
          );
        }
        if (!inclusionFinal) cached.needsFinal = true;
        throw error;
      } finally {
        cancel();
        release();
        confirmations.delete(item);
      }
    }
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
          if (
            blocked(peer) ||
            boundary.state(peer) !== "established" ||
            indexes[peer] !== message.index
          )
            return refusal("This chat is no longer available.");
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
      if (
        item.problem !== null ||
        posts[`chat-request:${item.requestId}`]?.confirmation !== undefined
      )
        void postRequest(item);
    }
    for (const id of Object.keys(acceptances)) {
      const item = options.notifications.get(id);
      if (item?.state.tag === "pending")
        void resumeAcceptance(item).catch((error: unknown) => {
          if (state === "running")
            options.notifications.posting(id, errorText(error));
        });
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
    prompts.abort();
    for (const grant of agreed.values()) grant.live = false;
    agreed.clear();
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

  const acceptanceWanted = (
    item: HostNotification,
    kept: NonNullable<Saved["acceptances"]>[string],
  ): string | null =>
    state !== "running"
      ? `${product.name} stopped`
      : blocked(kept.peer)
        ? "You blocked this person."
        : acceptances[item.id] !== kept ||
            options.notifications.get(item.id)?.state.tag !== "pending" ||
            boundary.requestId(kept.peer) !== kept.requestId
          ? "This request is no longer pending."
          : null;

  const finishAcceptance = (
    item: HostNotification,
    kept: NonNullable<Saved["acceptances"]>[string],
    outcome: FinalOutcome,
  ) => {
    if (acceptanceWanted(item, kept) !== null) return;
    const late = afterFinal(
      `the acceptance of ${item.sender.name}'s request`,
      outcome,
    );
    if (late !== null) {
      const cached = posts[`chat-accept:${kept.requestId}`];
      if (cached !== undefined) {
        cached.needsFinal = true;
        delete cached.confirmation;
      }
      save();
      options.notifications.posting(
        item.id,
        `${late} Select Accept to retry the same acceptance. It is not confirmed delivered.`,
      );
      return;
    }
    boundary.accepted(kept.peer);
    save();
    options.notifications.posting(item.id, null);
    options.notifications.accept(item.id);
    delete posts[`chat-accept:${kept.requestId}`];
    delete acceptances[item.id];
    save();
    tell(kept.peer);
  };

  async function resumeAcceptance(item: HostNotification): Promise<void> {
    const kept = acceptances[item.id];
    if (kept === undefined || accepting.has(item.id)) return;
    const cached = posts[`chat-accept:${kept.requestId}`];
    if (cached?.confirmation === undefined) {
      options.notifications.posting(
        item.id,
        "Acceptance is unconfirmed. Select Accept to retry the same acceptance.",
      );
      return;
    }
    const pending = cached.confirmation;
    const check = () => {
      const why = acceptanceWanted(item, kept);
      if (why !== null) throw new Error(why);
      if (
        !Number.isSafeInteger(pending.until) ||
        typeof pending.submitted !== "boolean" ||
        now() >= pending.until ||
        claimPeriod(now()) !== cached.period
      )
        throw new Error(
          "The saved acceptance confirmation ended. Retry explicitly; no new slot was claimed.",
        );
    };
    const run = async () => {
      let cancel = () => {};
      let release = () => {};
      try {
        check();
        const statement = decodeStatement(hexToBytes(cached.bytes));
        if (
          cached.peer !== kept.peer ||
          bytesToHex(statement.signer) !==
            bytesToHex(
              options.notificationSigner(
                "chat-accept",
                kept.requestId,
                cached.period,
              ).public,
            )
        )
          throw new Error(
            "The saved acceptance's peer or notification account does not match.",
          );
        if (now() >= Number(statement.expiry >> 32n) * 1000)
          throw new Error("The saved acceptance expired.");
        if (cached.claim?.blockHash === undefined)
          throw new Error("The saved inclusion block is missing.");
        const deadline = new AbortController();
        const signal = AbortSignal.any([prompts.signal, deadline.signal]);
        cancel = schedule(() => deadline.abort(), pending.until - now());
        const abortionWait = abortion(signal);
        release = abortionWait.release;
        const confirmed = await Promise.race([
          options.notificationGuard(
            {
              account: bytesToHex(statement.signer),
              period: cached.period,
              ...cached.claim,
            },
            { wait: "finalized", signal },
          ),
          abortionWait.aborted,
        ]);
        check();
        if (!confirmed.claimFinal)
          throw new Error("The acceptance's claim is not proved final.");
        if (!pending.submitted) {
          const connection = store;
          if (connection === null)
            throw new Error("The Statement Store is not open.");
          const sent = await submitUntilAllowed(
            {
              bytes: hexToBytes(cached.bytes),
              submit: (bytes) => connection.submit(bytes),
              unwanted: () => {
                try {
                  check();
                  signal.throwIfAborted();
                  return null;
                } catch (error) {
                  return errorText(error);
                }
              },
            },
            (ms) => new Promise((resolve) => schedule(resolve, ms)),
          );
          if (!isStored(sent.result))
            throw new Error(
              `The Statement Store answered ${sent.result.status}.`,
            );
        }
        finishAcceptance(item, kept, { tag: "final" });
      } catch (error) {
        finishAcceptance(item, kept, {
          tag: "unconfirmed",
          why: errorText(error),
        });
      } finally {
        cancel();
        release();
      }
    };
    const pendingRun = run().finally(() => accepting.delete(item.id));
    accepting.set(item.id, pendingRun);
    await pendingRun;
  }

  const acceptNow = async (item: HostNotification): Promise<void> => {
    const peer = peerOfItem(item.id);
    const theirs = requestOfItem(item.id);
    if (
      peer === null ||
      theirs === null ||
      boundary.requestId(peer) !== theirs ||
      options.notifications.get(item.id)?.state.tag !== "pending" ||
      (boundary.state(peer) !== "incoming" &&
        acceptances[item.id] === undefined)
    )
      throw new Error("This request is no longer pending.");
    if (blocked(peer)) throw new Error("You blocked this person.");
    const accepted = await options.confirmAccept({
      product,
      from: item.sender,
      purpose: item.purpose,
      signal: prompts.signal,
    });
    if (!accepted) throw new Error("You did not accept.");
    if (state !== "running") throw new Error(`${product.name} stopped.`);
    if (blocked(peer)) throw new Error("You blocked this person.");
    const kept = { peer, requestId: theirs };
    acceptances[item.id] = kept;
    const cached = posts[`chat-accept:${theirs}`];
    const statement =
      cached === undefined
        ? boundary.accept(peer, {
            messageId: newId(),
            nowMs: now(),
            expiry: expiryAt(now()),
          })
        : decodeStatement(hexToBytes(cached.bytes));
    if (isRefused(statement)) throw new Error(statement.refused);
    try {
      save();
      const posted = await postAs(
        "chat-accept",
        theirs,
        statement,
        () => acceptanceWanted(item, kept),
        peer,
      );
      if (acceptanceWanted(item, kept) !== null)
        throw new Error(acceptanceWanted(item, kept)!);
      boundary.accepted(peer);
      save();
      options.notifications.posting(
        item.id,
        "Acceptance posted here. Checking its claim; this is not delivery confirmation.",
      );
      tell(peer);
      void posted.final.then((outcome) =>
        finishAcceptance(item, kept, outcome),
      );
    } catch (error) {
      if (acceptanceWanted(item, kept) === null)
        options.notifications.posting(
          item.id,
          `Acceptance not confirmed: ${errorText(error)} Select Accept to retry.`,
        );
      throw new Error(`The acceptance was not posted: ${errorText(error)}`);
    }
    options.log(`${product.name}: you accepted ${item.sender.name}'s request.`);
  };

  const accept = (item: HostNotification): Promise<void> => {
    const current = accepting.get(item.id);
    if (current !== undefined) return current;
    const pending = acceptNow(item).finally(() => accepting.delete(item.id));
    accepting.set(item.id, pending);
    return pending;
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
      // The user decides before the worker is asked: the allowance wait and
      // the prompt are both longer than the start's time limit allows.
      confirm: async (account, welcome) => {
        const peer = account.toLowerCase();
        if (state !== "running")
          throw new Error(`${product.name} is not running.`);
        if (blocked(peer)) throw new Error("You blocked this person.");
        const text = welcome.trim() === "" ? null : welcome;
        const sent = await askSend(peer, text);
        if (state !== "running") throw new Error(`${product.name} stopped.`);
        if (!sent) return null;
        const grant: Agreed = { welcome: text, live: true };
        agreed.set(peer, grant);
        return () => {
          grant.live = false;
          if (agreed.get(peer) === grant) agreed.delete(peer);
        };
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
          status:
            item.problem === null
              ? { tag: "waiting" as const }
              : inFlight.has(item)
                ? { tag: "posting" as const }
                : retries.has(item.peer)
                  ? { tag: "retrying" as const, reason: item.problem }
                  : { tag: "paused" as const, reason: item.problem },
        }))
        .sort((left, right) => right.createdAt - left.createdAt),
    cancel: (id) => {
      if (!id.startsWith(SENT_PREFIX)) return false;
      const peer = id.slice(SENT_PREFIX.length);
      if (!boundary.withdraw(peer)) return false;
      for (const item of outgoing.filter((item) => item.peer === peer)) {
        confirmations.get(item)?.abort();
        delete posts[`chat-request:${item.requestId}`];
        requestAttempts.delete(item);
      }
      outgoing = outgoing.filter((item) => item.peer !== peer);
      retries.get(peer)?.();
      retries.delete(peer);
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
