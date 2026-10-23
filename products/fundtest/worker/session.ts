// One funding session, from its assignment to its last report.
//
// Each session moves through the phases in saved.ts and stores what it is
// about to do in the worker's storage before each external action (a faucet
// drip, a top-up, a payment request), so a restarted worker that is assigned
// the session again picks up at that checkpoint. Every action a resumed phase can repeat is safe to
// repeat: top-up and payment ids are fixed by the session, so the host answers
// a repeat with `AlreadyExists`; the faucet drips an address once; and a
// report the host already took is refused as `OutOfOrder`. A session stopped
// by an error it cannot settle is run again later and resumes the same way.

import type {
  FundingAssignment,
  FundingCancelAnswer,
  FundingFailure,
  FundingUpdate,
} from "@parity/truapi";
import {
  FundingHeldError,
  FundingShortfallError,
  PASEO_ASSET_HUB_PARA_ID,
} from "@getsome/funding";
import { TOKENS } from "@getsome/core";
import type { EphemeralKeypairWithSecret } from "@getsome/ephemeral";
import type { ScreenAction, SessionView } from "../shared/channel";
import { FAUCET_PERMISSION } from "../shared/faucet";
import { toHex } from "../shared/hex";
import {
  paymentIdFor,
  refundIdFor,
  reserveAccount,
  sessionAccount,
  topUpIdFor,
  topUpSecret,
} from "./accounts";
import { onTestnet, withChains } from "./chain";
import {
  BANK_DETAILS,
  CONFIG,
  CRYPTO_ASSETS,
  CRYPTO_NETWORK,
  DEPOSIT_WINDOW_MS,
  ETA_SECS,
  FIAT,
  MAX_AMOUNT,
  MIN_AMOUNT,
  ONE_CASH,
} from "./config";
import {
  Aborted,
  HostCallError,
  describe,
  firstOf,
  isTransient,
  sleep,
  truapi,
  unwrap,
} from "./host";
import { DRIP_AMOUNT, DRIP_MARGIN, FaucetRefused, requestDrip } from "./faucet";
import {
  claimable,
  convertOnce,
  depositBalance,
  depositToken,
  displaySymbol,
  planConversion,
  sweepSurplus,
  CLAIM_UNIT,
  type CryptoSymbol,
} from "./onchain";
import { payoutOf, priceOf, termsOfQuoteId, type QuoteTerms } from "./quotes";
import { readSaved, storeSaved, type Saved } from "./saved";
import {
  forgetSession,
  markKilled,
  presentScreen,
  wasKilled,
  writeView,
} from "./screens";
import {
  killPointOf,
  triggerOf,
  type KillPoint,
  type Trigger,
} from "./triggers";

/** Wait between chain reads and pipeline ticks, as getcash's worker ticks. */
const TICK_MS = 6_000;
/** Top-ups a session makes at most, the host's limit. */
const MAX_TOP_UPS = 3;
/** Extra wait past the quoted ETA for a session made slow on purpose. */
const PAST_ETA_MS = 60_000;
/** How long a faucet drip may take to land on Asset Hub Next. */
const DRIP_ARRIVAL_MS = 5 * 60_000;
/** Times a failure that may clear by itself is retried, a tick apart, before
 *  the session stops and is run again later. */
const RETRY_ATTEMPTS = 50;
/** Times a report the host failed to take is made again before the session
 *  stops and is run again later. */
const REPORT_ATTEMPTS = 8;
/** Longest wait between two tries of a report. */
const MAX_REPORT_RETRY_MS = 30_000;
/** Times an update after the release is made again while the host may not
 *  have marked the session released yet. */
const RELEASED_REPORT_ATTEMPTS = 10;
/** Failed payout attempts the `payout-retry` trigger makes before one goes
 *  through. */
const PAYOUT_RETRIES = 2;
/** How long a cancel offer stays open for the user. */
const CANCEL_OFFER_MS = 5 * 60_000;
/** The fee a cancel after money moved costs: 1% of the amount, at least half
 *  a CASH, never more than the amount. */
const CANCEL_FEE_BPS = 100n;
const CANCEL_FEE_MIN = ONE_CASH / 2n;
/** Times in a row an error the session cannot settle may stop it before it
 *  gives up: a session no money moved for then ends as failed, and one money
 *  moved for keeps being run again, since only it can settle that money. */
export const MAX_RESTARTS = 6;

/** A failure the session ends on rather than retries. */
/** `value`, which the session's phase guarantees is saved. A missing one
 *  means the saved state does not match its phase, so the session stops
 *  rather than act on a guess. */
function required<T>(value: T | undefined, what: string): T {
  if (value === undefined)
    throw new Error(`fundtest: ${what} is missing from the saved state`);
  return value;
}

class SessionFailure extends Error {
  constructor(readonly reason: FundingFailure) {
    super(describe(reason));
    this.name = "SessionFailure";
  }
}

/** The host no longer has the session open for this provider: it ended,
 *  expired or was cancelled, so nothing is left to do. */
class SessionOver extends Error {
  constructor() {
    super("the host no longer has the session open");
    this.name = "SessionOver";
  }
}

/** The session stopped at its kill point, as if the worker crashed. */
export class Killed extends Error {
  constructor(readonly point: KillPoint) {
    super(`stopped at kill point ${point}`);
    this.name = "Killed";
  }
}

const other = (code: string, message: string): FundingFailure => ({
  tag: "Other",
  value: { code, message },
});

/** The terms a session runs on: the rail and asset its quote names, else a
 *  card payment in euro, the provider's first route. The direction is always
 *  the assignment's. */
function termsOf(assignment: FundingAssignment): QuoteTerms {
  const quoted = assignment.quote && termsOfQuoteId(assignment.quote.quoteId);
  return {
    rail: quoted?.rail ?? "Card",
    asset: quoted?.asset ?? FIAT.symbol,
    amount: assignment.amount ?? quoted?.amount ?? 0n,
    direction: assignment.direction,
  };
}

/** Runs one assigned session. */
export class Session {
  readonly intent: string;
  private readonly terms: QuoteTerms;
  private readonly saved: Saved;
  private abort = new AbortController();
  private cancelRequested = false;
  private account: Promise<EphemeralKeypairWithSecret> | undefined;
  /** A demo crypto deposit under way, which the deposit watch waits out so
   *  the drip's surplus is swept before the conversion starts. */
  private demoDeposit: Promise<void> | undefined;
  /** Why the demo deposit stopped the session: a failure it ends on, or its
   *  kill point. */
  private demoFailure: SessionFailure | Killed | undefined;

  /** `stored` is the record the session kept in storage, if any, and
   *  `restarts` counts the times in a row an error stopped this session. */
  constructor(
    assignment: FundingAssignment,
    stored: string | undefined,
    private readonly restarts = 0,
  ) {
    this.intent = assignment.intent;
    this.terms = termsOf(assignment);
    this.saved = readSaved(stored);
    const amount = this.saved.amount ?? assignment.amount?.toString();
    if (amount !== undefined) this.saved.amount = amount;
  }

  /** The inbound part of the saved state. Every inbound session saves it
   *  when it starts, so its absence means the saved state was corrupted. */
  private inboundState(): NonNullable<Saved["inbound"]> {
    return required(this.saved.inbound, "the inbound state");
  }

  /** The outbound part of the saved state, saved when an outbound session
   *  starts, like {@link inboundState}. */
  private outboundState(): NonNullable<Saved["outbound"]> {
    return required(this.saved.outbound, "the outbound state");
  }

  private get amount(): bigint | undefined {
    return this.saved.amount === undefined
      ? undefined
      : BigInt(this.saved.amount);
  }

  private get trigger(): Trigger | undefined {
    return triggerOf(this.amount);
  }

  private get signal(): AbortSignal {
    return this.abort.signal;
  }

  /** Whether nothing has moved yet, so the session may still be cancelled. */
  private get cancellable(): boolean {
    const phase =
      this.saved.inbound?.phase ?? this.saved.outbound?.phase ?? "new";
    return (
      !this.saved.inbound?.paymentSeen &&
      (phase === "new" || phase === "awaiting-user")
    );
  }

  /** Whether an accepted cancel can still be honoured: an inbound session
   *  until its first top-up, an outbound one until its payout is reported. */
  private get undoable(): boolean {
    if (this.terms.direction === "In") {
      const inbound = this.saved.inbound;
      return inbound?.phase !== "done" && (inbound?.claims.length ?? 0) === 0;
    }
    const outbound = this.saved.outbound;
    return outbound?.phase === "paying-out" && !outbound.payoutReported;
  }

  /** Whether the session started moving money, so only it can end it. */
  private get fundsStarted(): boolean {
    if (this.terms.direction === "In")
      return (this.saved.inbound?.claims.length ?? 0) > 0;
    const phase = this.saved.outbound?.phase ?? "new";
    return phase !== "new" && phase !== "awaiting-user";
  }

  /** The user asked to cancel, and the core asks for the answer. While
   *  nothing has moved the session stops: `Stopped`, and the core ends it as
   *  cancelled. After that the session offers to cancel for a fee, or
   *  refuses when it is past the point it can undo. The core asks again
   *  after a restart while it has no answer, and an offer still open is made
   *  again as it was. */
  async cancel(): Promise<FundingCancelAnswer> {
    this.cancelRequested = true;
    if (this.cancellable) {
      this.abort.abort();
      return { tag: "Stopped" };
    }
    if (this.saved.cancel === undefined) {
      const refusal = this.cancelRefusal();
      if (refusal !== undefined)
        return { tag: "Refused", value: { reason: refusal } };
      const { fee, refund } = this.cancelTerms();
      this.saved.cancel = {
        state: "offered",
        expiresAt: Date.now() + CANCEL_OFFER_MS,
        fee: fee.toString(),
        refund: refund.toString(),
      };
      await this.save();
    }
    const offer = this.saved.cancel;
    return {
      tag: "Offer",
      value: {
        fee: BigInt(offer.fee),
        refund: BigInt(offer.refund),
        expiresAt: BigInt(offer.expiresAt),
      },
    };
  }

  /** The user accepted the cancel offer: the session stores that, stops
   *  what it is doing and settles the cancel. */
  async cancelAccepted(): Promise<void> {
    const offer = this.saved.cancel;
    if (offer?.state === "accepted") return;
    if (offer === undefined || !this.undoable) {
      console.warn(
        `[fundtest] ${this.intent}: an accepted cancel arrived past the point it can be undone`,
      );
      return;
    }
    this.saved.cancel = { ...offer, state: "accepted" };
    await this.save();
    this.abort.abort();
  }

  /** The user declined the cancel offer, or it lapsed: the session carries
   *  on. */
  async cancelDeclined(): Promise<void> {
    if (this.saved.cancel?.state !== "offered") return;
    delete this.saved.cancel;
    this.cancelRequested = false;
    await this.save();
  }

  /** Why a cancel after money moved is refused, or nothing when the session
   *  can still be undone for a fee: an inbound session until its credit
   *  starts, an outbound one between its release and the payout. */
  private cancelRefusal(): string | undefined {
    if (this.trigger === "cancel-refuse")
      return "This provider does not cancel a payment once it is under way";
    if (this.terms.direction === "In") {
      if (!this.undoable)
        return "The CASH is already on its way to your balance";
      return this.terms.rail === "Crypto"
        ? "This provider cannot send a crypto deposit back"
        : undefined;
    }
    switch (this.saved.outbound?.phase) {
      case "paying-out":
        return this.undoable ? undefined : "The payout has already been made";
      case "refunding":
      case "done":
        return "The payout failed and is being refunded already";
      default:
        return "Your payment is still completing. Try again once it has";
    }
  }

  /** What cancelling costs and what the user gets back, in balance units. */
  private cancelTerms(): { fee: bigint; refund: bigint } {
    const debited = this.saved.outbound?.debited;
    const amount =
      debited === undefined ? this.requireAmount() : BigInt(debited);
    const percent = (amount * CANCEL_FEE_BPS) / 10_000n;
    const charged = percent > CANCEL_FEE_MIN ? percent : CANCEL_FEE_MIN;
    const fee = charged < amount ? charged : amount;
    return { fee, refund: amount - fee };
  }

  /** Waits while a cancel offer is open, so the session does not pass the
   *  point it can undo before the user answers. Only the core's answer ends
   *  the wait: it settles the cancel as declined once the offer lapses on its
   *  clock, so the worker's own clock never decides. */
  private async cancelGate(): Promise<void> {
    for (;;) {
      const cancel = this.saved.cancel;
      if (cancel?.state === "accepted") throw new Aborted();
      if (cancel?.state !== "offered") return;
      await sleep(1_000, this.signal);
    }
  }

  /** Settles a cancel the user accepted: an inbound session ends refunded on
   *  the user's rail, and an outbound one tops the user up by the offer's
   *  refund from the reserve account, under an id fixed by the session. */
  private async settleCancel(): Promise<void> {
    this.abort = new AbortController();
    const offer = required(this.saved.cancel, "the cancel offer");
    if (this.terms.direction === "In") return this.end({ tag: "Refunded" });
    const state = this.outboundState();
    state.refund ??= { id: refundIdFor(this.intent), amount: offer.refund };
    state.phase = "refunding";
    await this.save();
    await this.refund(state.refund);
    state.phase = "done";
    await forgetSession(this.intent);
  }

  /** Runs the session until it ends, reporting as it goes. Answers `killed`
   *  when it stopped at its kill point, and `retry` when an error it cannot
   *  settle stopped it: either way the worker runs it again and it
   *  resumes from its last checkpoint. */
  async run(): Promise<"ended" | "killed" | "retry"> {
    try {
      try {
        if (this.saved.cancel?.state === "accepted") await this.settleCancel();
        else if (this.terms.direction === "In") await this.inbound();
        else await this.outbound();
      } catch (error) {
        await this.settle(error);
      }
      return "ended";
    } catch (error) {
      if (error instanceof Killed) {
        console.info(`[fundtest] ${this.intent}: ${error.message}`);
        return "killed";
      }
      if (error instanceof SessionOver) {
        console.info(`[fundtest] ${this.intent}: ${error.message}`);
        await forgetSession(this.intent);
        return "ended";
      }
      if (this.restarts >= MAX_RESTARTS && !this.fundsStarted) {
        console.error(
          `[fundtest] ${this.intent} gives up after ${this.restarts} restarts: ${describe(error)}`,
        );
        await this.end(other("provider_stuck", describe(error)));
        return "ended";
      }
      console.error(
        `[fundtest] ${this.intent} stopped and resumes later: ${describe(error)}`,
      );
      return "retry";
    }
  }

  /** Ends the session on an error that decides its outcome, and rethrows any
   *  other. */
  private async settle(error: unknown): Promise<void> {
    if (error instanceof Aborted && this.saved.cancel?.state === "accepted")
      return this.settleCancel();
    // The core ended the session as cancelled on the `Stopped` answer.
    if (error instanceof Aborted && this.cancelRequested)
      return forgetSession(this.intent);
    if (error instanceof SessionFailure) return this.end(error.reason);
    if (
      error instanceof FundingShortfallError ||
      error instanceof FundingHeldError
    ) {
      return this.end(
        other(
          error instanceof FundingHeldError ? "held" : "shortfall",
          error.message,
        ),
      );
    }
    throw error;
  }

  private async end(reason: FundingFailure): Promise<void> {
    await this.report({ tag: "Failed", value: { reason } });
    await forgetSession(this.intent);
  }

  /**
   * Reports `update`, trying again with a growing wait while the host fails to
   * take it. A session the host no longer has ends quietly. `OutOfOrder`
   * means the host took the update before a restart, or, for a failure, that
   * funds already move and the host decides the outcome.
   *
   * An update after the release (`afterRelease`) may reach the host before it
   * has marked the session released, which it answers with `OutOfOrder` too,
   * so that refusal is retried for a while before it counts as taken.
   */
  private async report(
    update: FundingUpdate,
    afterRelease = false,
  ): Promise<void> {
    const client = await truapi();
    for (let attempt = 0; ; attempt++) {
      try {
        await unwrap(
          client.fundingProvider.report({ intent: this.intent, update }),
          `report ${update.tag}`,
        );
        return;
      } catch (error) {
        const tag = error instanceof HostCallError ? error.tag : undefined;
        if (tag === "OutOfOrder") {
          if (afterRelease && attempt < RELEASED_REPORT_ATTEMPTS) {
            await sleep(CONFIG.reportRetryMs);
            continue;
          }
          console.info(
            update.tag === "Failed"
              ? `[fundtest] ${this.intent}: the host keeps the session, funds are moving`
              : `[fundtest] ${this.intent}: ${update.tag} already reported`,
          );
          return;
        }
        if (tag === "NotFound") throw new SessionOver();
        if (!isTransient(error) || attempt + 1 >= REPORT_ATTEMPTS) throw error;
        const wait = Math.min(
          CONFIG.reportRetryMs * 2 ** attempt,
          MAX_REPORT_RETRY_MS,
        );
        console.warn(
          `[fundtest] ${this.intent}: ${update.tag} not reported, trying again in ${wait} ms: ${describe(error)}`,
        );
        await sleep(wait);
      }
    }
  }

  /** Stores the session's checkpoint, retrying until storage keeps it: an
   *  action taken after a checkpoint that did not stick could be repeated by
   *  a restarted worker. */
  private async save(): Promise<void> {
    await this.retrying("save", () => storeSaved(this.intent, this.saved));
  }

  /** Stops the session at kill point `point` when its amount chose it, once:
   *  the mark is kept in product storage, apart from the checkpoint,
   *  so the resumed session runs on past it. */
  private async killAt(point: KillPoint): Promise<void> {
    if (killPointOf(this.amount) !== point) return;
    if (await wasKilled(this.intent)) return;
    await markKilled(this.intent);
    throw new Killed(point);
  }

  private sessionAccount(): Promise<EphemeralKeypairWithSecret> {
    this.account ??= sessionAccount(this.intent);
    return this.account;
  }

  /** Runs `work` until it succeeds, waiting a tick after each failure that
   *  may clear by itself. A host refusal, a programming error, a session
   *  failure and the pipeline's terminal errors are not retried, and neither
   *  is a failure that outlasts `RETRY_ATTEMPTS`: the session then stops and
   *  is run again later. */
  private async retrying<T>(what: string, work: () => Promise<T>): Promise<T> {
    for (let attempt = 1; ; attempt++) {
      try {
        return await work();
      } catch (error) {
        if (
          error instanceof Aborted ||
          error instanceof Killed ||
          error instanceof SessionOver ||
          error instanceof SessionFailure ||
          error instanceof FaucetRefused ||
          error instanceof FundingShortfallError ||
          error instanceof FundingHeldError ||
          !isTransient(error) ||
          attempt >= RETRY_ATTEMPTS
        ) {
          throw error;
        }
        console.warn(
          `[fundtest] ${this.intent}: ${what} failed, retrying: ${describe(error)}`,
        );
        await sleep(TICK_MS, this.signal);
      }
    }
  }

  /** Reads `read` every tick until it returns a value. */
  private async poll<T>(
    what: string,
    read: () => Promise<T | undefined>,
  ): Promise<T> {
    for (;;) {
      const value = await this.retrying(what, read);
      if (value !== undefined) return value;
      await sleep(TICK_MS, this.signal);
    }
  }

  /** How long a simulated arrival on `rail` takes. */
  private arrivalMs(normalMs: number): number {
    return this.trigger === "slow"
      ? Number(ETA_SECS[this.terms.rail]) * 1_000 + PAST_ETA_MS
      : normalMs;
  }

  /** How long the simulated card or bank payment takes to arrive. */
  private fiatArrivalMs(): number {
    return this.arrivalMs(
      this.terms.rail === "Card" ? CONFIG.cardArrivalMs : CONFIG.bankArrivalMs,
    );
  }

  /** How long the simulated payout takes, its failed attempts included. */
  private payoutWaitMs(): number {
    const attempts = this.trigger === "payout-retry" ? PAYOUT_RETRIES + 1 : 1;
    return attempts * this.arrivalMs(CONFIG.payoutMs);
  }

  /** Waits until `at`, Unix ms. */
  private waitUntil(at: number): Promise<void> {
    return sleep(Math.max(0, at - Date.now()), this.signal);
  }

  private get reference(): string {
    return `FT-${this.intent
      .replace(/[^a-zA-Z0-9]/g, "")
      .slice(-8)
      .toUpperCase()}`;
  }

  private view(extra: Partial<SessionView> = {}): SessionView {
    return {
      intent: this.intent,
      direction: this.terms.direction,
      rail: this.terms.rail,
      asset: this.terms.asset,
      decimals:
        this.terms.rail === "Crypto"
          ? (CRYPTO_ASSETS[this.terms.asset as CryptoSymbol]?.decimals ?? 0)
          : FIAT.decimals,
      ...(this.saved.amount === undefined ? {} : { amount: this.saved.amount }),
      ...extra,
    };
  }

  /** Takes an amount the user entered on a screen when the session had none. */
  private adoptAmount(
    action: Extract<ScreenAction, { amount?: string }>,
  ): bigint {
    if (this.amount !== undefined) return this.amount;
    if (action.amount === undefined || !/^\d+$/.test(action.amount)) {
      throw new SessionFailure(
        other("amount_required", "No amount was entered"),
      );
    }
    const amount = BigInt(action.amount);
    if (amount < MIN_AMOUNT) throw new SessionFailure({ tag: "BelowMinimum" });
    if (amount > MAX_AMOUNT) throw new SessionFailure({ tag: "AboveMaximum" });
    this.saved.amount = amount.toString();
    return amount;
  }

  private requireAmount(): bigint {
    if (this.amount === undefined) {
      throw new SessionFailure(
        other("amount_required", "This route needs an amount up front"),
      );
    }
    return this.amount;
  }

  // Inbound ------------------------------------------------------------------

  private async inbound(): Promise<void> {
    const state = (this.saved.inbound ??= { phase: "new", claims: [] });
    if (state.phase === "new") {
      await this.report({ tag: "AwaitingPayment" });
      await this.report({
        tag: "Details",
        value: {
          transactionId: `ft_${this.intent}`,
          reference: this.reference,
        },
      });
      state.phase = "awaiting-user";
      await this.save();
    }
    if (state.phase === "awaiting-user") {
      if (this.terms.rail === "Crypto") await this.awaitCryptoDeposit();
      else await this.awaitFiatPayment();
    }
    if (state.phase === "fiat-sent") {
      await this.waitUntil(
        state.arrivesAt ?? Date.now() + this.fiatArrivalMs(),
      );
      await this.report({
        tag: "PaymentReceived",
        value: { finalized: false },
      });
      if (this.trigger === "refund") {
        throw new SessionFailure({ tag: "Refunded" });
      }
      state.phase = "fiat-received";
      await this.save();
    }
    if (state.phase === "fiat-received") {
      await this.plan("PAS");
      state.phase = "funding";
      await this.save();
    }
    if (state.phase === "funding") {
      await this.fundFromFaucet();
      state.phase = "arriving";
      await this.save();
    }
    if (state.phase === "arriving") {
      await this.awaitFinalDeposit();
      await this.report({ tag: "PaymentReceived", value: { finalized: true } });
      await this.report({ tag: "Converting" });
      state.phase = "converting";
      await this.save();
    }
    if (state.phase === "converting") {
      await this.convert();
      state.phase = "crediting";
      await this.save();
    }
    if (state.phase === "crediting") {
      await this.cancelGate();
      await this.credit();
      await this.killAt(4);
      await this.report({ tag: "Delivered" });
      await forgetSession(this.intent);
      state.phase = "done";
      await this.save();
    }
  }

  /** Card and bank: the provider's screen, until the user pays or leaves. */
  private async awaitFiatPayment(): Promise<void> {
    const state = this.inboundState();
    const isBank = this.terms.rail === "Bank";
    const bank = { ...BANK_DETAILS, reference: this.reference };
    const price = () => {
      const amount = this.amount;
      return amount === undefined
        ? undefined
        : priceOf({ ...this.terms, amount });
    };
    const reportBankDeposit = async () => {
      const amount = price();
      if (amount === undefined) return;
      await this.report({
        tag: "Deposit",
        value: {
          deposit: {
            tag: "Bank",
            value: {
              amount,
              currency: FIAT.symbol,
              decimals: FIAT.decimals,
              beneficiary: bank.beneficiary,
              account: bank.account,
              bankCode: bank.bankCode,
              reference: bank.reference,
            },
          },
        },
      });
    };
    const knownPrice = price();
    await writeView(
      this.view({
        ...(knownPrice === undefined ? {} : { price: knownPrice.toString() }),
        ...(isBank ? { bank } : {}),
      }),
    );
    if (isBank) await reportBankDeposit();
    const action = isBank
      ? await presentScreen(this.intent, "/bank", "bank-sent", this.signal)
      : await presentScreen(this.intent, "/card", "card-paid", this.signal);
    if (!action) throw new SessionFailure({ tag: "Cancelled" });
    const hadAmount = this.amount !== undefined;
    this.adoptAmount(action);
    if (this.trigger === "decline") {
      throw new SessionFailure({ tag: "Declined" });
    }
    if (isBank && !hadAmount) await reportBankDeposit();
    state.arrivesAt = Date.now() + this.fiatArrivalMs();
    state.phase = "fiat-sent";
    await this.save();
  }

  /** Crypto: the deposit address, until enough has arrived. */
  private async awaitCryptoDeposit(): Promise<void> {
    const state = this.inboundState();
    const amount = this.requireAmount();
    const asset = this.terms.asset as CryptoSymbol;
    const account = await this.sessionAccount();
    const plan = await this.plan(asset);
    state.depositExpiresAt ??= Date.now() + DEPOSIT_WINDOW_MS;
    await this.save();
    const token = depositToken(plan);
    const deposit = BigInt(plan.deposit);
    const reportDeposit = (owed: bigint) =>
      this.report({
        tag: "Deposit",
        value: {
          deposit: {
            tag: "Crypto",
            value: {
              address: account.address,
              network: CRYPTO_NETWORK,
              asset: displaySymbol(token),
              amount: owed,
              decimals: token.decimals,
              exact: false,
              expiresAt: BigInt(
                required(state.depositExpiresAt, "the deposit deadline"),
              ),
            },
          },
        },
      });
    await reportDeposit(deposit);

    if (CONFIG.demoCrypto) {
      if (this.trigger === "refund") {
        await sleep(this.arrivalMs(CONFIG.cryptoArrivalMs), this.signal);
        await this.report({
          tag: "PaymentReceived",
          value: { finalized: false },
        });
        throw new SessionFailure({ tag: "Refunded" });
      }
      this.demoDeposit = sleep(
        this.arrivalMs(CONFIG.cryptoArrivalMs),
        this.signal,
      )
        .then(() => this.makeDemoDeposit())
        .catch((error: unknown) => {
          if (error instanceof SessionFailure || error instanceof Killed)
            this.demoFailure = error;
          else if (!(error instanceof Aborted)) {
            console.warn(
              `[fundtest] ${this.intent}: demo deposit: ${describe(error)}`,
            );
          }
        });
    }

    let lastSeen = 0n;
    await this.poll("deposit read", async () => {
      if (this.demoFailure) throw this.demoFailure;
      // A demo deposit can switch the plan to PAS, so read it every time.
      const current = required(state.plan, "the conversion plan");
      const held = await withChains((chains) =>
        depositBalance(
          chains.assetHub,
          depositToken(current),
          account.address,
          "best",
        ),
      );
      if (held >= BigInt(current.deposit)) {
        await this.demoDeposit;
        if (this.demoFailure) throw this.demoFailure;
        return true;
      }
      if (
        held === 0n &&
        Date.now() > required(state.depositExpiresAt, "the deposit deadline")
      ) {
        throw new SessionFailure({ tag: "Expired" });
      }
      if (held !== lastSeen && held > 0n) {
        // A short payment: ask for the rest.
        lastSeen = held;
        await this.markPaymentSeen();
        await reportDeposit(deposit - held);
      }
      return undefined;
    });
    await this.markPaymentSeen();
    await this.report({ tag: "PaymentReceived", value: { finalized: false } });
    state.phase = "arriving";
    await this.save();
  }

  /** Records that the user's payment reached the provider, before the host
   *  hears of it, so a restarted worker treats a cancel as a request. */
  private async markPaymentSeen(): Promise<void> {
    const state = this.inboundState();
    if (state.paymentSeen) return;
    state.paymentSeen = true;
    await this.save();
  }

  /** The conversion plan, sized once and kept. */
  private async plan(asset: CryptoSymbol | undefined) {
    const state = this.inboundState();
    if (state.plan) return state.plan;
    const amount = this.requireAmount();
    const account = await this.sessionAccount();
    state.plan = await this.retrying("route sizing", () =>
      withChains((chains) =>
        planConversion(chains, asset, amount, account.address),
      ),
    );
    await this.save();
    return state.plan;
  }

  /** A demo crypto deposit: the faucet's PAS, unless the user's own deposit
   *  has started to arrive. A session that asked for another asset switches to
   *  the PAS route first, since the faucet gives only PAS. */
  private async makeDemoDeposit(): Promise<void> {
    const state = this.inboundState();
    const account = await this.sessionAccount();
    const asked = required(state.plan, "the conversion plan");
    if (asked.depositSymbol !== TOKENS.PAS.symbol) {
      const held = await this.retrying("deposit read", () =>
        withChains((chains) =>
          depositBalance(
            chains.assetHub,
            depositToken(asked),
            account.address,
            "best",
          ),
        ),
      );
      if (held > 0n) return;
      const amount = this.requireAmount();
      state.plan = await this.retrying("route sizing", () =>
        withChains((chains) =>
          planConversion(chains, "PAS", amount, account.address),
        ),
      );
      await this.save();
    }
    await this.fundFromFaucet();
  }

  /** Makes sure the host lets this product's `fetch` reach the faucet. A
   *  card or bank screen asked the user already; a demo crypto deposit asks
   *  here. */
  private async allowFaucet(): Promise<void> {
    const client = await truapi();
    const granted = await client.permissions
      .requestRemotePermission(FAUCET_PERMISSION)
      .match(
        (response) => response.granted,
        () => false,
      );
    if (!granted) {
      throw new SessionFailure(
        other("faucet_blocked", "Network access to the faucet was not allowed"),
      );
    }
  }

  /** The simulated arrival: the session account asks the public faucet for
   *  PAS, waits for the drip to land and sweeps what the deposit does not
   *  need to the reserve account, so the conversion spends only the deposit.
   *  Does not ask again once a drip was asked for. */
  private async fundFromFaucet(): Promise<void> {
    const state = this.inboundState();
    const deposit = BigInt(required(state.plan, "the conversion plan").deposit);
    const account = await this.sessionAccount();
    if (!onTestnet()) {
      throw new SessionFailure(
        other("no_faucet", "The public faucet serves only Paseo"),
      );
    }
    if (deposit > DRIP_AMOUNT - DRIP_MARGIN) {
      throw new SessionFailure(
        other(
          "above_faucet_drip",
          "The deposit is more PAS than one faucet drip gives",
        ),
      );
    }
    const held = () =>
      withChains((chains) =>
        depositBalance(chains.assetHub, TOKENS.PAS, account.address, "best"),
      );
    // A drip asked for without an answer is asked for again: the faucet drips
    // an address once, so a repeat of one that went through is refused on
    // quota, and the session waits for the first.
    if (
      (await this.retrying("deposit read", held)) < deposit &&
      !state.drip?.hash
    ) {
      await this.allowFaucet();
      state.drip = { at: Date.now() };
      await this.save();
      try {
        const hash = await this.retrying("faucet drip", () =>
          requestDrip(
            { address: account.address, signBytes: account.signer.signBytes },
            PASEO_ASSET_HUB_PARA_ID,
          ),
        );
        await this.killAt(1);
        state.drip = { ...state.drip, hash };
        await this.save();
      } catch (error) {
        if (!(error instanceof FaucetRefused)) throw error;
        if (!error.quota) {
          throw new SessionFailure(other("faucet_refused", error.message));
        }
      }
    }
    const drip = state.drip;
    await this.poll("drip arrival", async () => {
      if ((await held()) >= deposit) return true;
      if (drip && Date.now() > drip.at + DRIP_ARRIVAL_MS) {
        throw new SessionFailure(
          other("faucet_timeout", "The faucet's PAS did not arrive in time"),
        );
      }
      return undefined;
    });
    const reserve = await reserveAccount();
    await this.retrying("surplus sweep", () =>
      withChains((chains) =>
        sweepSurplus(chains.assetHub, account, deposit, reserve.address),
      ),
    );
  }

  private async awaitFinalDeposit(): Promise<void> {
    const plan = required(this.inboundState().plan, "the conversion plan");
    const token = depositToken(plan);
    const deposit = BigInt(plan.deposit);
    const account = await this.sessionAccount();
    await this.poll("final deposit read", async () => {
      const held = await withChains((chains) =>
        depositBalance(chains.assetHub, token, account.address, "finalized"),
      );
      return held >= deposit ? true : undefined;
    });
  }

  /** getcash's pipeline, one tick at a time, until the CASH is final on
   *  People. */
  private async convert(): Promise<void> {
    const state = this.inboundState();
    const plan = required(state.plan, "the conversion plan");
    const account = await this.sessionAccount();
    const amount = this.requireAmount();
    for (;;) {
      const step = await this.retrying("conversion tick", () =>
        withChains((chains) =>
          convertOnce({
            chains,
            account,
            plan,
            settleAmount: amount,
            tick: state.tick,
            persist: async (tick) => {
              state.tick = tick;
              await this.save();
            },
          }),
        ),
      );
      if (step === "done") return;
      await this.killAt(2);
      await sleep(TICK_MS, this.signal);
    }
  }

  /** Claims the session account's CASH into the user's balance, over up to
   *  three top-ups: another when the host claimed less than asked, and a
   *  deliberate second one for the `retry` trigger. */
  private async credit(): Promise<void> {
    const state = this.inboundState();
    const account = await this.sessionAccount();
    const client = await truapi();
    for (let attempt = 0; attempt < MAX_TOP_UPS; attempt += 1) {
      let claim = state.claims[attempt];
      if (!claim) {
        let amount = await this.retrying("claimable read", () =>
          withChains((chains) => claimable(chains.people, account.address)),
        );
        if (amount === 0n) break;
        if (this.trigger === "retry" && attempt === 0) {
          const half = (amount / 2n / CLAIM_UNIT) * CLAIM_UNIT;
          if (half > 0n) amount = half;
        }
        claim = {
          id: topUpIdFor(account.publicKey, attempt),
          amount: amount.toString(),
          attempt,
        };
        state.claims.push(claim);
        await this.save();
      }
      const { id } = claim;
      const amount = BigInt(claim.amount);
      try {
        await this.topUp(id, amount, topUpSecret(account));
      } catch (error) {
        if (error instanceof HostCallError && error.tag === "InvalidSource") {
          throw new SessionFailure(
            other("invalid_source", "The host refused the session account"),
          );
        }
        throw error;
      }
      await this.killAt(3);
      await this.report({ tag: "Crediting", value: { topUpId: id, amount } });
      const outcome = await this.retrying("top-up status", () =>
        firstOf(
          client.payment.topUpStatusSubscribe({ request: { id } }),
          (status) => {
            if (status.tag === "Claimed")
              return status.value.finalized ? "claimed" : undefined;
            if (
              status.tag === "ClaimedPartially" ||
              status.tag === "NotClaimed"
            )
              return "short";
            return undefined;
          },
          this.signal,
        ),
      );
      const splitOnPurpose = this.trigger === "retry" && attempt === 0;
      if (outcome === "claimed" && !splitOnPurpose) return;
    }
    if (state.claims.length === 0) {
      throw new SessionFailure(
        other("nothing_to_claim", "No CASH reached the session account"),
      );
    }
  }

  /** Tops the user up by `amount` from the account `secret` signs for, under
   *  `id`. A repeat after a restart is answered as already made. */
  private async topUp(
    id: `0x${string}`,
    amount: bigint,
    secret: `0x${string}`,
  ): Promise<void> {
    const client = await truapi();
    await this.retrying("top-up", async () => {
      try {
        await unwrap(
          client.payment.topUp({
            amount,
            source: { tag: "PrivateKey", value: { sr25519SecretKey: secret } },
            id,
          }),
          "payment.topUp",
        );
      } catch (error) {
        if (error instanceof HostCallError && error.tag === "AlreadyExists")
          return;
        throw error;
      }
    });
  }

  // Outbound -----------------------------------------------------------------

  private async outbound(): Promise<void> {
    const state = (this.saved.outbound ??= { phase: "new" });
    const client = await truapi();
    if (state.phase === "new") {
      await this.report({
        tag: "Details",
        value: { transactionId: `ft_${this.intent}` },
      });
      state.phase = "awaiting-user";
      await this.save();
    }
    if (state.phase === "awaiting-user") {
      const amount = this.requireAmount();
      const payout = payoutOf({ ...this.terms, amount });
      await writeView(
        this.view(payout === undefined ? {} : { price: payout.toString() }),
      );
      const action = await presentScreen(
        this.intent,
        "/payout",
        "payout-confirmed",
        this.signal,
      );
      if (!action) throw new SessionFailure({ tag: "Cancelled" });
      state.destination = action.destination;
      state.paymentId = paymentIdFor(this.intent);
      state.phase = "requesting";
      await this.save();
    }
    if (state.phase === "requesting") {
      const amount = this.requireAmount();
      const id = required(state.paymentId, "the payment id");
      const destination = toHex((await reserveAccount()).publicKey);
      try {
        await unwrap(
          client.payment.request({ amount, destination, id }),
          "payment.request",
        );
      } catch (error) {
        const tag = error instanceof HostCallError ? error.tag : undefined;
        if (tag === "InsufficientBalance")
          throw new SessionFailure({ tag: "InsufficientBalance" });
        if (tag === "Rejected") throw new SessionFailure({ tag: "Cancelled" });
        if (tag !== "AlreadyExists") throw error;
      }
      await this.killAt(1);
      try {
        await this.report({
          tag: "Collecting",
          value: { paymentId: id, amount },
        });
      } catch (error) {
        // A resumed session may find the payment already completed, which
        // released the session: its status decides what comes next.
        if (!(error instanceof SessionOver)) throw error;
      }
      state.phase = "collecting";
      await this.save();
      await this.killAt(2);
    }
    if (state.phase === "collecting") {
      const id = required(state.paymentId, "the payment id");
      const amount = this.requireAmount();
      const debited = await this.retrying("payment status", () =>
        firstOf(
          client.payment.statusSubscribe({ request: { id } }),
          (status): bigint | null | undefined =>
            status.tag === "Completed"
              ? amount
              : status.tag === "ClaimedPartially"
                ? BigInt(status.value.actualClaimed)
                : status.tag === "Failed"
                  ? null
                  : undefined,
          this.signal,
        ),
      );
      const paid = debited !== null;
      // The host ends the session from the payment's outcome. A released
      // withdrawal keeps its stored state until the payout and any refund are
      // reported, so a restart resumes the payout rather than looking for the
      // payment again.
      if (paid) {
        state.debited = debited.toString();
        state.payoutAt = Date.now() + this.payoutWaitMs();
        state.phase = "paying-out";
        await this.save();
      } else {
        state.phase = "done";
      }
    }
    if (state.phase === "paying-out") {
      if (await this.payOut(state)) {
        state.refund = {
          id: refundIdFor(this.intent),
          amount: state.debited ?? this.requireAmount().toString(),
        };
        state.phase = "refunding";
        await this.save();
      } else {
        state.phase = "done";
      }
    }
    if (state.phase === "refunding") {
      await this.refund(required(state.refund, "the refund"));
      state.phase = "done";
    }
    await forgetSession(this.intent);
  }

  /** The simulated payout to the destination the user typed, due at
   *  `payoutAt`. The CASH stays with the reserve account. Answers whether the
   *  payout failed. */
  private async payOut(
    state: NonNullable<Saved["outbound"]>,
  ): Promise<boolean> {
    const client = await truapi();
    const operation = await client.worker
      .beginOperation({ label: `fundtest payout ${this.intent}` })
      .match(
        (response) => response.id,
        () => undefined,
      );
    try {
      if (!state.converted) {
        await this.report({ tag: "Converting" }, true);
        state.converted = true;
        await this.save();
      }
      await this.killAt(3);
      if (this.trigger === "payout-retry" && !state.payoutReported) {
        for (let attempt = 0; attempt < PAYOUT_RETRIES; attempt++) {
          await sleep(this.arrivalMs(CONFIG.payoutMs), this.signal);
          await this.report({ tag: "PayoutRetrying" }, true);
        }
      }
      const failed = this.trigger === "refund";
      if (state.payoutReported) return failed;
      await this.waitUntil(state.payoutAt ?? Date.now() + this.payoutWaitMs());
      await this.cancelGate();
      await this.report(
        {
          tag: "Payout",
          value: {
            outcome: failed
              ? {
                  tag: "Failed",
                  value: { reason: "The receiving bank returned the payout" },
                }
              : { tag: "PaidOut" },
          },
        },
        true,
      );
      state.payoutReported = true;
      return failed;
    } finally {
      if (operation !== undefined)
        await client.worker.endOperation({ id: operation }).match(
          () => undefined,
          () => undefined,
        );
    }
  }

  /** Refunds a failed payout from the reserve account, which holds the CASH
   *  the session collected, then names the top-up for the host. The top-up's
   *  id was fixed before it is made, so a repeat after a restart is answered
   *  as already made. */
  private async refund(refund: {
    id: `0x${string}`;
    amount: string;
  }): Promise<void> {
    const amount = BigInt(refund.amount);
    await this.topUp(refund.id, amount, topUpSecret(await reserveAccount()));
    await this.report(
      { tag: "Refunding", value: { topUpId: refund.id, amount } },
      true,
    );
  }
}
