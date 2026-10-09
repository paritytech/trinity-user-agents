// One funding session, from its assignment to its last report.
//
// Each session moves through the phases in saved.ts and saves the phase it
// reached before acting on the next, so a restarted worker that is handed the
// session again picks up at that phase. A report the host already took is
// refused as `OutOfOrder` when a resumed phase repeats it, which is harmless.

import type {
  FundingAssignment,
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
import {
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
} from "./config";
import {
  Aborted,
  HostCallError,
  describe,
  firstOf,
  randomId,
  sleep,
  toHex,
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
import { priceOf, termsOfQuoteId, type QuoteTerms } from "./quotes";
import { readSaved, writeSaved, type Saved } from "./saved";
import { forgetSession, presentScreen, writeView } from "./screens";
import { triggerOf, type Trigger } from "./triggers";

/** Wait between chain reads and pipeline ticks, as getcash's worker ticks. */
const TICK_MS = 6_000;
/** Top-ups a session makes at most, the host's limit. */
const MAX_TOP_UPS = 3;
/** Extra wait past the quoted ETA for a session made slow on purpose. */
const PAST_ETA_MS = 60_000;
/** How long a faucet drip may take to land on Asset Hub Next. */
const DRIP_ARRIVAL_MS = 5 * 60_000;

/** A failure the session ends on rather than retries. */
class SessionFailure extends Error {
  constructor(readonly reason: FundingFailure) {
    super(describe(reason));
    this.name = "SessionFailure";
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
  private readonly abort = new AbortController();
  private cancelRequested = false;
  /** Set once `run` has returned, including after an error it could not
   *  report, when nothing is left to act on a cancel. */
  private stopped = false;
  /** Whether part of a crypto deposit has reached the session account. */
  private fundsSeen = false;
  private account: Promise<EphemeralKeypairWithSecret> | undefined;
  /** A demo crypto deposit under way, which the deposit watch waits out so
   *  the drip's surplus is swept before the conversion starts. */
  private demoDeposit: Promise<void> | undefined;
  /** Why the demo deposit could not be made, which ends the session. */
  private demoFailure: SessionFailure | undefined;

  constructor(assignment: FundingAssignment) {
    this.intent = assignment.intent;
    this.terms = termsOf(assignment);
    this.saved = readSaved(assignment.saved);
    const amount = this.saved.amount ?? assignment.amount?.toString();
    if (amount !== undefined) this.saved.amount = amount;
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
    return !this.fundsSeen && (phase === "new" || phase === "awaiting-user");
  }

  /** The user asked to cancel. Honoured while nothing has moved. */
  cancel(): void {
    this.cancelRequested = true;
    if (this.cancellable && this.stopped) void this.end({ tag: "Cancelled" });
    else if (this.cancellable) this.abort.abort();
    else
      console.info(
        `[fundtest] ${this.intent}: cancel ignored, funds are moving`,
      );
  }

  /** Runs the session to its end, reporting as it goes. */
  async run(): Promise<void> {
    try {
      if (this.terms.direction === "In") await this.inbound();
      else await this.outbound();
    } catch (error) {
      if (error instanceof Aborted && this.cancelRequested) {
        await this.end({ tag: "Cancelled" });
      } else if (error instanceof SessionFailure) {
        await this.end(error.reason);
      } else if (
        error instanceof FundingShortfallError ||
        error instanceof FundingHeldError
      ) {
        await this.end(
          other(
            error instanceof FundingHeldError ? "held" : "shortfall",
            error.message,
          ),
        );
      } else {
        console.error(`[fundtest] ${this.intent} stopped: ${describe(error)}`);
      }
    } finally {
      this.stopped = true;
    }
  }

  private async end(reason: FundingFailure): Promise<void> {
    await this.report({ tag: "Failed", value: { reason } }).catch(
      (error: unknown) =>
        console.error(
          `[fundtest] ${this.intent}: could not report the failure: ${describe(error)}`,
        ),
    );
    await forgetSession(this.intent);
  }

  private async report(update: FundingUpdate): Promise<void> {
    const client = await truapi();
    try {
      await unwrap(
        client.fundingProvider.report({ intent: this.intent, update }),
        `report ${update.tag}`,
      );
    } catch (error) {
      if (error instanceof HostCallError && error.tag === "OutOfOrder") {
        console.info(
          `[fundtest] ${this.intent}: ${update.tag} already reported`,
        );
        return;
      }
      throw error;
    }
  }

  private async save(): Promise<void> {
    const client = await truapi();
    await client.fundingProvider
      .save({ intent: this.intent, state: writeSaved(this.saved) })
      .match(
        () => undefined,
        (error) =>
          console.warn(
            `[fundtest] ${this.intent}: save failed: ${describe(error)}`,
          ),
      );
  }

  private sessionAccount(): Promise<EphemeralKeypairWithSecret> {
    this.account ??= sessionAccount(this.intent);
    return this.account;
  }

  /** Runs `work` until it succeeds, waiting a tick after each failure. A
   *  session failure and the pipeline's terminal errors are not retried. */
  private async retrying<T>(what: string, work: () => Promise<T>): Promise<T> {
    for (;;) {
      try {
        return await work();
      } catch (error) {
        if (
          error instanceof Aborted ||
          error instanceof SessionFailure ||
          error instanceof FaucetRefused ||
          error instanceof FundingShortfallError ||
          error instanceof FundingHeldError
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
      const normal =
        this.terms.rail === "Card"
          ? CONFIG.cardArrivalMs
          : CONFIG.bankArrivalMs;
      await sleep(this.arrivalMs(normal), this.signal);
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
      await this.credit();
      await this.report({ tag: "Delivered" });
      state.phase = "done";
      await this.save();
      await forgetSession(this.intent);
    }
  }

  /** Card and bank: the provider's screen, until the user pays or leaves. */
  private async awaitFiatPayment(): Promise<void> {
    const state = this.saved.inbound!;
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
    if (isBank && !hadAmount) await reportBankDeposit();
    state.phase = "fiat-sent";
    await this.save();
  }

  /** Crypto: the deposit address, until enough has arrived. */
  private async awaitCryptoDeposit(): Promise<void> {
    const state = this.saved.inbound!;
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
              expiresAt: BigInt(state.depositExpiresAt!),
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
          if (error instanceof SessionFailure) this.demoFailure = error;
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
      const current = state.plan!;
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
      if (held === 0n && Date.now() > state.depositExpiresAt!) {
        throw new SessionFailure({ tag: "Expired" });
      }
      if (held !== lastSeen && held > 0n) {
        // A short payment: ask for the rest.
        lastSeen = held;
        this.fundsSeen = true;
        await reportDeposit(deposit - held);
      }
      return undefined;
    });
    await this.report({ tag: "PaymentReceived", value: { finalized: false } });
    state.phase = "arriving";
    await this.save();
  }

  /** The conversion plan, sized once and kept. */
  private async plan(asset: CryptoSymbol | undefined) {
    const state = this.saved.inbound!;
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
    const state = this.saved.inbound!;
    const account = await this.sessionAccount();
    const asked = state.plan!;
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
    const state = this.saved.inbound!;
    const deposit = BigInt(state.plan!.deposit);
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
    if ((await this.retrying("deposit read", held)) < deposit && !state.drip) {
      await this.allowFaucet();
      const at = Date.now();
      try {
        const hash = await this.retrying("faucet drip", () =>
          requestDrip(
            { address: account.address, signBytes: account.signer.signBytes },
            PASEO_ASSET_HUB_PARA_ID,
          ),
        );
        state.drip = { at, hash };
      } catch (error) {
        if (!(error instanceof FaucetRefused)) throw error;
        // A quota refusal means this account was dripped already, by a
        // request whose answer was lost: wait for that drip.
        if (!error.quota) {
          throw new SessionFailure(other("faucet_refused", error.message));
        }
        state.drip = { at };
      }
      await this.save();
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
    const plan = this.saved.inbound!.plan!;
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
    const state = this.saved.inbound!;
    const plan = state.plan!;
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
      await sleep(TICK_MS, this.signal);
    }
  }

  /** Claims the session account's CASH into the user's balance, over up to
   *  three top-ups: another when the host claimed less than asked, and a
   *  deliberate second one for the `retry` trigger. */
  private async credit(): Promise<void> {
    const state = this.saved.inbound!;
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
      await this.retrying("top-up", async () => {
        try {
          await unwrap(
            client.payment.topUp({
              amount,
              source: {
                tag: "PrivateKey",
                value: { sr25519SecretKey: topUpSecret(account) },
              },
              id,
            }),
            "payment.topUp",
          );
        } catch (error) {
          if (error instanceof HostCallError && error.tag === "AlreadyExists")
            return;
          if (error instanceof HostCallError && error.tag === "InvalidSource") {
            throw new SessionFailure(
              other("invalid_source", "The host refused the session account"),
            );
          }
          throw error;
        }
      });
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
      const quote = priceOf({ ...this.terms, amount });
      await writeView(
        this.view(quote === undefined ? {} : { price: quote.toString() }),
      );
      const action = await presentScreen(
        this.intent,
        "/payout",
        "payout-confirmed",
        this.signal,
      );
      if (!action) throw new SessionFailure({ tag: "Cancelled" });
      state.destination = action.destination;
      state.paymentId = randomId();
      state.phase = "requesting";
      await this.save();
    }
    if (state.phase === "requesting") {
      const amount = this.requireAmount();
      const id = state.paymentId!;
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
      await this.report({
        tag: "Collecting",
        value: { paymentId: id, amount },
      });
      state.phase = "collecting";
      await this.save();
    }
    if (state.phase === "collecting") {
      const id = state.paymentId!;
      const paid = await this.retrying("payment status", () =>
        firstOf(
          client.payment.statusSubscribe({ request: { id } }),
          (status) =>
            status.tag === "Completed" || status.tag === "PartiallyClaimed"
              ? true
              : status.tag === "Failed"
                ? false
                : undefined,
          this.signal,
        ),
      );
      // The host ends the session from the payment's outcome; once it is
      // released the host no longer keeps saved state, so nothing more is saved.
      state.phase = paid ? "paying-out" : "done";
    }
    if (state.phase === "paying-out") {
      await this.payOut();
      state.phase = "done";
    }
    await forgetSession(this.intent);
  }

  /** The simulated payout to the destination the user typed. The CASH stays
   *  with the collection account. */
  private async payOut(): Promise<void> {
    const client = await truapi();
    const operation = await client.worker
      .beginOperation({ label: `fundtest payout ${this.intent}` })
      .match(
        (response) => response.id,
        () => undefined,
      );
    try {
      await sleep(this.arrivalMs(CONFIG.payoutMs), this.signal);
      await reportPayout(
        this.intent,
        this.trigger === "refund"
          ? {
              outcome: "failed",
              reason: "The receiving bank returned the payout",
            }
          : { outcome: "paid-out" },
      );
    } finally {
      if (operation !== undefined)
        await client.worker.endOperation({ id: operation }).match(
          () => undefined,
          () => undefined,
        );
    }
  }
}

/** How a payout went. */
type PayoutOutcome =
  | { outcome: "paid-out" }
  | { outcome: "failed"; reason: string };

/**
 * Reports a released outbound session's payout. The host marks the session
 * released only once the user's payment completes, so a `NotFound` or an
 * out-of-order refusal right after it is retried briefly.
 */
async function reportPayout(
  intent: string,
  payout: PayoutOutcome,
): Promise<void> {
  const client = await truapi();
  const outcome =
    payout.outcome === "paid-out"
      ? ({ tag: "PaidOut" } as const)
      : ({ tag: "Failed", value: { reason: payout.reason } } as const);
  for (let attempt = 0; ; attempt++) {
    try {
      await unwrap(
        client.fundingProvider.report({
          intent,
          update: { tag: "Payout", value: { outcome } },
        }),
        "report Payout",
      );
      return;
    } catch (error) {
      const retryable =
        error instanceof HostCallError &&
        (error.tag === "NotFound" || error.tag === "OutOfOrder");
      if (!retryable || attempt >= PAYOUT_REPORT_ATTEMPTS) throw error;
      await sleep(PAYOUT_REPORT_RETRY_MS);
    }
  }
}

const PAYOUT_REPORT_ATTEMPTS = 10;
const PAYOUT_REPORT_RETRY_MS = 1_000;
