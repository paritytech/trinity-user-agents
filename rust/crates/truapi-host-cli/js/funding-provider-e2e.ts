// End-to-end check of a provider's worker serving funding sessions, against
// the CLI signing host with `TRUAPI_FUNDING_PROVIDERS` naming the provider and
// its worker script. The overlay offers the provider, the auto-accepting user
// picks it, and the host launches the worker as its own process when the core
// asks for it and stops it when the core lets it go.
//
// The worker (`ScriptedProvider`, run by `scripts/funding-worker.ts`) answers
// the core's `quote`, `assign`, `cancel` and `settleCancel` calls and calls the
// core with `report` and `presentFrame`. What it does with a session, how it
// answers its cancel and how it settles an accepted one are set by the
// session's direction and amount, as the product and the worker are separate
// processes, and it records what it was asked and what it did in the journal
// `TRUAPI_FUNDING_WORKER_LOG` names. The product's cases read that journal and
// the host's transcript, and act as the user through the host's commands
// (`actAsUser`), as a person at the terminal would.
//
// The run is split across a host restart. `runProviderStart` takes an inbound
// session as far as its top-up, and `runProviderResume`, in a new host process
// on the same storage, checks the core starts the worker again for the session
// still open, assigns it where it stopped with the worker's own state still in
// its storage, and delivers it from the claim. Top-ups and payments go to the
// host's ledger engines, which complete them in full.
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { ok } from "neverthrow";
import type { TrUApiClient } from "../../../../js/packages/truapi/src/index.ts";
import type {
  FundingAssignment,
  FundingCancelAnswer,
  FundingUpdate,
  HostFundingStatusSubscribeItem,
} from "../../../../js/packages/truapi/src/generated/types.ts";
import type { DiagnosisRow } from "./diagnosis.ts";
import {
  WAIT_MS,
  randomId,
  stringify,
  untilComplete,
  waitForTranscript,
} from "./funding-e2e.ts";

/** What the first run leaves for the run after the restart. */
export interface ProviderState {
  intent: string;
  topUpId: string;
  amount: string;
}

type HexString = `0x${string}`;

/** Run `work` once the handler's answer has gone out, so the core holds the
 *  answer before the worker's next call about the same session. */
function afterAnswer(work: () => unknown) {
  setTimeout(work, 0);
}

/** An inbound session the worker credits `DELIVERED_CREDIT` of, in full. */
export const DELIVERED_AMOUNT = 1_000n;
export const DELIVERED_CREDIT = 900n;
/** An outbound session the worker pays out in full, and reports paid out. */
export const RELEASED_AMOUNT = 500n;
/** An inbound session the worker fails. */
export const FAILED_AMOUNT = 2_000n;
/** The inbound session the worker shows screens for and takes to its
 *  top-up, and delivers after the restart. */
export const IN_AMOUNT = 1_100n;
/** An outbound session the worker pays out in full, then reports its payout
 *  on, through retries, to the end. */
export const OUT_AMOUNT = 550n;
/** An inbound session whose cancel the worker stops. */
export const CANCEL_AMOUNT = 300n;
/** An inbound session the worker takes payment for, then offers terms to
 *  cancel. */
export const OFFER_AMOUNT = 250n;
/** An inbound session the worker starts crediting before the user cancels. */
export const LATE_AMOUNT = 200n;
/** An outbound session whose cancel the worker answers with an offer only
 *  once the session is released, and refunds once the user accepts. */
export const RELEASED_CANCEL_AMOUNT = 400n;
const CANCEL_FEE = 3n;
/** What the worker refunds of an accepted cancel after release. */
const REFUND = RELEASED_CANCEL_AMOUNT - CANCEL_FEE;

/** Type one command into the host, as the user at its terminal would. The
 *  battery hands the host this pipe as its stdin. */
export function actAsUser(command: string) {
  const pipe = process.env.TRUAPI_FUNDING_USER;
  if (!pipe)
    throw new Error("TRUAPI_FUNDING_USER not set; cannot act as the user");
  appendFileSync(pipe, `${command}\n`);
}

/** Wait until `reached` holds, up to `WAIT_MS`. */
export async function until<T>(
  reached: () => T | undefined,
  what: string,
): Promise<T> {
  const deadline = Date.now() + WAIT_MS;
  for (;;) {
    const hit = reached();
    if (hit !== undefined) return hit;
    if (Date.now() >= deadline) throw new Error(`never saw ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

/** The storage key the worker keeps a session's own state under. */
const stateKey = (intent: string) => `funding:${intent}`;

/** One line of the worker's journal. `pid` names the worker process. */
export type JournalLine = { pid: number } & (
  | { kind: "connected" }
  | { kind: "quoted"; direction: string; amount: string; quoteId: string }
  | {
      kind: "assigned";
      intent: string;
      direction: string;
      amount?: string;
      lastUpdate?: FundingUpdate;
      quoteId?: string;
    }
  | { kind: "cancel"; intent: string }
  | { kind: "settled"; intent: string; decision: string }
  | {
      kind: "step";
      intent: string;
      step: string;
      ok: boolean;
      output: string;
      topUpId?: string;
    }
);

/** What the worker recorded, across every process that ran it. */
export function journal(path: string): JournalLine[] {
  if (!existsSync(path)) return [];
  // A line still being written is skipped; the next read sees it whole.
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.length > 0)
    .flatMap((line) => {
      try {
        return [JSON.parse(line) as JournalLine];
      } catch {
        return [];
      }
    });
}

/** The worker's first record of `kind` about session `intent`. */
export function journaled<Kind extends JournalLine["kind"]>(
  path: string,
  kind: Kind,
  intent: string,
  step?: string,
): Promise<Extract<JournalLine, { kind: Kind }>> {
  return until(
    () =>
      journal(path).find(
        (line): line is Extract<JournalLine, { kind: Kind }> =>
          line.kind === kind &&
          "intent" in line &&
          line.intent === intent &&
          (step === undefined || ("step" in line && line.step === step)),
      ),
    `the worker's ${step ?? kind} on ${intent}`,
  );
}

/** The worker's step `step` on session `intent`, or an error if it failed. */
async function stepDone(path: string, intent: string, step: string) {
  const done = await journaled(path, "step", intent, step);
  if (!done.ok) throw new Error(`the worker's ${step} failed: ${done.output}`);
  return done;
}

export async function report(
  client: TrUApiClient,
  intent: string,
  update: FundingUpdate,
): Promise<void> {
  const reported = await client.fundingProvider.report({ intent, update });
  if (reported.isErr()) {
    throw new Error(
      `report ${update.tag} refused: ${stringify(reported.error)}`,
    );
  }
}

/** Start a top-up of `amount` under `topUpId` for session `intent`. */
async function topUp(
  client: TrUApiClient,
  intent: string,
  topUpId: HexString,
  amount: bigint,
) {
  const toppedUp = await client.payment.topUp({
    amount,
    source: {
      tag: "ProductAccount",
      value: { derivationIndex: { tag: "Index", value: 0 } },
    },
    id: topUpId,
    funding: intent,
  });
  if (toppedUp.isErr()) {
    throw new Error(`payment.topUp failed: ${stringify(toppedUp.error)}`);
  }
}

/** Request a payment of `amount` for session `intent`, and return its id. */
async function requestPayment(
  client: TrUApiClient,
  intent: string,
  amount: bigint,
): Promise<HexString> {
  const paymentId = randomId();
  const paid = await client.payment.request({
    amount,
    destination: `0x${"22".repeat(32)}`,
    id: paymentId,
    funding: intent,
  });
  if (paid.isErr()) {
    throw new Error(`payment.request failed: ${stringify(paid.error)}`);
  }
  return paymentId;
}

/** Name payment `paymentId` of `amount` in a Collecting report. */
async function collecting(
  client: TrUApiClient,
  intent: string,
  paymentId: HexString,
  amount: bigint,
) {
  await report(client, intent, {
    tag: "Out",
    value: { tag: "Collecting", value: { paymentId, amount } },
  });
}

/** Request a payment of `amount` for session `intent` and name it in a
 *  Collecting report. */
async function payOut(client: TrUApiClient, intent: string, amount: bigint) {
  const paymentId = await requestPayment(client, intent, amount);
  await collecting(client, intent, paymentId, amount);
}

/** Wait for the host to see session `intent` released. */
function releasedOnHost(fundingLogPath: string, intent: string) {
  return waitForTranscript(
    fundingLogPath,
    (line) =>
      line.kind === "status" &&
      line.intent === intent &&
      line.tag === "Released",
    `${intent} released on the host`,
  );
}

/** The provider's worker: it prices every ask one to one, takes every
 *  session the core assigns, does what the session's direction and amount
 *  call for, answers cancels by them too, and settles accepted offers by
 *  ending the session or refunding it. Every call is recorded in the
 *  journal, and every call is idempotent, as the core repeats what is pending
 *  when the worker connects. */
export class ScriptedProvider {
  private readonly pid = process.pid;
  private readonly started = new Set<string>();
  /** Each assigned session's direction and amount, which choose what the
   *  worker does with it. */
  private readonly kinds = new Map<string, string>();
  /** Sessions whose payment arrived, so their cancel is answered with an
   *  offer. */
  private readonly paid = new Set<string>();
  /** The payment requested for a session, named only once the user cancels
   *  it. */
  private readonly payments = new Map<string, HexString>();

  constructor(
    private readonly client: TrUApiClient,
    private readonly journalPath: string,
    private readonly fundingLogPath: string,
  ) {
    this.record({ kind: "connected" });
    client.fundingProvider.onQuote(({ ask }) => {
      const quoteId = `q-${crypto.randomUUID()}`;
      this.record({
        kind: "quoted",
        direction: ask.direction,
        amount: ask.amount.toString(),
        quoteId,
      });
      return ok({
        tag: "Quoted",
        value: {
          quote: {
            quoteId,
            sendAmount: ask.amount,
            receiveAmount: ask.amount,
            providerFee: 0n,
            networkFee: 0n,
            etaSecs: 60n,
            assetDecimals: 2,
          },
        },
      });
    });
    client.fundingProvider.onAssign(({ session }) => {
      this.record({
        kind: "assigned",
        intent: session.intent,
        direction: session.direction,
        amount: session.amount?.toString(),
        lastUpdate: session.lastUpdate,
        quoteId: session.quote?.quoteId,
      });
      this.kinds.set(session.intent, kindOf(session));
      if (!this.started.has(session.intent)) {
        this.started.add(session.intent);
        afterAnswer(() => this.serve(session));
      }
      return ok(undefined);
    });
    client.fundingProvider.onCancel(async ({ intent }) => {
      this.record({ kind: "cancel", intent });
      return ok(await this.cancelAnswer(intent));
    });
    client.fundingProvider.onSettleCancel(({ intent, decision }) => {
      this.record({ kind: "settled", intent, decision: decision.tag });
      if (decision.tag === "Accepted") {
        afterAnswer(() => this.settleAccepted(intent));
      }
      return ok(undefined);
    });
  }

  /** How the worker answers the cancel of session `intent`. An answer may
   *  wait for the worker's own work, as an offer on a withdrawal waits for
   *  its release. */
  private async cancelAnswer(intent: string): Promise<FundingCancelAnswer> {
    switch (this.kinds.get(intent)) {
      case `In:${CANCEL_AMOUNT}`:
        return { tag: "Stopped" };
      case `In:${OFFER_AMOUNT}`:
        return this.paid.has(intent)
          ? {
              tag: "Offer",
              value: { fee: CANCEL_FEE, refund: OFFER_AMOUNT - CANCEL_FEE },
            }
          : refused;
      case `Out:${RELEASED_CANCEL_AMOUNT}`: {
        let answer = refused;
        await this.step(intent, "offer_once_released", async () => {
          const paymentId = this.payments.get(intent);
          if (!paymentId) throw new Error("no payment was requested");
          // A withdrawal can be offered for only once it is released, so the
          // worker names its payment now and answers once it completes.
          await collecting(
            this.client,
            intent,
            paymentId,
            RELEASED_CANCEL_AMOUNT,
          );
          await releasedOnHost(this.fundingLogPath, intent);
          answer = { tag: "Offer", value: { fee: CANCEL_FEE, refund: REFUND } };
          return "named the payment, and offered terms once it was released";
        });
        return answer;
      }
      default:
        return refused;
    }
  }

  /** Settle the cancel of session `intent` the user accepted: refund a
   *  released withdrawal, and end any other session as cancelled. */
  private settleAccepted(intent: string) {
    const { client } = this;
    if (this.kinds.get(intent) === `Out:${RELEASED_CANCEL_AMOUNT}`) {
      return this.step(intent, "refund", async () => {
        const topUpId = randomId();
        await topUp(client, intent, topUpId, REFUND);
        await report(client, intent, {
          tag: "Out",
          value: { tag: "Refunding", value: { topUpId, amount: REFUND } },
        });
        return "refunded the withdrawal less the fee";
      });
    }
    return this.step(intent, "settle_cancel", async () => {
      await report(client, intent, {
        tag: "Failed",
        value: { reason: { tag: "Cancelled" } },
      });
      return "ended the session as cancelled";
    });
  }

  private record(line: Record<string, unknown>) {
    appendFileSync(
      this.journalPath,
      `${stringify({ pid: this.pid, ...line })}\n`,
    );
  }

  /** Run `work` as step `step` of session `intent` and record how it went. */
  private async step(
    intent: string,
    step: string,
    work: () => Promise<string | { output: string; topUpId: string }>,
  ) {
    try {
      const done = await work();
      this.record(
        typeof done === "string"
          ? { kind: "step", intent, step, ok: true, output: done }
          : { kind: "step", intent, step, ok: true, ...done },
      );
    } catch (error) {
      this.record({
        kind: "step",
        intent,
        step,
        ok: false,
        output: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /** Do what session `session`'s direction and amount call for. A session
   *  assigned again after a restart is carried on only where the worker
   *  kept its own state for it. */
  private async serve(session: FundingAssignment) {
    const { client } = this;
    const { intent } = session;
    const key = kindOf(session);
    if (session.lastUpdate !== undefined) {
      if (key === `In:${IN_AMOUNT}`) {
        await this.resumeInbound(intent, session.lastUpdate);
      }
      return;
    }
    switch (key) {
      case `In:${DELIVERED_AMOUNT}`:
        return this.step(intent, "deliver", async () => {
          const topUpId = randomId();
          await topUp(client, intent, topUpId, DELIVERED_CREDIT);
          await report(client, intent, {
            tag: "In",
            value: {
              tag: "Crediting",
              value: { topUpId, amount: DELIVERED_CREDIT },
            },
          });
          await report(client, intent, {
            tag: "In",
            value: { tag: "Delivered" },
          });
          return "credited and delivered";
        });
      case `Out:${RELEASED_AMOUNT}`:
        await this.step(intent, "release", async () => {
          await payOut(client, intent, RELEASED_AMOUNT);
          return "paid out";
        });
        // A released withdrawal holds the worker until its payout is
        // reported, so the worker reports it and the run leaves nothing open.
        return this.step(intent, "payout", async () => {
          await releasedOnHost(this.fundingLogPath, intent);
          await report(client, intent, {
            tag: "Out",
            value: { tag: "Payout", value: { outcome: { tag: "PaidOut" } } },
          });
          return "reported the payout";
        });
      case `In:${FAILED_AMOUNT}`:
        return this.step(intent, "fail", async () => {
          // Failing at once would end the session before the product
          // watches it, and the case checks the product sees it in
          // progress first.
          await new Promise((resolve) => setTimeout(resolve, 500));
          await report(client, intent, {
            tag: "Failed",
            value: {
              reason: {
                tag: "Other",
                value: {
                  code: "provider_failed",
                  message: "the battery's provider failed the session",
                },
              },
            },
          });
          return "failed the session";
        });
      case `In:${IN_AMOUNT}`:
        return this.serveInbound(intent);
      case `Out:${OUT_AMOUNT}`:
        return this.serveOutbound(intent);
      case `In:${OFFER_AMOUNT}`:
        return this.step(intent, "payment_received", async () => {
          await report(client, intent, {
            tag: "In",
            value: { tag: "PaymentReceived", value: { finalized: true } },
          });
          this.paid.add(intent);
          return "the payment arrived";
        });
      case `In:${LATE_AMOUNT}`:
        return this.serveLate(intent);
      case `Out:${RELEASED_CANCEL_AMOUNT}`:
        return this.step(intent, "payment_requested", async () => {
          this.payments.set(
            intent,
            await requestPayment(client, intent, RELEASED_CANCEL_AMOUNT),
          );
          return "requested the payment, to name once the user cancels";
        });
    }
  }

  /** Pay the session out, then once it is released report its payout
   *  through two retries and its references to the end, and check a report
   *  after that is refused. */
  private async serveOutbound(intent: string) {
    const { client } = this;
    await this.step(intent, "release", async () => {
      await payOut(client, intent, OUT_AMOUNT);
      return "paid out";
    });
    await this.step(intent, "payout", async () => {
      await releasedOnHost(this.fundingLogPath, intent);
      await report(client, intent, {
        tag: "Out",
        value: { tag: "Converting" },
      });
      await report(client, intent, {
        tag: "Out",
        value: { tag: "PayoutRetrying" },
      });
      await report(client, intent, {
        tag: "Out",
        value: { tag: "PayoutRetrying" },
      });
      await report(client, intent, {
        tag: "Details",
        value: { transactionId: "payout-tx-1", reference: "battery-payout" },
      });
      await report(client, intent, {
        tag: "Out",
        value: { tag: "Payout", value: { outcome: { tag: "PaidOut" } } },
      });
      const late = await client.fundingProvider.report({
        intent,
        update: { tag: "Out", value: { tag: "PayoutRetrying" } },
      });
      // A paid-out withdrawal is over, so the core no longer serves it.
      if (!late.isErr() || !stringify(late.error).includes("NotFound")) {
        throw new Error(
          `a retry after the payout was not refused: ${stringify(late)}`,
        );
      }
      return "the payout's retries and references were taken after the release, and a retry once it was paid out was refused";
    });
  }

  /** Show a screen and a page, report forward and back, and start the
   *  top-up, keeping its id in the worker's own storage first. */
  private async serveInbound(intent: string) {
    const { client } = this;
    await this.step(intent, "present_frame", async () => {
      const framed = await client.fundingProvider.presentFrame({
        intent,
        target: { tag: "App", value: { route: "/kyc" } },
      });
      if (framed.isErr() || framed.value.outcome.tag !== "Closed") {
        throw new Error(`presentFrame: ${stringify(framed)}`);
      }
      return "the screen closed";
    });
    await this.step(intent, "present_page", async () => {
      const returnUrl = "https://onramp.example.com/meld/return?flow=buy";
      const framed = await client.fundingProvider.presentFrame({
        intent,
        target: {
          tag: "Page",
          value: { url: "https://widget.example.com/buy", returnUrl },
        },
      });
      if (
        framed.isErr() ||
        framed.value.outcome.tag !== "Returned" ||
        framed.value.outcome.value.url !== returnUrl
      ) {
        throw new Error(`presentFrame page: ${stringify(framed)}`);
      }
      return "the page returned its return URL";
    });
    await this.step(intent, "reports_forward_only", async () => {
      await report(client, intent, {
        tag: "In",
        value: { tag: "AwaitingPayment" },
      });
      await report(client, intent, {
        tag: "In",
        value: {
          tag: "Deposit",
          value: {
            deposit: {
              tag: "Crypto",
              value: {
                address: "0x0000000000000000000000000000000000000001",
                network: "polkadot",
                asset: "USDT",
                amount: 1_000n,
                decimals: 6,
                exact: true,
              },
            },
          },
        },
      });
      await report(client, intent, {
        tag: "In",
        value: { tag: "PaymentReceived", value: { finalized: true } },
      });
      await report(client, intent, {
        tag: "In",
        value: { tag: "Converting" },
      });
      const backwards = await client.fundingProvider.report({
        intent,
        update: { tag: "In", value: { tag: "AwaitingPayment" } },
      });
      if (
        !backwards.isErr() ||
        !stringify(backwards.error).includes("OutOfOrder")
      ) {
        throw new Error(
          `a backwards report was not refused: ${stringify(backwards)}`,
        );
      }
      return "updates moved forward, with a deposit to pay, and a backwards one was refused";
    });
    await this.step(intent, "credits_through_top_up", async () => {
      const topUpId = randomId();
      // Written before the top-up, so a restart in between still knows which
      // top-up it started.
      const kept = await client.localStorage.write({
        key: stateKey(intent),
        value: topUpId,
      });
      if (kept.isErr()) {
        throw new Error(`keeping its state failed: ${stringify(kept.error)}`);
      }
      await topUp(client, intent, topUpId, IN_AMOUNT);
      await report(client, intent, {
        tag: "In",
        value: { tag: "Crediting", value: { topUpId, amount: IN_AMOUNT } },
      });
      return { output: "started the top-up and named it", topUpId };
    });
  }

  /** Check the kept state against the update the core repeated, and
   *  deliver the session. */
  private async resumeInbound(intent: string, lastUpdate: FundingUpdate) {
    const { client } = this;
    await this.step(intent, "kept_state", async () => {
      const kept = await client.localStorage.read({ key: stateKey(intent) });
      if (
        lastUpdate.tag !== "In" ||
        lastUpdate.value.tag !== "Crediting" ||
        kept.isErr() ||
        kept.value.value !== lastUpdate.value.value.topUpId
      ) {
        throw new Error(
          `assigned with ${stringify(lastUpdate)}, kept ${stringify(kept)}`,
        );
      }
      return "the kept top-up matches the last update";
    });
    await this.step(intent, "deliver", async () => {
      await report(client, intent, { tag: "In", value: { tag: "Delivered" } });
      await client.localStorage.clear({ key: stateKey(intent) });
      return "delivered";
    });
  }

  /** Start crediting, wait for the user's cancel to be too late, then
   *  deliver. */
  private async serveLate(intent: string) {
    const { client } = this;
    const topUpId = randomId();
    await this.step(intent, "topped_up", async () => {
      await topUp(client, intent, topUpId, LATE_AMOUNT);
      return "started the top-up";
    });
    await this.step(intent, "deliver", async () => {
      await waitForTranscript(
        this.fundingLogPath,
        (line) =>
          line.kind === "cancel" &&
          line.intent === intent &&
          line.outcome === "TooLate",
        `the user's cancel of ${intent}`,
      );
      await report(client, intent, {
        tag: "In",
        value: { tag: "Crediting", value: { topUpId, amount: LATE_AMOUNT } },
      });
      await report(client, intent, { tag: "In", value: { tag: "Delivered" } });
      return "delivered";
    });
  }
}

const refused: FundingCancelAnswer = {
  tag: "Refused",
  value: { reason: "the battery set no answer" },
};

/** The direction and amount that choose what the worker does with a
 *  session. */
const kindOf = (session: FundingAssignment) =>
  `${session.direction}:${session.amount ?? "any"}`;

/** A case runner that records one row per case, passing on a returned
 *  output and failing on a thrown error. */
export function rowFactory(rows: DiagnosisRow[]) {
  return async (methodName: string, check: () => Promise<string>) => {
    const startedAt = performance.now();
    const row = (status: DiagnosisRow["status"], output: string) =>
      rows.push({
        id: `Funding/${methodName}`,
        serviceName: "Funding",
        methodName,
        status,
        output,
        durationMs: Math.round(performance.now() - startedAt),
      });
    try {
      row("pass", await check());
    } catch (error) {
      // One stuck case is recorded and the rest still run.
      row("fail", error instanceof Error ? error.message : String(error));
    }
  };
}

/** Request a session and return its intent, which the worker sees too. */
export async function requestFunding(
  client: TrUApiClient,
  direction: "In" | "Out",
  amount: bigint,
): Promise<string> {
  const requested = await client.funding.request({ direction, amount });
  if (requested.isErr()) {
    throw new Error(`funding.request failed: ${stringify(requested.error)}`);
  }
  return requested.value.intent;
}

export async function lastStatus(
  client: TrUApiClient,
  intent: string,
): Promise<HostFundingStatusSubscribeItem | undefined> {
  const items = await untilComplete(
    client.funding.statusSubscribe({ request: { intent } }),
    `session ${intent}`,
  );
  return items.at(-1);
}

/** Cancel session `intent` as the user, and wait for the host to record it. */
async function cancelAsUser(
  fundingLogPath: string,
  intent: string,
  outcome: string,
) {
  actAsUser(`/funding cancel ${intent}`);
  await waitForTranscript(
    fundingLogPath,
    (line) =>
      line.kind === "cancel" &&
      line.intent === intent &&
      line.outcome === outcome,
    `the user's cancel of ${intent} answered ${outcome}`,
  );
}

/** The first run: assignment, a provider screen, forward-only reports, the
 *  outbound and cancelled sessions, and an inbound one up to its top-up. */
export async function runProviderStart(
  client: TrUApiClient,
  fundingLogPath: string,
  journalPath: string,
): Promise<{ rows: DiagnosisRow[]; state?: ProviderState }> {
  const rows: DiagnosisRow[] = [];
  const check = rowFactory(rows);
  let inbound = "";
  let outbound = "";
  let state: ProviderState | undefined;

  await check("provider_assigned", async () => {
    inbound = await requestFunding(client, "In", IN_AMOUNT);
    const { direction, amount, lastUpdate, quoteId } = await journaled(
      journalPath,
      "assigned",
      inbound,
    );
    const quoted = journal(journalPath).some(
      (line) => line.kind === "quoted" && line.quoteId === quoteId,
    );
    if (
      direction !== "In" ||
      amount !== IN_AMOUNT.toString() ||
      lastUpdate !== undefined ||
      !quoteId ||
      !quoted
    ) {
      throw new Error(
        `unexpected assignment ${stringify({ direction, amount, lastUpdate, quoteId, quoted })}`,
      );
    }
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "presented" &&
        line.intent === inbound &&
        (line.providers ?? []).length > 0 &&
        (line.quotes ?? []).some((row) => row.state === `Quoted:${quoteId}`),
      `the provider and quote lists for ${inbound}`,
    );
    return "the host listed the provider, its worker quoted, and the core assigned it the session on that quote";
  });

  await check("provider_present_frame", async () => {
    await stepDone(journalPath, inbound, "present_frame");
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "frame" &&
        line.intent === inbound &&
        line.route === "/kyc",
      "the provider frame",
    );
    return "the host showed the provider's screen and it closed";
  });

  await check("provider_present_page", async () => {
    await stepDone(journalPath, inbound, "present_page");
    return "the host showed the provider's page and handed back its return URL";
  });

  await check("provider_reports_forward_only", async () => {
    await stepDone(journalPath, inbound, "reports_forward_only");
    return "updates moved forward, with a deposit to pay, and a backwards one was refused";
  });

  await check("provider_credits_through_top_up", async () => {
    const { topUpId } = await stepDone(
      journalPath,
      inbound,
      "credits_through_top_up",
    );
    if (!topUpId) throw new Error("the worker named no top-up");
    state = { intent: inbound, topUpId, amount: IN_AMOUNT.toString() };
    return "the top-up was started and named in a Crediting report";
  });

  await check("provider_out_released", async () => {
    outbound = await requestFunding(client, "Out", OUT_AMOUNT);
    const last = await lastStatus(client, outbound);
    if (last?.tag !== "Released" || last.value.debited !== OUT_AMOUNT) {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "the core released the session once the named payment completed";
  });

  await check("provider_payout_retrying", async () => {
    if (!outbound) throw new Error("no released withdrawal");
    return (await stepDone(journalPath, outbound, "payout")).output;
  });

  await check("provider_cancel_refund_after_release", async () => {
    const intent = await requestFunding(client, "Out", RELEASED_CANCEL_AMOUNT);
    await stepDone(journalPath, intent, "payment_requested");
    await cancelAsUser(fundingLogPath, intent, "Requested");
    await stepDone(journalPath, intent, "offer_once_released");
    const last = await lastStatus(client, intent);
    if (
      last?.tag !== "Released" ||
      last.value.debited !== RELEASED_CANCEL_AMOUNT
    ) {
      throw new Error(`was not released: ${stringify(last)}`);
    }
    await stepDone(journalPath, intent, "refund");
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "status" &&
        line.intent === intent &&
        typeof line.progress === "string" &&
        line.progress.startsWith(`Cancelled { refunded: Some(${REFUND})`),
      `${intent} ending cancelled with its refund`,
    );
    return "a released withdrawal the user cancelled on the provider's terms was refunded less the fee";
  });

  await check("provider_cancel", async () => {
    const cancelled = await requestFunding(client, "In", CANCEL_AMOUNT);
    await journaled(journalPath, "assigned", cancelled);
    await cancelAsUser(fundingLogPath, cancelled, "Requested");
    const last = await lastStatus(client, cancelled);
    if (last?.tag !== "Failed" || last.value.reason.tag !== "Cancelled") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    await journaled(journalPath, "cancel", cancelled);
    return "before money moved, the user's cancel reached the provider, which stopped, and the core ended the session";
  });

  await check("provider_cancel_offer_accepted", async () => {
    const intent = await requestFunding(client, "In", OFFER_AMOUNT);
    await stepDone(journalPath, intent, "payment_received");
    await cancelAsUser(fundingLogPath, intent, "Requested");
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "cancel-confirm" &&
        line.intent === intent &&
        line.answer === "Accepted",
      `the user accepting the offer on ${intent}`,
    );
    const settled = await journaled(journalPath, "settled", intent);
    if (settled.decision !== "Accepted") {
      throw new Error(`the provider heard ${stringify(settled)}`);
    }
    const last = await lastStatus(client, intent);
    if (last?.tag !== "Failed" || last.value.reason.tag !== "Cancelled") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "after the payment arrived, the provider offered terms, the user accepted them and the provider settled the cancel";
  });

  await check("provider_cancel_too_late", async () => {
    const intent = await requestFunding(client, "In", LATE_AMOUNT);
    await stepDone(journalPath, intent, "topped_up");
    await cancelAsUser(fundingLogPath, intent, "TooLate");
    const last = await lastStatus(client, intent);
    if (last?.tag !== "Delivered" || last.value.credited !== LATE_AMOUNT) {
      throw new Error(`ended as ${stringify(last)}`);
    }
    if (
      journal(journalPath).some(
        (line) => line.kind === "cancel" && line.intent === intent,
      )
    ) {
      throw new Error(
        "the provider was asked to cancel a session already crediting",
      );
    }
    return "a cancel after the credit started was too late, the provider was not asked, and the session was delivered";
  });

  return { rows, state };
}

/** The run after the restart: the core starts the worker for the session
 *  still open and assigns it where it stopped, and delivers it from what its
 *  top-up claimed. */
export async function runProviderResume(
  client: TrUApiClient,
  fundingLogPath: string,
  journalPath: string,
  state: ProviderState | undefined,
): Promise<DiagnosisRow[]> {
  const rows: DiagnosisRow[] = [];
  const check = rowFactory(rows);
  if (!state) {
    await check("provider_resumes_after_restart", async () => {
      throw new Error("the first run left no session to resume");
    });
    return rows;
  }

  await check("provider_resumes_after_restart", async () => {
    const assigned = await until(
      () =>
        journal(journalPath).findLast(
          (line) =>
            line.kind === "assigned" &&
            line.intent === state.intent &&
            line.lastUpdate !== undefined,
        ),
      `the session ${state.intent} assigned again`,
    );
    if (
      assigned.kind !== "assigned" ||
      assigned.lastUpdate?.tag !== "In" ||
      assigned.lastUpdate.value.tag !== "Crediting" ||
      assigned.lastUpdate.value.value.topUpId !== state.topUpId
    ) {
      throw new Error(`assigned again with ${stringify(assigned)}`);
    }
    await stepDone(journalPath, state.intent, "kept_state");
    return "the core started the worker for the session still open, assigned it with its last update, and the worker's own state was still kept";
  });

  await check("provider_in_delivered", async () => {
    const last = await lastStatus(client, state.intent);
    if (
      last?.tag !== "Delivered" ||
      last.value.credited !== BigInt(state.amount)
    ) {
      throw new Error(`ended as ${stringify(last)}`);
    }
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "status" &&
        line.intent === state.intent &&
        line.tag === "Delivered" &&
        line.amount === state.amount,
      `Delivered reaching the host for ${state.intent}`,
    );
    return "the core delivered the session from the claim of the named top-up";
  });

  return rows;
}
