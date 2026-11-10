import * as sr25519 from "@scure/sr25519";
import type { NotificationClaim } from "@parity/truapi-host/web";
import { deriveHardSecret } from "./chat-keys.js";
import {
  decodeStatement,
  encodeSigned,
  verifyStatement,
} from "../../bridges/primitives/statement.js";
import type { StatementStoreLike } from "../platform/statement-store.js";
import { isStored } from "../platform/statement-store.js";
import type { AllowanceGuard } from "../platform/allowance-usable.js";
import { postWhenUsable, type FinalOutcome } from "../platform/guarded-post.js";
import type { InvitationEvent, InvitationObserver } from "./observer.js";
import type { DeliveryReceiver, HostDelivery, IdentityKeys } from "./ports.js";
import { fromHex, toHex } from "../../bridges/primitives/bytes.js";
import { digest } from "./sealing.js";

/**
 * DRAFT ROUTE. A `HostDelivery` over the People chain's Statement Store: every
 * request and reply of every host on a network goes to one inbox per UTC day,
 * and each host tries to open what it reads there. Nothing in the clear names
 * a recipient or the request a reply answers: sealing, the evidence inside
 * and the reply key decide whom a post is for (`binding.ts`).
 *
 * Each post is one statement signed by a notification account: a fresh
 * sr25519 account per post and day, hard-derived from the wallet's entropy,
 * that the core registers for the day by proving the user's personhood
 * (`claimNotificationAccount`). The runtime gives such an account one
 * statement of at most {@link NOTIFICATION_DATA_MAX_BYTES} of data, and the
 * node checks that allowance; this host never signs a post with the user's
 * identity account. The same post on the same day derives the same account,
 * so posting it again after a restart claims nothing new. A post waits until
 * its account may post by the network's rule, and keeps one signed
 * statement per day, so ordinary retries send the same bytes
 * (`../platform/guarded-post.ts`).
 *
 * While this host runs, a request is posted again each new day, until it is
 * withdrawn, and a reply until its `until`. A post this delivery cannot keep
 * up, because no slot is free, the claim or the store refused it, or it is
 * too large, is reported through `posting` and tried again later; nothing is
 * deleted.
 *
 * With `submit` off, every claim is only validated against the runtime and
 * nothing is submitted: each post is then reported as not posted, with what
 * the claim would have been.
 *
 * Not decided: the inbox topic, the envelope, the retention, and whether a
 * shared inbox scales. This is a route to test the path, not Backhaul.
 */

/** Data one notification statement may hold: the People runtime's `NotificationAllowance`, read 2026-10-30. */
export const NOTIFICATION_DATA_MAX_BYTES = 10_240;
const DAY_MS = 86_400_000;
/** How long a post that could not be kept up waits before it is tried again. */
export const RETRY_MS = 15 * 60_000;
/** The return route of every request: the day inbox. */
export const DAY_INBOX_ROUTE = "day-inbox:draft-1";
/**
 * Payloads already handed on, by signer and encrypted data, so an outer
 * proof replacement is not opened twice. A new day's account still reaches
 * the receiver, which answers it. Bounded.
 */
const SEEN_MAX = 2048;

export interface StatementDeliveryOptions {
  /** The network's id: the inbox is per network. */
  network: string;
  /** The wallet's BIP-39 entropy, which the notification accounts derive from. */
  entropy: Uint8Array;
  /** Open the People chain's Statement Store. */
  openStore(): Promise<StatementStoreLike>;
  /** Register, or with `submit` false only validate, `account` for today. */
  claim(account: Uint8Array, submit: boolean): Promise<NotificationClaim>;
  /** When a claimed account may post, by the network's rule. */
  guard: AllowanceGuard;
  /** Whether claims and statements are submitted, or claims only validated. */
  submit: boolean;
  now(): number;
  /** Run `task` after `delayMs`. Returns a function that cancels it. */
  schedule(task: () => void, delayMs: number): () => void;
  log(line: string): void;
  /** Told which notification account each post uses, and each problem keeping it up. */
  observe?: InvitationObserver;
}

/** The UTC day of `nowMs`, as the runtime counts notification periods. */
export const periodOf = (nowMs: number): number => Math.floor(nowMs / DAY_MS);

/** The day inbox's topic on `network` for `period`. */
export const inboxTopic = (network: string, period: number): Uint8Array =>
  digest(
    "truapi-web-host/host-requests/day-inbox/draft-1",
    network,
    String(period),
  );

/** The notification account of post `id` on `period`. */
export function notificationAccount(
  entropy: Uint8Array,
  id: string,
  period: number,
): { public: Uint8Array; sign(payload: Uint8Array): Uint8Array } {
  const secret = deriveHardSecret(entropy, [
    "truapi-web-host",
    "notification",
    id,
    String(period),
  ]);
  return {
    public: sr25519.getPublicKey(secret),
    sign: (payload) => sr25519.sign(secret, payload),
  };
}

/**
 * A statement's expiry: the end of the day after `period` in the high 32
 * bits, so a post outlives its day by one, and `nowMs` in seconds as its
 * priority.
 */
export const postExpiry = (period: number, nowMs: number): bigint =>
  (BigInt((period + 2) * 86_400) << 32n) | BigInt(Math.floor(nowMs / 1000));

interface Post {
  sealed: string;
  until: number;
  /** The day it was last posted on. */
  posted: number | null;
  problem: string | null;
  /** The statement signed for this post on `period`, which every try that day sends. */
  signed?: {
    period: number;
    bytes: Uint8Array;
    claim?: NotificationClaim;
    renewed?: boolean;
  };
  needsFinal?: boolean;
  attempt?: symbol;
  publishing?: boolean;
}

export class StatementStoreDelivery implements HostDelivery {
  readonly maxPostBytes = NOTIFICATION_DATA_MAX_BYTES;
  private readonly posts = new Map<string, Post>();
  private receiver: DeliveryReceiver | null = null;
  private store: StatementStoreLike | null = null;
  private opening: Promise<StatementStoreLike> | null = null;
  private unsubscribe = () => {};
  private subscribedPeriod: number | null = null;
  private cancelDay = () => {};
  private cancelRetry = () => {};
  private claims: Promise<unknown> = Promise.resolve();
  private readonly seen = new Set<string>();
  private closed = false;

  constructor(private readonly options: StatementDeliveryOptions) {}

  listen(receiver: DeliveryReceiver): void {
    if (this.closed) return;
    this.receiver = receiver;
    void this.subscribe();
    this.scheduleDay();
  }

  returnRoute(): string {
    return DAY_INBOX_ROUTE;
  }

  isReturnRoute(route: string): boolean {
    return route === DAY_INBOX_ROUTE;
  }

  /** Every request goes to the day inbox; `to` is not named on the network. */
  postRequest(_to: IdentityKeys, id: string, sealed: string): void {
    this.add(id, sealed, Infinity);
  }

  postReply(route: string, id: string, sealed: string, until: number): void {
    if (route !== DAY_INBOX_ROUTE) {
      this.options.log("Not posted: not a return route of this delivery.");
      return;
    }
    this.add(id, sealed, until);
  }

  /** Every reply reaches the receiver on the day inbox; it keeps what opens. */
  watch(): void {}

  unwatch(): void {}

  withdraw(id: string): void {
    this.posts.delete(id);
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.posts.clear();
    this.unsubscribe();
    this.cancelDay();
    this.cancelRetry();
    this.store?.close();
    this.store = null;
    this.receiver = null;
  }

  private add(id: string, sealed: string, until: number): void {
    if (this.closed) return;
    const known = this.posts.get(id);
    // The same post again: keep what was posted today, so it costs no new slot.
    if (known?.sealed === sealed) known.until = until;
    else this.posts.set(id, { sealed, until, posted: null, problem: null });
    void this.publish(id);
  }

  private openStore(): Promise<StatementStoreLike> {
    if (this.store !== null) return Promise.resolve(this.store);
    this.opening ??= this.options.openStore().then(
      (store) => {
        this.opening = null;
        if (this.closed) {
          store.close();
          throw new Error("the delivery is closed");
        }
        this.store = store;
        void store.closed.then(() => this.lost(store));
        return store;
      },
      (error: unknown) => {
        this.opening = null;
        throw error;
      },
    );
    return this.opening;
  }

  /** The People connection ended: read and post again once it is back. */
  private lost(store: StatementStoreLike): void {
    if (this.closed || this.store !== store) return;
    this.store = null;
    this.unsubscribe();
    this.unsubscribe = () => {};
    this.subscribedPeriod = null;
    this.options.log("The People connection closed; delivery reconnects.");
    this.retryLater();
  }

  private async subscribe(): Promise<void> {
    const period = periodOf(this.options.now());
    if (this.closed || this.subscribedPeriod === period) return;
    try {
      const store = await this.openStore();
      const { network } = this.options;
      const stop = await store.subscribe(
        {
          matchAny: [
            inboxTopic(network, period - 1),
            inboxTopic(network, period),
          ],
        },
        (statements) => this.read(statements),
      );
      if (this.closed) return stop();
      this.unsubscribe();
      this.unsubscribe = stop;
      this.subscribedPeriod = period;
    } catch (error) {
      this.options.log(`The day inbox could not be read: ${String(error)}`);
      this.retryLater();
    }
  }

  private read(statements: Uint8Array[]): void {
    const receiver = this.receiver;
    if (receiver === null) return;
    for (const encoded of statements) {
      let statement;
      try {
        statement = decodeStatement(encoded);
      } catch {
        continue;
      }
      if (!verifyStatement(statement)) continue;
      const hash = toHex(
        digest("day-inbox-payload", statement.signer, statement.data),
      );
      if (this.seen.has(hash)) continue;
      if (this.seen.size >= SEEN_MAX) this.seen.clear();
      this.seen.add(hash);
      const sealed = toHex(statement.data);
      receiver.request(sealed);
      receiver.reply(DAY_INBOX_ROUTE, sealed);
    }
  }

  /** At each new UTC day: read the new inbox and post everything again. */
  private scheduleDay(): void {
    const now = this.options.now();
    const next = (periodOf(now) + 1) * DAY_MS + 30_000;
    this.cancelDay = this.options.schedule(() => {
      if (this.closed) return;
      void this.subscribe();
      for (const id of [...this.posts.keys()]) void this.publish(id);
      this.scheduleDay();
    }, next - now);
  }

  private retryLater(): void {
    this.cancelRetry();
    this.cancelRetry = this.options.schedule(() => {
      if (this.closed) return;
      void this.subscribe();
      for (const [id, post] of this.posts)
        if (post.problem !== null || post.posted === null)
          void this.publish(id);
    }, RETRY_MS);
  }

  private report(id: string, post: Post, problem: string | null): void {
    if (problem !== null) this.retryLater();
    if (post.problem === problem) return;
    post.problem = problem;
    this.observe({ type: "delivery-problem", post: id, problem });
    if (problem !== null)
      this.options.log(`Post ${id.slice(2, 10)}: ${problem}`);
    this.receiver?.posting(id, problem);
  }

  /** Post `id` on today's inbox, unless it is already, through a claimed notification account. */
  private async publish(id: string): Promise<void> {
    const post = this.posts.get(id);
    if (this.closed || post === undefined || post.publishing) return;
    const now = this.options.now();
    if (now >= post.until) {
      this.posts.delete(id);
      return;
    }
    const period = periodOf(now);
    if (post.posted === period) return;
    const data = fromHex(post.sealed);
    if (data.length > NOTIFICATION_DATA_MAX_BYTES)
      return this.report(
        id,
        post,
        `it is ${data.length} bytes sealed; a notification holds ${NOTIFICATION_DATA_MAX_BYTES}.`,
      );
    const account = notificationAccount(this.options.entropy, id, period);
    this.observe({
      type: "delivery-account",
      post: id,
      account: toHex(account.public),
      period,
    });
    const { submit } = this.options;
    const attempt = Symbol();
    post.attempt = attempt;
    post.publishing = true;
    const claimed = async () => {
      if (post.signed?.period === period && post.signed.claim !== undefined)
        return {
          ...post.signed.claim,
          ...(post.needsFinal ? { blockHash: undefined } : {}),
        };
      // One claim at a time, so two posts never race for one slot.
      const claim = await this.serially(() =>
        this.options.claim(account.public, submit),
      );
      this.options.log(
        `Post ${id.slice(2, 10)}: notification account ${toHex(account.public).slice(0, 10)}… ${claim.outcome} for day ${claim.period}, slot ${claim.seq}${claim.collection === undefined ? "" : ` of ${claim.collection}`}${claim.blockHash === undefined ? "" : ` in block ${claim.blockHash}`}.`,
      );
      if (claim.period !== period)
        throw new Error("the day changed while it was claimed.");
      if (post.signed?.period === period) post.signed.claim = claim;
      return claim;
    };
    try {
      if (!submit) {
        const claim = await claimed();
        if (this.posts.get(id) !== post) return;
        return this.report(
          id,
          post,
          `not posted: claims are only validated. Today's claim would take slot ${claim.seq} of ${claim.collection ?? "a collection"}.`,
        );
      }
      // Ordinary retries keep the proof, including after a lost submit answer.
      if (post.signed?.period !== period) {
        post.needsFinal = false;
        post.signed = {
          period,
          bytes: encodeSigned(
            {
              expiry: postExpiry(period, now),
              channel: null,
              topics: [inboxTopic(this.options.network, period)],
              data,
            },
            account.public,
            account.sign,
          ),
        };
      }
      let postingStore: StatementStoreLike | undefined;
      const posted = await postWhenUsable(
        {
          claim: async () => ({
            account: toHex(account.public),
            ...(await claimed()),
          }),
          bytes: post.signed.bytes,
          sign: post.signed.renewed ? undefined : account.sign,
          retryUnconfirmed: post.needsFinal,
          replaced: (bytes) => {
            post.signed = { ...post.signed!, bytes, renewed: true };
          },
          submit: async (bytes) => {
            if (postingStore === undefined)
              throw new Error("the posting connection is not open");
            return postingStore.submit(bytes);
          },
          unwanted: () =>
            this.closed ||
            this.posts.get(id) !== post ||
            post.attempt !== attempt
              ? "the post was withdrawn"
              : periodOf(this.options.now()) !== period
                ? "the day changed"
                : this.options.now() >= post.until
                  ? "the post has expired"
                  : null,
        },
        {
          guard: async (claim, request) => {
            postingStore = await this.openStore();
            const checked =
              request?.wait !== "finalized" &&
              (await postingStore.supportsBestBlockPosting?.()) === true;
            return this.options.guard(claim, {
              ...request,
              ...(!checked ? { wait: "finalized" } : {}),
              wording: {
                account: "The post's notification account",
                retry: "it is tried again later",
              },
            });
          },
          sleep: (ms) =>
            new Promise((resolve) => this.options.schedule(resolve, ms)),
          now: this.options.now,
        },
      );
      const { result } = posted;
      this.options.log(
        `Post ${id.slice(2, 10)}: the Statement Store answered ${result.status}${result.reason === undefined ? "" : ` (${result.reason})`} for ${data.length} bytes${posted.submits > 1 ? `, at submit ${posted.submits}` : ""}.`,
      );
      if (this.posts.get(id) !== post || post.attempt !== attempt) return;
      if (!isStored(result))
        return this.report(
          id,
          post,
          `the Statement Store refused it: ${result.status}${result.reason === undefined ? "" : ` (${result.reason})`}.`,
        );
      post.posted = period;
      this.report(id, post, null);
      void posted.final.then((outcome) =>
        this.settle(id, post, period, attempt, outcome),
      );
    } catch (error) {
      if (this.posts.get(id) === post && post.attempt === attempt)
        this.report(
          id,
          post,
          error instanceof Error ? error.message : String(error),
        );
    } finally {
      if (post.attempt === attempt) post.publishing = false;
    }
  }

  /**
   * What a post that went at a best block came to once its claim's finality
   * was known. One whose claim did not become final, even after the one
   * recovery, counts as not posted today. A later retry requires finalized
   * allowance and can create at most one outer replacement.
   */
  private settle(
    id: string,
    post: Post,
    period: number,
    attempt: symbol,
    outcome: FinalOutcome,
  ): void {
    if (
      outcome.tag === "final" ||
      this.closed ||
      this.posts.get(id) !== post ||
      post.attempt !== attempt
    )
      return;
    if (outcome.tag === "reposted") {
      this.options.log(
        `Post ${id.slice(2, 10)}: posted again once its account held at the finalized head: ${outcome.why}.`,
      );
      return;
    }
    if (post.posted !== period) return;
    post.posted = null;
    post.needsFinal = true;
    this.report(id, post, `posted at a best block, but ${outcome.why}.`);
  }

  private observe(event: InvitationEvent): void {
    if (this.closed) return;
    try {
      this.options.observe?.(event);
    } catch {
      // The developer tools only watch.
    }
  }

  private serially<T>(task: () => Promise<T>): Promise<T> {
    const run = this.claims.then(task, task);
    this.claims = run.catch(() => undefined);
    return run;
  }
}
