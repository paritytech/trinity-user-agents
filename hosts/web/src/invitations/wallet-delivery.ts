import {
  PROVISIONAL_MS,
  type UsableAllowance,
} from "../platform/allowance-usable.js";
import type { InvitationObserver } from "./observer.js";
import type { ConversationAllowance, HostDelivery } from "./ports.js";
import { StatementStoreDelivery } from "./statement-delivery.js";
import type { WalletRequestsSession } from "./wallet-chat.js";
import { bytesToHex } from "@parity/truapi/scale";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import type { ProductId } from "@parity/truapi";

/**
 * How a wallet's host requests travel, and what that spends: the request
 * lists they are kept in, the delivery that carries them, the allowance of
 * the chat accounts, and what the prompts say it costs. The host runs
 * {@link networkDelivery}. The dev server's tools may hand the wallet's chat
 * another one (`tools/demos/wallet-deliveries.ts`).
 */
export interface WalletDelivery {
  /** How requests and replies travel, for the log. */
  readonly line: string;
  /**
   * Added to the request lists' storage keys. Deliveries whose requests name
   * different return routes keep different lists.
   */
  readonly listSuffix: string;
  /** Whether claims are submitted. False: each is only validated, and nothing is submitted. */
  readonly submits: boolean;
  /**
   * Whether the wallet's saved contacts stay out of the picker, and so out
   * of payments, while a chat product runs over this delivery.
   */
  readonly hidesWalletContacts: boolean;
  /**
   * Why a chat product whose setup needs the network's own Statement Store,
   * such as Nova Chat, cannot run over this delivery, or null when it can.
   */
  readonly networkOnlyProblem: string | null;
  /** What a chat product spends and exposes, for its prompts, or null when it spends nothing. */
  costs(encrypted: boolean): ChatCosts | null;
  /** The delivery of the user whose chat public key is `self`. */
  hostDelivery(session: WalletRequestsSession, self: string): HostDelivery;
  /**
   * The allowance of a chat product's conversation accounts, or undefined
   * when this delivery gives none. `seen` is told where each account became
   * usable.
   */
  allowance(
    session: WalletRequestsSession,
    seen: AllowanceSeen | null,
  ): ConversationAllowance | undefined;
}

/** Told where a claimed account became usable, for a developer view or a test. */
export type AllowanceSeen = (usable: UsableAllowance) => void;

/** What a wallet's chat product spends of the person's allowance, and what it exposes, for the host's prompts. */
export interface ChatCosts {
  /** The one muted line both prompts show, which opens the breakdown. */
  summary: string;
  /** What sending a request spends. */
  request: string;
  /** What replying to a request spends. */
  reply: string;
  /** What the chat that follows spends. */
  chat: string;
  /** Who can read the chat's messages, or null when none are sent. */
  messages: string | null;
}

/** The chat's account and its allowance, the same in every delivery that gives one. */
export const CHAT_ACCOUNT_COST =
  "1 Statement Store slot each day you send, for this chat's own account.";

/**
 * The network delivery's request lists. Its requests name the day inbox as
 * their return route, which no other delivery reaches.
 */
export const DAY_INBOX_LISTS = ":day-inbox";

/** What a chat product spends and exposes on the network. */
export function networkCosts(encrypted: boolean): ChatCosts {
  return {
    summary: encrypted
      ? "Uses daily slots · Messages are end-to-end encrypted"
      : "Uses daily slots · Messages are public",
    request:
      "1 notification slot now, and 1 more each day it is pending while this host is open.",
    reply:
      "1 notification slot now, and 1 more on any later day their request arrives again.",
    chat: CHAT_ACCOUNT_COST,
    messages: encrypted
      ? "End-to-end encrypted. Who posts, to which topics, when and how much is public."
      : "Not encrypted: anyone can read them.",
  };
}

/** Where the network delivery logs, and who it tells what it does. */
export interface NetworkDeliveryDeps {
  log(line: string): void;
  /** Told the delivery's facts, for the inspector's Statements view. */
  observe?: InvitationObserver;
}

/**
 * The delivery over `session`'s People chain Statement Store, whose posts
 * are signed by notification accounts the core claims, or with `submit`
 * false only validates.
 */
export function statementStoreDelivery(
  session: WalletRequestsSession,
  submit: boolean,
  deps: NetworkDeliveryDeps,
): HostDelivery {
  return new StatementStoreDelivery({
    network: session.network,
    entropy: session.entropy,
    openStore: () => session.openStatementStore(),
    claim: (target, submitClaim) =>
      session.claimNotificationAccount(target, submitClaim),
    guard: session.usableAllowance,
    submit,
    now: () => Date.now(),
    schedule: (task, delayMs) => {
      const timer = setTimeout(task, Math.min(delayMs, 2 ** 31 - 1));
      return () => clearTimeout(timer);
    },
    log: deps.log,
    ...(deps.observe === undefined ? {} : { observe: deps.observe }),
  });
}

/**
 * How a wallet's chat product gets its conversation accounts an allowance:
 * a general Statement Store slot of the wallet's personhood for the UTC day,
 * which the core claims for the product's own account at the index it names.
 * With `submit` false the runtime only says whether it would accept the
 * claim and nothing is submitted, so a message fails and says so.
 *
 * Product-signed posts use the network's guarded rule. Only a known inclusion block proved
 * on finalized ancestry grants a full-day lease. An unknown block proves
 * allowance alone and grants at most {@link PROVISIONAL_MS}.
 */
export function conversationAllowance(
  session: WalletRequestsSession,
  submit: boolean,
  seen: AllowanceSeen | null,
  now: () => number = Date.now,
): ConversationAllowance {
  const claims = new Map<string, Promise<StatementAccountClaim>>();
  const deadlines = new Map<string, number>();
  const reading = new Map<string, ReturnType<ConversationAllowance["allow"]>>();
  return {
    allow(product, index) {
      const period = Math.floor(now() / 86_400_000);
      const key = `${product}:${bytesToHex(index)}:${period}`;
      const current = reading.get(key);
      if (current !== undefined) return current;
      const pending = read(product, index, key).finally(() =>
        reading.delete(key),
      );
      reading.set(key, pending);
      return pending;
    },
  };

  async function read(product: ProductId, index: Uint8Array, key: string) {
    let pending = claims.get(key);
    if (pending === undefined) {
      pending = session.claimProductStatementAccount(product, index, submit);
      claims.set(key, pending);
    }
    const claim = await pending;
    if (claim.outcome === "validated")
      throw new Error(
        `This tab only validates claims: the runtime would give this chat's account ${claim.account} slot ${claim.seq} of ${claim.collection}, and nothing was claimed.`,
      );
    const deadline = deadlines.get(key);
    const usable = await session.usableAllowance(
      claim,
      deadline !== undefined ? { wait: "finalized" } : undefined,
    );
    if (!usable.running() || Math.floor(now() / 86_400_000) !== claim.period)
      throw new Error(
        "The session or allowance day ended. Retry in the current session.",
      );
    seen?.(usable);
    const dayEnd = (claim.period + 1) * 86_400_000;
    const provisional = !usable.claimFinal;
    if (
      provisional &&
      (deadline === undefined || usable.usableAt === "finalized")
    )
      deadlines.set(key, Math.min(dayEnd, now() + PROVISIONAL_MS));
    if (provisional && (deadlines.get(key) ?? 0) <= now())
      throw new Error(
        "The claim's inclusion is still unconfirmed. Retry later; no new slot was claimed.",
      );
    return {
      account: claim.account,
      until: provisional ? deadlines.get(key)! : dayEnd,
      ...(provisional ? { provisional: true as const } : {}),
    };
  }
}

/**
 * The network delivery: requests and replies go on the network's day inbox,
 * each post claims a notification slot, and each conversation account claims
 * a Statement Store slot.
 */
export function networkDelivery(deps: NetworkDeliveryDeps): WalletDelivery {
  return {
    line: "requests and replies go on the network's day inbox, and each post claims a notification slot.",
    listSuffix: DAY_INBOX_LISTS,
    submits: true,
    hidesWalletContacts: true,
    networkOnlyProblem: null,
    costs: networkCosts,
    hostDelivery: (session) => statementStoreDelivery(session, true, deps),
    allowance: (session, seen) => conversationAllowance(session, true, seen),
  };
}
