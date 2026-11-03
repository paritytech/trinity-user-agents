import {
  subscriptionKey,
  type StatementTraffic,
  type StatementTrafficSink,
  type TrafficSource,
  type RpcErrorValue,
} from "../../src/platform/statement-observer.js";
import type { InvitationEvent } from "../../src/invitations/observer.js";
import type {
  ConversationDiagnostics,
  DiagnosticsReport,
} from "../../src/products/diagnostics.js";
import { redactUrls } from "../../src/platform/activity.js";
import { bytesToHex } from "@parity/truapi/scale";
import {
  clientRefusal,
  wentOut,
  nodeText,
  shapeOf,
  readFilter,
  readPage,
  readStatementHex,
  readSubmitAnswer,
  type FilterView,
  type StatementMeta,
  type SubmitAnswer,
} from "./decode.js";

/**
 * What the inspector's Statements view knows: a bounded, in-memory ledger of
 * the Statement Store traffic, resources and conversations this tab saw,
 * kept for this session only. Nothing is stored or sent.
 *
 * Every fact keeps where it came from:
 *
 * - the answers and pages of this tab's People connection, as they crossed
 *   it (`src/platform/statement-observer.ts`). That connection is the
 *   embedded light client, which has no statement store: an answer `new`
 *   means a peer took the broadcast, peers' own refusals never reach it, and
 *   its pages carry what peers sent (see `decode.ts`);
 * - the core's claim results, as the host received them;
 * - the host's own request service and delivery (`src/invitations/observer.ts`);
 * - grant reads at a block, made on demand;
 * - what a product reported about itself (`src/products/diagnostics.ts`).
 *
 * What it derives from them, such as an account's latest observed
 * statements, describes what this tab saw and when, never what the network
 * holds now: peers evict without saying so, refuse without saying so, and
 * send only some of what they hold. A statement seen earlier may be gone;
 * one never seen here may be held.
 *
 * Statement data is kept only while capture is on, within a byte budget, and
 * is never exported. Keys, seeds and secrets never reach the ledger: no
 * source it reads carries them.
 */

/** How much the ledger keeps of each kind. The oldest go first. */
export interface LedgerLimits {
  events: number;
  statements: number;
  submits: number;
  subscriptions: number;
  accounts: number;
  topics: number;
  connections: number;
  /** Channels, by signer and channel. */
  channels: number;
  /** Conversations products reported. */
  conversations: number;
  /** Requests the host reported. */
  requests: number;
  /** Bytes of statement data kept while capture is on, in all, waiting for the node's answer included. */
  payloadBytes: number;
  /** Bytes of one statement's data kept while capture is on. */
  payloadPerStatement: number;
}

export const DEFAULT_LIMITS: LedgerLimits = {
  events: 800,
  statements: 1000,
  submits: 400,
  subscriptions: 200,
  accounts: 300,
  topics: 300,
  connections: 200,
  channels: 600,
  conversations: 64,
  requests: 200,
  payloadBytes: 512 * 1024,
  payloadPerStatement: 16 * 1024,
};

export type EventKind =
  | "session"
  | "connection"
  | "submit"
  | "subscription"
  | "page"
  | "claim"
  | "request"
  | "product"
  | "grant";

/** What an event is about, for filtering and for following it to other views. */
export interface EventRefs {
  accounts?: string[];
  topics?: string[];
  hash?: string;
  handle?: string;
  conversation?: string;
  product?: string;
  subscription?: number;
  submit?: number;
}

export interface LedgerEvent {
  /** Unique in this tab and never reused. */
  id: number;
  at: number;
  kind: EventKind;
  /** Where the fact came from. */
  source: FactSource;
  text: string;
  failed?: boolean;
  refs: EventRefs;
  details: [string, string][];
}

/** Where a fact comes from, as the view labels it. */
export type FactSource =
  | "client"
  | "core"
  | "host"
  | "product"
  | "chain"
  | "inspector";

export interface Connection {
  id: number;
  source: TrafficSource;
  openedAt: number;
  /** It was opened before this session started: its earlier traffic is not in the ledger. */
  earlier?: boolean;
  closedAt?: number;
  closedBy?: "caller" | "ended" | "failed";
  reason?: string;
}

export type SubmitOutcome =
  | { kind: "pending" }
  | { kind: "answered"; at: number; answer: SubmitAnswer }
  /** The RPC layer refused it. `local` when this tab's no-submit guard did, before the network. */
  | {
      kind: "rpc-error";
      at: number;
      error: RpcErrorValue;
      local: boolean;
      /** What the light client's error means, when its message is one it is known to give. */
      meaning?: string;
    }
  | { kind: "unanswered"; at: number; why: string };

export interface SubmitRecord {
  id: number;
  at: number;
  source: TrafficSource;
  connection: number;
  meta: StatementMeta | null;
  /** Why the submitted parameter could not be read as a statement. */
  unreadable?: string;
  outcome: SubmitOutcome;
}

/**
 * Where in a subscription's stream its statements came. `initial`: in a
 * page carrying `remaining`, the stored replay a full node sends first (the
 * light client's initial page is always empty). `later`: after that
 * initial batch; from the light client these are what peers sent, new or
 * long stored. `unclassified`: the stream gave no `remaining` at all.
 */
export type Delivery = "initial" | "later" | "unclassified";

export interface SubscriptionRecord {
  id: number;
  source: TrafficSource;
  connection: number;
  filter: FilterView;
  requestedAt: number;
  openedAt?: number;
  remote?: string;
  /** It was opened before this session started: its counts start at the session's start. */
  earlier?: boolean;
  state: "opening" | "open" | "refused" | "ended";
  /** Where its stream stands, from `remaining`: waiting for the first page, in the initial batch, after it, or unknown. */
  phase: "waiting" | "initial" | "later" | "unknown";
  lastRemaining?: number;
  sawRemaining: boolean;
  pages: number;
  unrecognisedPages: number;
  statements: number;
  byDelivery: Record<Delivery, number>;
  /** Statements this subscription delivered again. */
  duplicates: number;
  endedAt?: number;
  endedHow?: "unsubscribed" | "connection closed" | "refused";
  error?: string;
  /** Statement hashes it delivered, in order, at most 200. */
  hashes: string[];
}

/** How the ledger came to see a statement: this tab's broadcast that went out, or a subscription page. */
export type Sighting = "broadcast" | Delivery;

export interface ObservedStatement {
  meta: StatementMeta;
  firstAt: number;
  lastAt: number;
  sightings: number;
  via: Sighting[];
  subscriptions: number[];
  submits: number[];
  /** Its data, `0x` hex, while capture is on. */
  payload?: string;
  payloadCut?: boolean;
}

export interface ChannelRecord {
  signer: string;
  channel: string;
  /** The hash of the statement with the highest expiry seen. */
  latest: string;
  latestExpiry: string;
  /** Times a higher expiry was seen on it after a lower one. */
  replacements: number;
  /** Statements seen with a lower expiry than the latest, after it. */
  older: number;
  /** Submissions answered `channelPriorityTooLow`: a full node's refusal, which the light client never gives. */
  priorityRefusals: number;
  history: string[];
}

export type Role =
  | { kind: "product-account"; product: string; index: string }
  | { kind: "notification-account"; post?: string; period?: number }
  | {
      kind: "conversation-account";
      product: string;
      conversation: string;
      protocol: string;
    }
  | { kind: "conversation-peer"; product: string; conversation: string }
  | { kind: "chat-allowance"; product: string; handle: string };

/**
 * Whether `role` records that the core or this host derived the account from
 * the wallet signed in now: the core's claim of a product's account, the
 * host delivery's notification account for one of its posts, or the host's
 * chat allowance. A notification claim alone names an account the caller
 * chose, so it is not one. The ledger starts empty for each session, so
 * these hold for the current wallet only.
 */
function derivedHere(role: Role): boolean {
  switch (role.kind) {
    case "product-account":
    case "chat-allowance":
      return true;
    case "notification-account":
      return role.post !== undefined;
    default:
      return false;
  }
}

/**
 * Whose an account is, as far as this tab knows in this session. A local
 * record of where the account came from, not a signature check.
 *
 * - `this-wallet`: the core or this host derived it from the signed-in
 *   wallet in this session.
 * - `reported`: a product named it, and nothing derived it here. That says
 *   nothing of whose it is.
 * - `unknown`: anything else: only seen signing or claimed for, or its record
 *   was dropped. It may still be this wallet's, from another origin, browser
 *   or an earlier session: those derive accounts from indexes and post ids
 *   this tab does not hold.
 */
export type Ownership =
  | { kind: "this-wallet"; evidence: Role[] }
  | { kind: "reported"; roles: Role[] }
  | { kind: "unknown" };

/** `account`'s {@link Ownership}; unknown when the ledger holds no record of it. */
export function ownership(account: AccountRecord | undefined): Ownership {
  const roles = account?.roles ?? [];
  const evidence = roles.filter(derivedHere);
  if (evidence.length > 0) return { kind: "this-wallet", evidence };
  const reported = roles.filter(
    (role) =>
      role.kind === "conversation-account" || role.kind === "conversation-peer",
  );
  return reported.length > 0
    ? { kind: "reported", roles: reported }
    : { kind: "unknown" };
}

export interface ClaimRecord {
  at: number;
  kind: "product" | "notification";
  product?: string;
  index?: string;
  account?: string;
  submit: boolean;
  outcome: "registered" | "alreadyAllocated" | "validated" | "failed";
  period?: number;
  seq?: number;
  collection?: string;
  blockHash?: string;
  reason?: string;
}

export type GrantRead =
  | {
      at: number;
      block: string;
      value: { maxCount: number; maxSize: number } | null;
    }
  | { at: number; error: string };

export interface AccountRecord {
  account: string;
  /** Increases with each record the ledger makes: a record dropped and made again has a new one. */
  seq: number;
  firstAt: number;
  /**
   * Products the host or the core tied this account to: a product-account
   * claim, or an allowance for a request of that product. Only these
   * products' reports are joined with the account's facts.
   */
  owners: string[];
  roles: Role[];
  claims: ClaimRecord[];
  grant?: GrantRead;
  /** Submissions seen signed by it: attempts, whatever the answer. */
  submits: number;
  /** Submissions answered `new` or `known`: broadcast to a peer, not shown stored. With their data bytes. */
  wentOut: number;
  wentOutDataBytes: number;
  /** The light client's answers to its submissions, by status. */
  answers: Partial<Record<string, number>>;
  /** Its submissions the RPC layer answered with an error, past this tab's no-submit guard. */
  rpcErrors: number;
  /** Its submissions this tab's no-submit guard refused: none reached the light client. */
  refusedLocally: number;
  lastRefusal?: { at: number; submit: number; answer: SubmitAnswer };
}

/**
 * A topic as this tab met it: on statements it observed or was asked to
 * submit, in a subscription's filter, or in a product's report. Which
 * product reports name it is read from the conversations, not kept here.
 */
export interface TopicRecord {
  topic: string;
  firstAt: number;
  /** Statements observed on it; one dropped from the list and seen again counts again. */
  statements: number;
  /** When this tab last observed a statement on it. Not that any peer still holds one. */
  lastAt?: number;
  /** Submissions on it the core or the host asked for, whatever the answer. */
  submits: number;
  /** Of those, the ones the light client answered `new` or `known`: broadcast to a peer, not shown stored. */
  wentOut: number;
  subscriptions: number[];
}

export interface ConversationRecord {
  key: string;
  product: string;
  run: number;
  reportedAt: number;
  report: ConversationDiagnostics;
  /** The worker stopped: its last report no longer holds. */
  stale: boolean;
  /**
   * Where the cost window starts: this run's first report seen here for
   * this conversation account, with the counts the ledger and the product
   * had then.
   */
  baseline: CostBaseline;
}

export interface CostBaseline {
  at: number;
  /** The account the report named, or none. */
  account?: string;
  /** The {@link AccountRecord.seq} of that account's record when the window started. */
  accountSeq: number | null;
  attempts: number;
  wentOut: number;
  wentOutDataBytes: number;
  /** The product's `sent` count in that report. */
  sent: number;
}

/** What a conversation spent since its baseline, or why that is not known. */
export type Cost =
  | {
      known: true;
      since: number;
      attempts: number;
      wentOut: number;
      wentOutDataBytes: number;
      sent: number;
    }
  | { known: false; why: string };

export interface RequestRecord {
  handle: string;
  product: string;
  role: "sender" | "recipient" | "unknown";
  state: string;
  firstAt: number;
  lastAt: number;
  index?: string;
  account?: string;
  until?: number;
  refusal?: string;
  notificationAccounts: string[];
  problem?: string;
}

/** What one claim asked the core. */
export type ClaimRequest =
  | { kind: "product"; product: string; index: string; submit: boolean }
  | { kind: "notification"; account: string; submit: boolean };

/** What the core answered a claim with. A notification claim names no account: the request did. */
export interface ClaimAnswer {
  account?: string;
  period: number;
  outcome: string;
  seq: number;
  collection?: string;
  blockHash?: string;
}

/** Counts since this session started in this tab. Nothing evicts them. */
export interface Counters {
  connections: number;
  submits: number;
  answers: Partial<Record<string, number>>;
  rpcErrors: number;
  refusedLocally: number;
  unanswered: number;
  submittedDataBytes: number;
  submittedEncodedBytes: number;
  subscriptions: number;
  subscriptionsRefused: number;
  pages: number;
  delivered: number;
  duplicates: number;
  claims: Partial<Record<ClaimRecord["outcome"], number>>;
  grantReads: number;
  reports: number;
}

const emptyCounters = (): Counters => ({
  connections: 0,
  submits: 0,
  answers: {},
  rpcErrors: 0,
  refusedLocally: 0,
  unanswered: 0,
  submittedDataBytes: 0,
  submittedEncodedBytes: 0,
  subscriptions: 0,
  subscriptionsRefused: 0,
  pages: 0,
  delivered: 0,
  duplicates: 0,
  claims: {},
  grantReads: 0,
  reports: 0,
});

/**
 * An account's latest observed statements: what this tab saw, not what the
 * node holds now. See {@link StatementLedger.latestObserved}.
 */
export interface LatestObserved {
  /** Statements counted: latest observed per channel, and those without a channel, not known expired. */
  statements: number;
  dataBytes: number;
  /** Statements that would count but whose expiry has passed. */
  expired: number;
  /** When the newest of the counted statements was last observed, or null when none is. */
  lastObservedAt: number | null;
  /** Channels whose latest observed statement the ledger dropped from its list: not counted. */
  dropped: number;
}

const SUBMIT = "statement_submit";
const SUBSCRIBE = "statement_subscribeStatement";
const UNSUBSCRIBE = "statement_unsubscribeStatement";
/** How many of its latest statement hashes a subscription keeps, to tell a repeat. */
export const HASHES_PER_SUBSCRIPTION = 200;
const HISTORY = 8;
const CLAIMS_PER_ACCOUNT = 8;
const ROLES_PER_ACCOUNT = 8;

export const short = (hex: string, keep = 6): string =>
  hex.length <= 2 + keep * 2
    ? hex
    : `${hex.slice(0, 2 + keep)}…${hex.slice(-4)}`;

function push<T>(list: T[], value: T): void {
  if (!list.includes(value)) list.push(value);
}

export class StatementLedger {
  private limits: LedgerLimits;
  private counters = emptyCounters();
  private events: LedgerEvent[] = [];
  private nextEvent = 1;
  private nextSubmit = 1;
  private nextSubscription = 1;
  private readonly connections = new Map<number, Connection>();
  /** Requests waiting for an answer, by connection and id: the submit or subscription they belong to. */
  private readonly waiting = new Map<
    string,
    { method: string; submit?: number; subscription?: number }
  >();
  private readonly submits = new Map<number, SubmitRecord>();
  private readonly subscriptions = new Map<number, SubscriptionRecord>();
  /** Subscriptions by connection and the node's id for them. */
  private readonly remotes = new Map<string, number>();
  private readonly statements = new Map<string, ObservedStatement>();
  private readonly channels = new Map<string, ChannelRecord>();
  private readonly accounts = new Map<string, AccountRecord>();
  private readonly topics = new Map<string, TopicRecord>();
  private readonly conversations = new Map<string, ConversationRecord>();
  private readonly requests = new Map<string, RequestRecord>();
  private readonly evicted: Record<string, number> = {};
  private capturing = false;
  private payloadBytes = 0;
  private payloadFull = false;
  private sessionKey: string | null = null;
  private sessionStarted: number;
  private readonly listeners = new Set<() => void>();
  /** Counts sessions and clears, so an answer that started before one is dropped. */
  private epochValue = 0;
  private nextAccountSeq = 1;
  /** Node ids of open subscriptions dropped from the list, by connection. */
  private readonly droppedOpen = new Set<string>();
  /** Statement data of submissions waiting for the node's answer, by submit, while capture is on. */
  private readonly pendingData = new Map<
    number,
    { kept: Uint8Array; length: number }
  >();
  private pendingDataBytes = 0;

  /**
   * `localRefusal` tells an RPC error this tab made itself, such as a
   * validate-only tab's refusal to submit, from one the node or the light
   * client gave.
   */
  constructor(
    limits: Partial<LedgerLimits> = {},
    private readonly now: () => number = Date.now,
    private readonly localRefusal: (error: RpcErrorValue) => boolean = () =>
      false,
  ) {
    this.limits = { ...DEFAULT_LIMITS, ...limits };
    this.sessionStarted = now();
  }

  /** The current session and clear count. An operation that started under another drops its result. */
  epoch(): number {
    return this.epochValue;
  }

  // --- sessions and settings ---------------------------------------------

  /**
   * The session that is signed in now. A different key forgets everything of
   * the last one. Answers whether the session changed.
   */
  startSession(key: string, label: string): boolean {
    if (key === this.sessionKey) return false;
    const first = this.sessionKey === null;
    this.sessionKey = key;
    if (!first) this.reset();
    this.event("session", "inspector", `Session: ${label}`, {}, [
      [
        "Note",
        "The ledger starts empty for each session. Counters count from here.",
      ],
    ]);
    this.notify();
    return true;
  }

  /** Forget everything, as at a new session. Capture stays as it is, with nothing captured. */
  clear(): void {
    this.reset();
    this.notify();
  }

  /**
   * Forget the session's facts. The core's and the host's connections may
   * outlive a session, so their source and their open subscriptions stay, as
   * opened earlier, with counts from now; their earlier traffic goes.
   */
  private reset(): void {
    this.epochValue += 1;
    this.counters = emptyCounters();
    this.events = [];
    const connections = [...this.connections.values()].filter(
      (connection) => connection.closedAt === undefined,
    );
    const open = new Set(connections.map((connection) => connection.id));
    const subscriptions = [...this.subscriptions.values()].filter(
      (record) =>
        open.has(record.connection) &&
        (record.state === "open" || record.state === "opening"),
    );
    const kept = new Set(subscriptions.map((record) => record.id));
    const remotes = [...this.remotes].filter(([, id]) => kept.has(id));
    const waiting = [...this.waiting].filter(
      ([, entry]) =>
        entry.subscription !== undefined &&
        entry.method === SUBSCRIBE &&
        kept.has(entry.subscription),
    );
    this.connections.clear();
    this.waiting.clear();
    this.submits.clear();
    this.subscriptions.clear();
    this.remotes.clear();
    this.statements.clear();
    this.channels.clear();
    this.accounts.clear();
    this.topics.clear();
    this.conversations.clear();
    this.requests.clear();
    for (const key of Object.keys(this.evicted)) delete this.evicted[key];
    this.pendingData.clear();
    this.pendingDataBytes = 0;
    this.payloadBytes = 0;
    this.payloadFull = false;
    this.sessionStarted = this.now();
    for (const connection of connections)
      if (open.has(connection.id))
        this.connections.set(connection.id, { ...connection, earlier: true });
    for (const record of subscriptions) {
      this.subscriptions.set(record.id, {
        ...record,
        earlier: true,
        pages: 0,
        unrecognisedPages: 0,
        statements: 0,
        byDelivery: { initial: 0, later: 0, unclassified: 0 },
        duplicates: 0,
        hashes: [],
      });
      if ("topics" in record.filter)
        for (const topic of record.filter.topics)
          push(this.topic(topic).subscriptions, record.id);
    }
    for (const [key, id] of remotes) this.remotes.set(key, id);
    for (const [key, entry] of waiting) this.waiting.set(key, entry);
  }

  get capture(): boolean {
    return this.capturing;
  }

  /** Keep statement data from now on, or stop and forget every byte kept. */
  setCapture(on: boolean): void {
    if (on === this.capturing) return;
    this.capturing = on;
    if (!on) {
      for (const statement of this.statements.values()) {
        delete statement.payload;
        delete statement.payloadCut;
      }
      this.pendingData.clear();
      this.pendingDataBytes = 0;
      this.payloadBytes = 0;
      this.payloadFull = false;
    }
    this.event(
      "session",
      "inspector",
      on
        ? "Payload capture on: statement data is kept in memory in this tab"
        : "Payload capture off: every captured byte is forgotten",
      {},
      [],
    );
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of [...this.listeners]) listener();
  }

  // --- traffic -------------------------------------------------------------

  readonly traffic: StatementTrafficSink = (event) => {
    this.onTraffic(event);
    this.notify();
  };

  private onTraffic(event: StatementTraffic): void {
    const at = this.now();
    switch (event.type) {
      case "connected": {
        this.connections.set(event.connection, {
          id: event.connection,
          source: event.source,
          openedAt: at,
        });
        this.boundConnections();
        this.counters.connections += 1;
        this.event(
          "connection",
          "inspector",
          `Connection ${event.connection} (${event.source}) carries statement traffic`,
          {},
          [],
        );
        return;
      }
      case "request":
        return this.request(event, at);
      case "response":
        return this.response(event, at);
      case "notification":
        return this.notification(event, at);
      case "closed":
        return this.closed(event, at);
    }
  }

  private sourceOf(connection: number): TrafficSource {
    return this.connections.get(connection)?.source ?? "host";
  }

  private request(
    event: Extract<StatementTraffic, { type: "request" }>,
    at: number,
  ): void {
    const key = `${event.connection}:${event.id}`;
    const source = this.sourceOf(event.connection);
    if (event.method === SUBMIT) {
      const hex = Array.isArray(event.params) ? event.params[0] : undefined;
      const read = readStatementHex(hex);
      const record: SubmitRecord = {
        id: this.nextSubmit++,
        at,
        source,
        connection: event.connection,
        meta: read?.meta ?? null,
        ...(read === null ? { unreadable: "the parameter is not hex" } : {}),
        outcome: { kind: "pending" },
      };
      this.submits.set(record.id, record);
      this.bound(this.submits, this.limits.submits, "submits");
      this.wait(key, { method: event.method, submit: record.id });
      this.counters.submits += 1;
      if (read !== null) {
        this.counters.submittedDataBytes += read.meta.dataBytes;
        this.counters.submittedEncodedBytes += read.meta.encodedBytes;
        const signer = read.meta.signer;
        if (signer !== undefined) this.account(signer).submits += 1;
        for (const topic of read.meta.topics) this.topic(topic).submits += 1;
        if (this.capturing && read.data !== null)
          this.holdPending(record.id, read.data);
      }
      this.event(
        "submit",
        source === "core" ? "core" : "host",
        `Submit #${record.id} asked by the ${source}${read === null ? "" : `: ${read.meta.dataBytes} data bytes, signer ${read.meta.signer === undefined ? "none" : short(read.meta.signer)}`}`,
        {
          submit: record.id,
          ...(read?.meta.signer === undefined
            ? {}
            : { accounts: [read.meta.signer] }),
          ...(read === null
            ? {}
            : { hash: read.meta.hash, topics: read.meta.topics }),
        },
        read === null ? [] : metaDetails(read.meta),
      );
      return;
    }
    if (event.method === SUBSCRIBE) {
      const filter = readFilter(event.params);
      const record: SubscriptionRecord = {
        id: this.nextSubscription++,
        source,
        connection: event.connection,
        filter,
        requestedAt: at,
        state: "opening",
        phase: "waiting",
        sawRemaining: false,
        pages: 0,
        unrecognisedPages: 0,
        statements: 0,
        byDelivery: { initial: 0, later: 0, unclassified: 0 },
        duplicates: 0,
        hashes: [],
      };
      this.subscriptions.set(record.id, record);
      this.boundSubscriptions();
      this.wait(key, { method: event.method, subscription: record.id });
      this.counters.subscriptions += 1;
      const topics = "topics" in filter ? filter.topics : [];
      for (const topic of topics)
        push(this.topic(topic).subscriptions, record.id);
      this.event(
        "subscription",
        "inspector",
        `Subscription S${record.id} asked by the ${source}: ${filterText(filter)}`,
        { subscription: record.id, topics },
        [],
      );
      return;
    }
    if (event.method === UNSUBSCRIBE) {
      const remote = Array.isArray(event.params)
        ? subscriptionKey(JSON.stringify(event.params[0]))
        : undefined;
      const id =
        remote === undefined
          ? undefined
          : this.remotes.get(`${event.connection}:${remote}`);
      this.wait(key, { method: event.method, subscription: id });
      if (id !== undefined) this.endSubscription(id, at, "unsubscribed");
      return;
    }
    this.wait(key, { method: event.method });
  }

  /**
   * Remember a request until its answer. Past the submit limit the oldest is
   * forgotten, and a submission it belonged to is unanswered as far as the
   * ledger knows.
   */
  private wait(
    key: string,
    value: { method: string; submit?: number; subscription?: number },
  ): void {
    this.waiting.set(key, value);
    while (this.waiting.size > this.limits.submits) {
      const [oldest, entry] = this.waiting.entries().next().value as [
        string,
        { submit?: number },
      ];
      this.waiting.delete(oldest);
      this.evicted["waiting requests"] =
        (this.evicted["waiting requests"] ?? 0) + 1;
      if (entry.submit !== undefined)
        this.unanswered(
          entry.submit,
          this.now(),
          "the inspector stopped waiting: too many requests were waiting",
        );
    }
  }

  /** Keep a submission's data until the node answers, within the capture budget. */
  private holdPending(submit: number, data: Uint8Array): void {
    const kept = data.slice(0, this.limits.payloadPerStatement);
    if (
      this.payloadBytes + this.pendingDataBytes + kept.length >
      this.limits.payloadBytes
    ) {
      this.payloadFull = true;
      return;
    }
    this.pendingData.set(submit, { kept, length: data.length });
    this.pendingDataBytes += kept.length;
  }

  /** The data held for `submit`, released from the pending budget. */
  private releasePending(
    submit: number,
  ): { kept: Uint8Array; length: number } | undefined {
    const data = this.pendingData.get(submit);
    if (data === undefined) return undefined;
    this.pendingData.delete(submit);
    this.pendingDataBytes -= data.kept.length;
    return data;
  }

  /** Mark a submission still waiting as unanswered, and say why. */
  private unanswered(submit: number, at: number, why: string): void {
    this.releasePending(submit);
    const record = this.submits.get(submit);
    if (record?.outcome.kind !== "pending") return;
    record.outcome = { kind: "unanswered", at, why };
    this.counters.unanswered += 1;
    this.event(
      "submit",
      "inspector",
      `Submit #${record.id}: no answer seen (${why})`,
      this.submitRefs(record),
      [],
      true,
    );
  }

  private response(
    event: Extract<StatementTraffic, { type: "response" }>,
    at: number,
  ): void {
    const key = `${event.connection}:${event.id}`;
    const waiting = this.waiting.get(key);
    this.waiting.delete(key);
    if (waiting?.submit !== undefined) {
      const payload = this.releasePending(waiting.submit);
      const record = this.submits.get(waiting.submit);
      if (record === undefined || record.outcome.kind !== "pending") return;
      if (event.error !== undefined) {
        const local = this.isLocal(event.error);
        const error = safeError(event.error);
        const meaning = local ? null : clientRefusal(event.error.message);
        record.outcome = {
          kind: "rpc-error",
          at,
          error,
          local,
          ...(meaning === null ? {} : { meaning }),
        };
        if (local) this.counters.refusedLocally += 1;
        else this.counters.rpcErrors += 1;
        const signer = record.meta?.signer;
        if (signer !== undefined) {
          const account = this.account(signer);
          if (local) account.refusedLocally += 1;
          else account.rpcErrors += 1;
        }
        this.event(
          "submit",
          local ? "host" : "client",
          local
            ? `Submit #${record.id} refused in this tab: it submits nothing`
            : meaning !== null
              ? `Submit #${record.id}: ${meaning}`
              : `Submit #${record.id}: RPC error ${error.message ?? ""}`.trim(),
          this.submitRefs(record),
          [["Error", JSON.stringify(error)]],
          true,
        );
        return;
      }
      const answer = readSubmitAnswer(event.result);
      record.outcome = { kind: "answered", at, answer };
      this.counters.answers[answer.status] =
        (this.counters.answers[answer.status] ?? 0) + 1;
      const signer = record.meta?.signer;
      if (signer !== undefined) {
        const account = this.account(signer);
        account.answers[answer.status] =
          (account.answers[answer.status] ?? 0) + 1;
        if (wentOut(answer.status)) {
          account.wentOut += 1;
          account.wentOutDataBytes += record.meta?.dataBytes ?? 0;
        }
        if (!wentOut(answer.status) && answer.status !== "knownExpired")
          account.lastRefusal = { at, submit: record.id, answer };
      }
      if (record.meta !== null) {
        if (wentOut(answer.status))
          for (const topic of record.meta.topics)
            this.topic(topic).wentOut += 1;
        if (wentOut(answer.status))
          this.seen(
            record.meta,
            "broadcast",
            at,
            { submit: record.id },
            payload,
          );

        if (
          answer.reason === "channelPriorityTooLow" &&
          record.meta.channel !== undefined &&
          signer !== undefined
        ) {
          const channel = this.channels.get(`${signer}:${record.meta.channel}`);
          if (channel !== undefined) channel.priorityRefusals += 1;
        }
      }
      const failed = !wentOut(answer.status);
      this.event(
        "submit",
        "client",
        answer.status === "new"
          ? `Submit #${record.id}: broadcast (new): at least one peer took it; not shown stored`
          : `Submit #${record.id}: answered ${answer.status}${answer.reason === undefined ? "" : ` (${answer.reason})`}`,
        this.submitRefs(record),
        [
          ["Answer", answer.shape],
          ...(wentOut(answer.status)
            ? ([
                [
                  "Note",
                  "Peers check allowance, account room and channel priority themselves, and their refusals do not reach this tab.",
                ],
              ] as [string, string][])
            : []),
          ...(answer.status === "knownExpired"
            ? ([
                [
                  "Note",
                  "knownExpired: a full node knew this statement and it has expired. The core counts it as a failure; the host's invitation delivery counts it as posted.",
                ],
              ] as [string, string][])
            : []),
        ],
        failed,
      );
      return;
    }
    if (waiting?.subscription !== undefined && waiting.method === SUBSCRIBE) {
      const record = this.subscriptions.get(waiting.subscription);
      if (record === undefined) return;
      if (event.error !== undefined) {
        record.state = "refused";
        record.endedAt = at;
        record.endedHow = "refused";
        record.error = JSON.stringify(safeError(event.error));
        this.counters.subscriptionsRefused += 1;
        this.event(
          "subscription",
          "client",
          `Subscription S${record.id} refused: ${record.error}`,
          { subscription: record.id },
          [],
          true,
        );
        return;
      }
      const remote = subscriptionKey(JSON.stringify(event.result));
      const orphan = this.remotes.get(`${event.connection}:${remote}`);
      if (orphan !== undefined && orphan !== record.id)
        this.adopt(record, orphan);
      record.remote = remote;
      record.openedAt = at;
      if (record.state === "opening") record.state = "open";
      this.remotes.set(`${event.connection}:${remote}`, record.id);
      this.event(
        "subscription",
        "client",
        `Subscription S${record.id} opened`,
        { subscription: record.id },
        [["Node id", nodeText(remote, 80)]],
      );
    }
  }

  /** Pages that arrived before the answer that named their subscription join it. */
  private adopt(record: SubscriptionRecord, orphanId: number): void {
    const orphan = this.subscriptions.get(orphanId);
    if (orphan === undefined) return;
    record.pages += orphan.pages;
    record.statements += orphan.statements;
    record.unrecognisedPages += orphan.unrecognisedPages;
    record.duplicates += orphan.duplicates;
    for (const delivery of ["initial", "later", "unclassified"] as const)
      record.byDelivery[delivery] += orphan.byDelivery[delivery];
    record.hashes.push(...orphan.hashes);
    record.sawRemaining ||= orphan.sawRemaining;
    record.phase = orphan.phase;
    if (orphan.lastRemaining !== undefined)
      record.lastRemaining = orphan.lastRemaining;
    for (const hash of orphan.hashes) {
      const statement = this.statements.get(hash);
      if (statement === undefined) continue;
      statement.subscriptions = statement.subscriptions.map((id) =>
        id === orphanId ? record.id : id,
      );
    }
    this.subscriptions.delete(orphanId);
  }

  private notification(
    event: Extract<StatementTraffic, { type: "notification" }>,
    at: number,
  ): void {
    const remoteKey = `${event.connection}:${event.subscription}`;
    let id = this.remotes.get(remoteKey);
    if (id === undefined) {
      // Its subscription was dropped from the list while open, or its
      // answer has not been seen yet: keep the pages, and say which.
      const dropped = this.droppedOpen.delete(remoteKey);
      const record: SubscriptionRecord = {
        id: this.nextSubscription++,
        source: this.sourceOf(event.connection),
        connection: event.connection,
        filter: {
          kind: "unrecognised",
          shape: dropped
            ? "dropped from the list while open: its filter and earlier counts are gone"
            : "not seen: the page came first",
        },
        requestedAt: at,
        openedAt: at,
        remote: event.subscription,
        state: "open",
        phase: "waiting",
        sawRemaining: false,
        pages: 0,
        unrecognisedPages: 0,
        statements: 0,
        byDelivery: { initial: 0, later: 0, unclassified: 0 },
        duplicates: 0,
        hashes: [],
      };
      this.subscriptions.set(record.id, record);
      this.remotes.set(remoteKey, record.id);
      this.boundSubscriptions();
      id = record.id;
    }
    const record = this.subscriptions.get(id);
    if (record === undefined) return;
    record.pages += 1;
    this.counters.pages += 1;
    const page = readPage(event.result);
    if (page.kind === "unrecognised") {
      record.unrecognisedPages += 1;
      this.event(
        "page",
        "client",
        `Subscription S${record.id}: a notification this view does not read`,
        { subscription: record.id },
        [["Shape", page.shape]],
      );
      return;
    }
    let delivery: Delivery;
    if (page.remaining !== undefined) {
      delivery = "initial";
      record.sawRemaining = true;
      record.lastRemaining = page.remaining;
      record.phase = page.remaining === 0 ? "later" : "initial";
    } else if (record.sawRemaining && record.phase === "later")
      delivery = "later";
    else {
      delivery = "unclassified";
      if (!record.sawRemaining) record.phase = "unknown";
    }
    const hashes: string[] = [];
    const topics = new Set<string>();
    const signers = new Set<string>();
    for (const item of page.statements) {
      const read = readStatementHex(item);
      if (read === null) continue;
      record.statements += 1;
      record.byDelivery[delivery] += 1;
      this.counters.delivered += 1;
      if (record.hashes.includes(read.meta.hash)) {
        record.duplicates += 1;
        this.counters.duplicates += 1;
      } else {
        record.hashes.push(read.meta.hash);
        if (record.hashes.length > HASHES_PER_SUBSCRIPTION)
          record.hashes.shift();
      }
      hashes.push(read.meta.hash);
      for (const topic of read.meta.topics) topics.add(topic);
      if (read.meta.signer !== undefined) signers.add(read.meta.signer);
      this.seen(
        read.meta,
        delivery,
        at,
        { subscription: record.id },
        this.capturing && read.data !== null
          ? { kept: read.data, length: read.data.length }
          : undefined,
      );
    }
    const where =
      page.remaining === undefined
        ? delivery === "later"
          ? "after the initial batch: what peers sent, new or long stored"
          : record.sawRemaining
            ? "before the initial batch ended, with no remaining count"
            : "no remaining count: cannot tell where in the stream"
        : page.statements.length === 0 && page.remaining === 0
          ? "the initial batch, empty: the light client holds no statements"
          : `initial batch, at least ${page.remaining} more to follow`;
    this.event(
      "page",
      "client",
      page.statements.length === 0
        ? `Subscription S${record.id}: an empty page (${where})`
        : `Subscription S${record.id}: ${hashes.length} statement${hashes.length === 1 ? "" : "s"} (${where})`,
      {
        subscription: record.id,
        topics: [...topics],
        accounts: [...signers],
        ...(hashes.length === 1 ? { hash: hashes[0] } : {}),
      },
      hashes.slice(0, 8).map((hash) => ["Statement", hash]),
    );
  }

  private closed(
    event: Extract<StatementTraffic, { type: "closed" }>,
    at: number,
  ): void {
    const connection = this.connections.get(event.connection);
    if (connection !== undefined) {
      connection.closedAt = at;
      connection.closedBy = event.by;
      if (event.reason !== undefined)
        connection.reason = nodeText(redactUrls(event.reason));
    }
    for (const [key, waiting] of [...this.waiting]) {
      if (!key.startsWith(`${event.connection}:`)) continue;
      this.waiting.delete(key);
      if (waiting.submit !== undefined)
        this.unanswered(
          waiting.submit,
          at,
          "the connection closed before an answer came",
        );
    }
    for (const record of this.subscriptions.values())
      if (
        record.connection === event.connection &&
        (record.state === "open" || record.state === "opening")
      )
        this.endSubscription(record.id, at, "connection closed");
    this.event(
      "connection",
      "inspector",
      `Connection ${event.connection} closed (${event.by === "caller" ? "by its opener" : event.by === "ended" ? "its answers ended" : "reading failed"})`,
      {},
      event.reason === undefined
        ? []
        : [["Reason", nodeText(redactUrls(event.reason))]],
      event.by === "failed",
    );
  }

  private endSubscription(
    id: number,
    at: number,
    how: "unsubscribed" | "connection closed",
  ): void {
    const record = this.subscriptions.get(id);
    if (record === undefined || record.state === "ended") return;
    record.state = "ended";
    record.endedAt = at;
    record.endedHow = how;
    this.event(
      "subscription",
      "inspector",
      `Subscription S${id} ended: ${how}`,
      { subscription: id },
      [],
    );
  }

  private submitRefs(record: SubmitRecord): EventRefs {
    return {
      submit: record.id,
      ...(record.meta?.signer === undefined
        ? {}
        : { accounts: [record.meta.signer] }),
      ...(record.meta === null
        ? {}
        : { hash: record.meta.hash, topics: record.meta.topics }),
    };
  }

  /** Record a statement this tab broadcast, or its connection delivered. */
  private seen(
    meta: StatementMeta,
    via: Sighting,
    at: number,
    from: { submit?: number; subscription?: number },
    data: { kept: Uint8Array; length: number } | undefined,
  ): void {
    let statement = this.statements.get(meta.hash);
    if (statement === undefined) {
      statement = {
        meta,
        firstAt: at,
        lastAt: at,
        sightings: 0,
        via: [],
        subscriptions: [],
        submits: [],
      };
      this.statements.set(meta.hash, statement);
      this.bound(this.statements, this.limits.statements, "statements");
      for (const topic of meta.topics) this.topic(topic).statements += 1;
      if (meta.signer !== undefined) this.account(meta.signer);
      if (meta.signer !== undefined && meta.channel !== undefined)
        this.channel(meta, meta.signer, meta.channel);
    }
    statement.lastAt = at;
    for (const topic of meta.topics) this.topic(topic).lastAt = at;
    statement.sightings += 1;
    push(statement.via, via);
    if (from.submit !== undefined) push(statement.submits, from.submit);
    if (from.subscription !== undefined)
      push(statement.subscriptions, from.subscription);
    if (this.capturing && data !== undefined) this.keepPayload(statement, data);
  }

  private keepPayload(
    statement: ObservedStatement,
    data: { kept: Uint8Array; length: number },
  ): void {
    if (statement.payload !== undefined) return;
    const kept = data.kept.subarray(0, this.limits.payloadPerStatement);
    if (
      this.payloadBytes + this.pendingDataBytes + kept.length >
      this.limits.payloadBytes
    ) {
      this.payloadFull = true;
      return;
    }
    this.payloadBytes += kept.length;
    statement.payload = bytesToHex(kept);
    if (kept.length < data.length) statement.payloadCut = true;
  }

  private channel(
    meta: StatementMeta,
    signer: string,
    channelId: string,
  ): void {
    const key = `${signer}:${channelId}`;
    const expiry = BigInt(meta.expiry ?? "0");
    const known = this.channels.get(key);
    if (known === undefined) {
      this.channels.set(key, {
        signer,
        channel: channelId,
        latest: meta.hash,
        latestExpiry: expiry.toString(),
        replacements: 0,
        older: 0,
        priorityRefusals: 0,
        history: [meta.hash],
      });
      this.bound(this.channels, this.limits.channels, "channels");
      return;
    }
    if (known.latest === meta.hash) return;
    known.history.push(meta.hash);
    if (known.history.length > HISTORY) known.history.shift();
    if (expiry > BigInt(known.latestExpiry)) {
      known.latest = meta.hash;
      known.latestExpiry = expiry.toString();
      known.replacements += 1;
    } else known.older += 1;
  }

  // --- the host's and the core's facts ------------------------------------

  /**
   * Run a claim the host asks the core for, and record how it ended. The
   * caller gets exactly what `run` gave, answer or error: recording never
   * changes it. A claim that ends after a session change or a clear is not
   * recorded.
   */
  async claim<T extends ClaimAnswer>(
    request: ClaimRequest,
    run: () => Promise<T>,
  ): Promise<T> {
    const epoch = this.epochValue;
    let answer: T;
    try {
      answer = await run();
    } catch (error) {
      this.watch(epoch, () =>
        this.claimed(
          request,
          null,
          error instanceof Error ? error.message : String(error),
        ),
      );
      throw error;
    }
    this.watch(epoch, () => this.claimed(request, answer, null));
    return answer;
  }

  /** Record something, unless the session changed since `epoch`. Recording never throws. */
  private watch(epoch: number, record: () => void): void {
    if (epoch !== this.epochValue) return;
    try {
      record();
    } catch {
      // The inspector only watches.
    }
  }

  private claimed(
    request: ClaimRequest,
    answer: ClaimAnswer | null,
    reason: string | null,
  ): void {
    const at = this.now();
    const outcome: ClaimRecord["outcome"] =
      answer === null
        ? "failed"
        : answer.outcome === "registered" || answer.outcome === "validated"
          ? answer.outcome
          : answer.outcome === "alreadyAllocated" ||
              answer.outcome === "alreadyRegistered"
            ? "alreadyAllocated"
            : "failed";
    const claimed =
      request.kind === "notification" ? request.account : answer?.account;
    const record: ClaimRecord = {
      at,
      kind: request.kind,
      submit: request.submit,
      outcome,
      ...(request.kind === "product"
        ? { product: request.product, index: request.index }
        : {}),
      ...(claimed === undefined ? {} : { account: claimed.toLowerCase() }),
      ...(answer === null
        ? {}
        : {
            period: answer.period,
            seq: answer.seq,
            ...(answer.collection === undefined || answer.collection === ""
              ? {}
              : { collection: answer.collection }),
            ...(answer.blockHash === undefined
              ? {}
              : { blockHash: answer.blockHash }),
          }),
      ...(reason === null ? {} : { reason: nodeText(redactUrls(reason)) }),
      ...(answer !== null && outcome === "failed"
        ? { reason: `the core answered ${answer.outcome}` }
        : {}),
    };
    this.counters.claims[outcome] = (this.counters.claims[outcome] ?? 0) + 1;
    if (record.account !== undefined) {
      const account = this.account(record.account);
      account.claims.push(record);
      if (account.claims.length > CLAIMS_PER_ACCOUNT) account.claims.shift();
      if (request.kind === "product")
        this.role(account, {
          kind: "product-account",
          product: request.product,
          index: request.index,
        });
      else this.role(account, { kind: "notification-account" });
    }
    const what =
      request.kind === "product"
        ? `${request.product}'s account at Raw(${short(request.index, 4)})`
        : `notification account ${short(request.account)}`;
    const words: Record<ClaimRecord["outcome"], string> = {
      registered: "got a slot now",
      alreadyAllocated: "already held a slot this period",
      validated:
        "would be accepted: validation only, nothing was submitted or allocated",
      failed: "got no slot",
    };
    this.event(
      "claim",
      "core",
      `Claim: ${what} ${words[outcome]}${record.reason === undefined ? "" : `: ${record.reason}`}`,
      {
        ...(record.account === undefined ? {} : { accounts: [record.account] }),
        ...(record.product === undefined ? {} : { product: record.product }),
      },
      claimDetails(record),
      outcome === "failed",
    );
    this.notify();
  }

  /** What the request service and its delivery report. */
  readonly invitations = (event: InvitationEvent): void => {
    this.onInvitation(cleanInvitation(event));
    this.notify();
  };

  private onInvitation(event: InvitationEvent): void {
    const at = this.now();
    if (event.type === "delivery-account") {
      const account = this.account(event.account);
      this.role(account, {
        kind: "notification-account",
        post: event.post,
        period: event.period,
      });
      const request = this.requests.get(event.post);
      if (request !== undefined) {
        push(request.notificationAccounts, event.account);
        if (request.notificationAccounts.length > HISTORY)
          request.notificationAccounts.shift();
      }
      this.event(
        "request",
        "host",
        `Delivery: post ${short(event.post, 4)} goes out as notification account ${short(event.account)}, day ${event.period}`,
        { handle: event.post, accounts: [event.account] },
        [],
      );
      return;
    }
    if (event.type === "delivery-problem") {
      const request = this.requests.get(event.post);
      if (request !== undefined) {
        if (event.problem === null) delete request.problem;
        else request.problem = event.problem;
      }
      this.event(
        "request",
        "host",
        event.problem === null
          ? `Delivery: post ${short(event.post, 4)} is up again`
          : `Delivery: post ${short(event.post, 4)} cannot be kept up: ${event.problem}`,
        { handle: event.post },
        [],
        event.problem !== null,
      );
      return;
    }
    const request = this.requestFor(event.handle, event.product, at);
    request.lastAt = at;
    switch (event.type) {
      case "request-sent":
        request.role = "sender";
        request.state = "sent";
        break;
      case "request-received":
        request.role = "recipient";
        request.state = "received";
        break;
      case "reply-authenticated":
        request.role = "sender";
        request.state = "reply authenticated";
        break;
      case "replied":
        request.role = "recipient";
        request.state = "replied";
        break;
      case "request-closed":
        request.state = event.how;
        break;
      case "request-paused":
        if (event.problem === null) delete request.problem;
        else request.problem = event.problem;
        break;
      case "chat-account":
        request.index = event.index;
        if (event.outcome === "refused") request.refusal = event.reason;
        else {
          delete request.refusal;
          if (event.account !== undefined) {
            request.account = event.account.toLowerCase();
            this.role(this.account(request.account), {
              kind: "chat-allowance",
              product: event.product,
              handle: event.handle,
            });
          }
          if (event.until !== undefined) request.until = event.until;
        }
        break;
    }
    this.event(
      event.type === "chat-account" ? "claim" : "request",
      "host",
      invitationText(event),
      {
        handle: event.handle,
        product: event.product,
        ...(event.type === "chat-account" && event.account !== undefined
          ? { accounts: [event.account.toLowerCase()] }
          : {}),
      },
      event.type === "chat-account"
        ? [
            ["Index", event.index],
            ...(event.until === undefined
              ? []
              : ([["Until", new Date(event.until).toISOString()]] as [
                  string,
                  string,
                ][])),
          ]
        : [],
      (event.type === "chat-account" && event.outcome === "refused") ||
        (event.type === "request-paused" && event.problem !== null),
    );
  }

  private requestFor(
    handle: string,
    product: string,
    at: number,
  ): RequestRecord {
    let record = this.requests.get(handle);
    if (record === undefined) {
      record = {
        handle,
        product,
        role: "unknown",
        state: "seen",
        firstAt: at,
        lastAt: at,
        notificationAccounts: [],
      };
      this.requests.set(handle, record);
      this.bound(this.requests, this.limits.requests, "requests");
    }
    return record;
  }

  /** A worker run's diagnostics report, or null when that worker stopped. */
  readonly diagnostics = (
    worker: { name: string; run: number },
    report: DiagnosticsReport | null,
  ): void => {
    const at = this.now();
    if (report === null) {
      let any = false;
      for (const record of this.conversations.values())
        if (record.product === worker.name && !record.stale) {
          record.stale = true;
          any = true;
        }
      if (any) {
        this.event(
          "product",
          "host",
          `${worker.name} stopped: its last report no longer holds`,
          { product: worker.name },
          [],
        );
        this.notify();
      }
      return;
    }
    this.counters.reports += 1;
    const keys = new Set<string>();
    for (const conversation of report.conversations) {
      const key = `${worker.name}:${conversation.id}`;
      keys.add(key);
      const known = this.conversations.get(key);
      const changed =
        known === undefined ||
        known.stale ||
        JSON.stringify(structural(known.report)) !==
          JSON.stringify(structural(conversation));
      // The cost window starts again with a new run or a new account.
      const sameWindow =
        known !== undefined &&
        known.run === worker.run &&
        known.report.account === conversation.account;
      // The account's record exists before the window starts, so a record
      // dropped and made again later is told apart by its sequence.
      if (conversation.account !== undefined)
        this.role(this.account(conversation.account), {
          kind: "conversation-account",
          product: worker.name,
          conversation: conversation.id,
          protocol: conversation.protocol,
        });
      this.conversations.delete(key);
      this.conversations.set(key, {
        key,
        product: worker.name,
        run: worker.run,
        reportedAt: at,
        report: conversation,
        stale: false,
        baseline: sameWindow
          ? known.baseline
          : this.baselineFor(conversation, at),
      });
      this.bound(
        this.conversations,
        this.limits.conversations,
        "conversations",
      );
      for (const peer of conversation.peers ?? [])
        this.role(this.account(peer), {
          kind: "conversation-peer",
          product: worker.name,
          conversation: conversation.id,
        });
      for (const topic of [
        ...(conversation.listen ?? []),
        ...(conversation.post?.topics ?? []),
      ])
        this.topic(topic);
      if (changed)
        this.event(
          "product",
          "product",
          `${worker.name} reports conversation ${short(conversation.id, 4)}: ${conversation.protocol}${conversation.state === undefined ? "" : `, ${conversation.state}`}`,
          {
            product: worker.name,
            conversation: key,
            ...(conversation.request === undefined
              ? {}
              : { handle: conversation.request }),
            accounts: [
              ...(conversation.account === undefined
                ? []
                : [conversation.account]),
              ...(conversation.peers ?? []),
            ],
            topics: [
              ...(conversation.listen ?? []),
              ...(conversation.post?.topics ?? []),
            ],
          },
          [],
        );
    }
    for (const [key, record] of this.conversations)
      if (record.product === worker.name && !keys.has(key)) record.stale = true;
    this.notify();
  };

  private baselineFor(
    conversation: ConversationDiagnostics,
    at: number,
  ): CostBaseline {
    const account =
      conversation.account === undefined
        ? undefined
        : this.accounts.get(conversation.account);
    return {
      at,
      ...(conversation.account === undefined
        ? {}
        : { account: conversation.account }),
      accountSeq: account?.seq ?? null,
      attempts: account?.submits ?? 0,
      wentOut: account?.wentOut ?? 0,
      wentOutDataBytes: account?.wentOutDataBytes ?? 0,
      sent: conversation.logical?.sent ?? 0,
    };
  }

  /**
   * What `record`'s conversation spent since its baseline: attempts and
   * submissions that went out (answered `new` or `known`) the host saw signed by its account, against the
   * messages the product reports sent, over the same window. Not known when
   * the window has no account, the host holds nothing tying the account to
   * the product, no message was sent yet, or the ledger dropped the
   * account's record since the window started.
   */
  cost(record: ConversationRecord): Cost {
    const { baseline } = record;
    if (baseline.account === undefined)
      return { known: false, why: "the product reports no account" };
    const account = this.accounts.get(baseline.account);
    if (account === undefined || account.seq !== baseline.accountSeq)
      return {
        known: false,
        why: "the ledger dropped its account's record since the window started",
      };
    if (!account.owners.includes(record.product))
      return {
        known: false,
        why: `the host holds no claim or allowance tying its account to ${record.product}`,
      };
    const sent = (record.report.logical?.sent ?? 0) - baseline.sent;
    if (sent <= 0)
      return { known: false, why: "no message sent since the window started" };
    return {
      known: true,
      since: baseline.at,
      attempts: account.submits - baseline.attempts,
      wentOut: account.wentOut - baseline.wentOut,
      wentOutDataBytes: account.wentOutDataBytes - baseline.wentOutDataBytes,
      sent,
    };
  }

  /**
   * Record a grant read made at `block`, which started at `epoch`. Answers
   * false, recording nothing, when the session changed since.
   */
  grants(
    epoch: number,
    block: string,
    values: {
      account: string;
      value: { maxCount: number; maxSize: number } | null;
    }[],
  ): boolean {
    if (epoch !== this.epochValue) return false;
    const at = this.now();
    this.counters.grantReads += 1;
    for (const { account, value } of values)
      this.account(account).grant = { at, block, value };
    this.event(
      "grant",
      "chain",
      `Read the grants of ${values.length} account${values.length === 1 ? "" : "s"} at block ${short(block)}`,
      { accounts: values.map(({ account }) => account) },
      values.map(({ account, value }) => [
        short(account),
        value === null
          ? "no allowance stored"
          : `${value.maxCount} statements, ${value.maxSize} bytes`,
      ]),
    );
    this.notify();
    return true;
  }

  /** Record a grant read, started at `epoch`, that failed. As {@link grants}. */
  grantsFailed(epoch: number, accounts: string[], reason: string): boolean {
    if (epoch !== this.epochValue) return false;
    const at = this.now();
    const text = nodeText(redactUrls(reason));
    for (const account of accounts)
      this.account(account).grant = { at, error: text };
    this.event(
      "grant",
      "chain",
      `Grant read failed: ${text}`,
      { accounts },
      [],
      true,
    );
    this.notify();
    return true;
  }

  // --- reading ---------------------------------------------------------------

  sessionStart(): number {
    return this.sessionStarted;
  }

  counts(): Readonly<Counters> {
    return this.counters;
  }

  /** How many of each kind were dropped to keep the limits, since the session started. */
  dropped(): Readonly<Record<string, number>> {
    return this.evicted;
  }

  /** Captured bytes kept, those waiting for the node's answer included. */
  payloadUse(): { bytes: number; limit: number; full: boolean } {
    return {
      bytes: this.payloadBytes + this.pendingDataBytes,
      limit: this.limits.payloadBytes,
      full: this.payloadFull,
    };
  }

  eventList(): readonly LedgerEvent[] {
    return this.events;
  }

  connectionList(): Connection[] {
    return [...this.connections.values()];
  }

  submitList(): SubmitRecord[] {
    return [...this.submits.values()];
  }

  subscriptionList(): SubscriptionRecord[] {
    return [...this.subscriptions.values()];
  }

  statement(hash: string): ObservedStatement | undefined {
    return this.statements.get(hash);
  }

  statementList(): ObservedStatement[] {
    return [...this.statements.values()];
  }

  channelList(signer?: string): ChannelRecord[] {
    const all = [...this.channels.values()];
    return signer === undefined ? all : all.filter((c) => c.signer === signer);
  }

  accountList(): AccountRecord[] {
    return [...this.accounts.values()];
  }

  accountRecord(account: string): AccountRecord | undefined {
    return this.accounts.get(account.toLowerCase());
  }

  topicList(): TopicRecord[] {
    return [...this.topics.values()];
  }

  topicRecord(topic: string): TopicRecord | undefined {
    return this.topics.get(topic.toLowerCase());
  }

  conversationList(): ConversationRecord[] {
    return [...this.conversations.values()];
  }

  requestList(): RequestRecord[] {
    return [...this.requests.values()];
  }

  requestRecord(handle: string): RequestRecord | undefined {
    return this.requests.get(handle);
  }

  /**
   * `account`'s latest observed statements: per channel the one with the
   * highest expiry this tab saw, and every one without a channel, whose
   * expiry has not passed, this tab's own broadcasts included. An
   * observation, not occupancy: peers may have refused or evicted any of
   * them without saying so, and may hold others no one here was sent. It is
   * neither a lower nor an upper bound of what the network holds now.
   * `dropped` counts channels whose latest statement left the ledger's list.
   */
  latestObserved(account: string): LatestObserved {
    const signer = account.toLowerCase();
    const nowSeconds = Math.floor(this.now() / 1000);
    const live = (meta: StatementMeta) =>
      meta.expiresAt === undefined ||
      meta.expiresAt === null ||
      meta.expiresAt >= nowSeconds;
    const observed: LatestObserved = {
      statements: 0,
      dataBytes: 0,
      expired: 0,
      lastObservedAt: null,
      dropped: 0,
    };
    const count = (statement: ObservedStatement) => {
      if (!live(statement.meta)) {
        observed.expired += 1;
        return;
      }
      observed.statements += 1;
      observed.dataBytes += statement.meta.dataBytes;
      observed.lastObservedAt = Math.max(
        observed.lastObservedAt ?? 0,
        statement.lastAt,
      );
    };
    for (const channel of this.channels.values()) {
      if (channel.signer !== signer) continue;
      const latest = this.statements.get(channel.latest);
      if (latest !== undefined) count(latest);
      else observed.dropped += 1;
    }
    for (const statement of this.statements.values())
      if (
        statement.meta.signer === signer &&
        statement.meta.channel === undefined
      )
        count(statement);
    return observed;
  }

  // --- helpers ------------------------------------------------------------------

  private account(account: string): AccountRecord {
    const key = account.toLowerCase();
    let record = this.accounts.get(key);
    if (record === undefined) {
      record = {
        account: key,
        seq: this.nextAccountSeq++,
        firstAt: this.now(),
        owners: [],
        roles: [],
        claims: [],
        submits: 0,
        wentOut: 0,
        wentOutDataBytes: 0,
        answers: {},
        rpcErrors: 0,
        refusedLocally: 0,
      };
      this.accounts.set(key, record);
      this.bound(this.accounts, this.limits.accounts, "accounts");
    }
    return record;
  }

  /** Whether the host or the core tied `account` to `product`. */
  owns(product: string, account: string): boolean {
    return (
      this.accounts.get(account.toLowerCase())?.owners.includes(product) ===
      true
    );
  }

  private own(account: AccountRecord, product: string): void {
    if (account.owners.includes(product)) return;
    account.owners.push(product);
    if (account.owners.length > ROLES_PER_ACCOUNT) account.owners.shift();
  }

  private role(account: AccountRecord, role: Role): void {
    if (role.kind === "product-account" || role.kind === "chat-allowance")
      this.own(account, role.product);
    const text = JSON.stringify(role);
    if (account.roles.some((known) => JSON.stringify(known) === text)) return;
    // A claim names a notification account; the delivery adds its post and day.
    if (role.kind === "notification-account") {
      const notification = account.roles.some(
        (known) => known.kind === "notification-account",
      );
      if (role.post === undefined && notification) return;
      account.roles = account.roles.filter(
        (known) =>
          known.kind !== "notification-account" || known.post !== undefined,
      );
    }
    account.roles.push(role);
    if (account.roles.length > ROLES_PER_ACCOUNT) {
      // Products' reports go first, so they cannot push out what the host derived.
      const reported = account.roles.findIndex((known) => !derivedHere(known));
      account.roles.splice(Math.max(reported, 0), 1);
    }
  }

  private topic(topic: string): TopicRecord {
    const key = topic.toLowerCase();
    let record = this.topics.get(key);
    if (record === undefined) {
      record = {
        topic: key,
        firstAt: this.now(),
        statements: 0,
        submits: 0,
        wentOut: 0,
        subscriptions: [],
      };
      this.topics.set(key, record);
      this.bound(this.topics, this.limits.topics, "topics");
    }
    return record;
  }

  /** Drop the oldest entries of `map` past `limit`, counting them. */
  private bound<K, V>(map: Map<K, V>, limit: number, name: string): void {
    while (map.size > limit) {
      const oldest = map.keys().next().value as K;
      map.delete(oldest);
      this.evicted[name] = (this.evicted[name] ?? 0) + 1;
    }
  }

  /** Drop the oldest subscriptions past the limit, and the node ids that name them. */
  /**
   * Drop subscriptions past the limit: ended and refused ones first, oldest
   * first; only then open ones, whose node ids are remembered so that a later
   * page says its subscription's earlier counts were dropped.
   */
  private boundSubscriptions(): void {
    while (this.subscriptions.size > this.limits.subscriptions) {
      const done = [...this.subscriptions.values()].find(
        (record) => record.state === "ended" || record.state === "refused",
      );
      const record =
        done ??
        this.subscriptions.get(
          this.subscriptions.keys().next().value as number,
        )!;
      this.subscriptions.delete(record.id);
      const name = done === undefined ? "open subscriptions" : "subscriptions";
      this.evicted[name] = (this.evicted[name] ?? 0) + 1;
      for (const [key, id] of [...this.remotes])
        if (id === record.id) {
          this.remotes.delete(key);
          if (done === undefined) {
            this.droppedOpen.add(key);
            if (this.droppedOpen.size > this.limits.subscriptions)
              this.droppedOpen.delete(
                this.droppedOpen.values().next().value as string,
              );
          }
        }
    }
  }

  /** Drop connections past the limit: closed ones first, oldest first. */
  private boundConnections(): void {
    while (this.connections.size > this.limits.connections) {
      const closed = [...this.connections.values()].find(
        (connection) => connection.closedAt !== undefined,
      );
      this.connections.delete(
        closed?.id ?? (this.connections.keys().next().value as number),
      );
      this.evicted.connections = (this.evicted.connections ?? 0) + 1;
    }
  }

  private isLocal(error: RpcErrorValue): boolean {
    try {
      return this.localRefusal(error);
    } catch {
      return false;
    }
  }

  private event(
    kind: EventKind,
    source: FactSource,
    text: string,
    refs: EventRefs,
    details: [string, string][],
    failed = false,
  ): void {
    this.events.push({
      id: this.nextEvent++,
      at: this.now(),
      kind,
      source,
      text: redactUrls(text),
      ...(failed ? { failed } : {}),
      refs,
      details: details.map(([label, value]) => [label, redactUrls(value)]),
    });
    if (this.events.length > this.limits.events) {
      const over = this.events.length - this.limits.events;
      this.events.splice(0, over);
      this.evicted.events = (this.evicted.events ?? 0) + over;
    }
  }
}

/**
 * An RPC error as the ledger keeps it: its code, its message with long hex
 * replaced, and its data only as a shape, since it may carry anything.
 */
function safeError(error: RpcErrorValue): RpcErrorValue {
  return {
    ...(error.code === undefined ? {} : { code: error.code }),
    ...(error.message === undefined
      ? {}
      : { message: nodeText(redactUrls(error.message)) }),
    ...(error.data === undefined ? {} : { data: shapeOf(error.data) }),
  };
}

/** `event` with its free text (problems and reasons) redacted and bounded as any other text the ledger keeps. */
function cleanInvitation(event: InvitationEvent): InvitationEvent {
  const clean = (text: string) => nodeText(redactUrls(text));
  switch (event.type) {
    case "delivery-problem":
    case "request-paused":
      return event.problem === null
        ? event
        : { ...event, problem: clean(event.problem) };
    case "chat-account":
      return event.reason === undefined
        ? event
        : { ...event, reason: clean(event.reason) };
    default:
      return event;
  }
}

/** A report without its counts, to tell a change of state from a count going up. */
function structural(report: ConversationDiagnostics): unknown {
  return { ...report, logical: undefined, facts: undefined };
}

export function filterText(filter: FilterView): string {
  switch (filter.kind) {
    case "any":
      return "every statement";
    case "matchAll":
      return `every one of ${filter.topics.length} topic${filter.topics.length === 1 ? "" : "s"}`;
    case "matchAny":
      return `any of ${filter.topics.length} topic${filter.topics.length === 1 ? "" : "s"}`;
    case "unrecognised":
      return `filter ${filter.shape}`;
  }
}

export function metaDetails(meta: StatementMeta): [string, string][] {
  return [
    ["Hash", meta.hash],
    [
      "Proof",
      meta.signer === undefined ? meta.proof : `${meta.proof} ${meta.signer}`,
    ],
    ["Expiry", expiryText(meta)],
    ...(meta.channel === undefined
      ? []
      : ([["Channel", meta.channel]] as [string, string][])),
    ...meta.topics.map((topic, index): [string, string] => [
      `Topic ${index + 1}`,
      topic,
    ]),
    ["Size", `${meta.dataBytes} data bytes, ${meta.encodedBytes} encoded`],
    ...(meta.decryptionKey
      ? ([["Decryption key", "named (not shown)"]] as [string, string][])
      : []),
    ...(meta.undecoded === undefined
      ? []
      : ([["Undecoded", meta.undecoded]] as [string, string][])),
  ];
}

export function expiryText(meta: StatementMeta): string {
  if (meta.expiry === undefined) return "none";
  if (meta.expiresAt === null) return `never (priority ${meta.priority})`;
  return `${new Date((meta.expiresAt ?? 0) * 1000).toISOString()} (priority ${meta.priority})`;
}

function claimDetails(record: ClaimRecord): [string, string][] {
  return [
    ["Submitted", record.submit ? "yes" : "no: validation only"],
    ...(record.account === undefined
      ? []
      : ([["Account", record.account]] as [string, string][])),
    ...(record.index === undefined
      ? []
      : ([["Index", record.index]] as [string, string][])),
    ...(record.period === undefined
      ? []
      : ([["Period", `UTC day ${record.period}`]] as [string, string][])),
    ...(record.seq === undefined
      ? []
      : ([
          [
            "Slot",
            `${record.seq}${record.collection === undefined ? "" : ` of ${record.collection}`}`,
          ],
        ] as [string, string][])),
    ...(record.blockHash === undefined
      ? []
      : ([["Block", record.blockHash]] as [string, string][])),
  ];
}

function invitationText(event: InvitationEvent): string {
  switch (event.type) {
    case "request-sent":
      return `Request ${short(event.handle, 4)} sent by the user`;
    case "request-received":
      return `Request ${short(event.handle, 4)} received and authenticated`;
    case "reply-authenticated":
      return `Request ${short(event.handle, 4)}: reply authenticated`;
    case "replied":
      return `Request ${short(event.handle, 4)}: the product replied; the host posted it`;
    case "request-closed":
      return `Request ${short(event.handle, 4)} ${event.how}`;
    case "request-paused":
      return event.problem === null
        ? `Request ${short(event.handle, 4)} posted again`
        : `Request ${short(event.handle, 4)} paused: ${event.problem}`;
    case "chat-account":
      return event.outcome === "refused"
        ? `chatAccount for request ${short(event.handle, 4)} refused: ${event.reason ?? ""}`
        : `chatAccount for request ${short(event.handle, 4)} ${event.outcome === "reused" ? "answered from this period's allowance" : "allowed"}: account ${short(event.account ?? "")}`;
    default:
      return "";
  }
}
