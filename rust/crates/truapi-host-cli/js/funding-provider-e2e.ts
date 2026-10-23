// End-to-end check of a provider worker serving funding sessions, against the
// CLI signing host with `TRUAPI_FUNDING_PROVIDERS` naming this product: the
// overlay offers it, the auto-accepting user picks it, so this script is both
// the product requesting funds and the provider's worker serving them.
//
// The core calls the worker with `quote`, `assign`, `cancel` and
// `settleCancel`, which `ScriptedProvider` answers; the worker calls the core with `report` and
// `presentFrame`. The battery acts as the user through the host's commands
// (`actAsUser`), as a person at the terminal would.
//
// The run is split across a host restart. `runProviderStart` takes an
// inbound session as far as its top-up, and `runProviderResume`, in a new host
// process on the same storage, checks the session is assigned again when the
// worker connects, with the worker's own state still in its storage, and lets
// the core deliver it from the claim. Top-ups and payments go to the host's
// ledger engines, which complete them in full.
import { appendFileSync } from "node:fs";
import type { TrUApiClient } from "../../../../js/packages/truapi/src/index.ts";
import type {
  FundingAssignment,
  FundingCancelAnswer,
  FundingCancelDecision,
  FundingUpdate,
  HostFundingStatusSubscribeItem,
} from "../../../../js/packages/truapi/src/generated/types.ts";
import type { DiagnosisRow } from "./diagnosis.ts";
import {
  WAIT_MS,
  fundingId,
  stringify,
  transcript,
  untilComplete,
  waitForTranscript,
} from "./funding-e2e.ts";

/** What the first run leaves for the run after the restart. */
export interface ProviderState {
  intent: string;
  /** The id the product requested the session with, which it watches by. */
  id: `0x${string}`;
  topUpId: string;
  amount: string;
}

type HexString = `0x${string}`;

const IN_AMOUNT = 1_100n;
const OUT_AMOUNT = 550n;
const CANCEL_AMOUNT = 300n;
const OFFER_AMOUNT = 250n;
const LATE_AMOUNT = 200n;
const CANCEL_FEE = 3n;
const RELEASED_CANCEL_AMOUNT = 400n;

function randomId(): HexString {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** Type one command into the host, as the user at its terminal would. The
 *  battery hands the host this pipe as its stdin. */
export function actAsUser(command: string) {
  const pipe = process.env.TRUAPI_FUNDING_USER;
  if (!pipe)
    throw new Error("TRUAPI_FUNDING_USER not set; cannot act as the user");
  appendFileSync(pipe, `${command}\n`);
}

/** Wait until `reached` holds, up to `WAIT_MS`. */
async function until<T>(
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

/** What the provider does with a session it is assigned. */
export type Plan = (
  client: TrUApiClient,
  session: FundingAssignment,
) => Promise<void>;

/** The provider's worker: it prices every ask one to one, takes every
 *  session the core assigns, runs the
 *  plan set for its direction and amount, answers cancels with the answer
 *  set for the session, and settles accepted offers by ending the session.
 *  Every call is recorded, and every call is idempotent, as the core repeats
 *  what is pending when the worker connects. */
export class ScriptedProvider {
  readonly assigned: FundingAssignment[] = [];
  readonly cancels: string[] = [];
  readonly settled: { intent: string; decision: FundingCancelDecision }[] = [];
  private readonly plans = new Map<string, Plan>();
  private readonly started = new Set<string>();
  private readonly cancelAnswers = new Map<
    string,
    FundingCancelAnswer | Promise<FundingCancelAnswer>
  >();
  private readonly onAccepted = new Map<string, () => Promise<void>>();
  private readonly failures: string[] = [];

  constructor(private readonly client: TrUApiClient) {
    client.fundingProvider.onQuote(({ ask }, send) => {
      send({
        tag: "Quoted",
        value: {
          quote: {
            quoteId: `q-${crypto.randomUUID()}`,
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
    client.fundingProvider.onAssign(({ session }, send) => {
      this.assigned.push(session);
      send(undefined);
      const plan = this.plans.get(planKey(session.direction, session.amount));
      if (plan && !this.started.has(session.intent)) {
        this.started.add(session.intent);
        plan(client, session).catch((error) =>
          this.failures.push(`${session.intent}: ${String(error)}`),
        );
      }
    });
    client.fundingProvider.onCancel(({ intent }, send) => {
      this.cancels.push(intent);
      // An answer may wait for the provider's own work, as an offer on a
      // withdrawal waits for its release.
      Promise.resolve(
        this.cancelAnswers.get(intent) ?? {
          tag: "Refused",
          value: { reason: "the battery set no answer" },
        },
      ).then(send);
    });
    client.fundingProvider.onSettleCancel(({ intent, decision }, send) => {
      this.settled.push({ intent, decision });
      send(undefined);
      if (decision.tag === "Accepted") {
        const settle =
          this.onAccepted.get(intent) ??
          (() =>
            report(client, intent, {
              tag: "Failed",
              value: { reason: { tag: "Cancelled" } },
            }));
        settle().catch((error) =>
          this.failures.push(`${intent}: ${String(error)}`),
        );
      }
    });
  }

  /** Run `plan` on the session assigned for `direction` and `amount`. */
  plan(direction: "In" | "Out", amount: bigint, plan: Plan) {
    this.plans.set(planKey(direction, amount), plan);
  }

  /** Answer the cancel of session `intent` with `answer`, once it is
   *  known. */
  answerCancel(
    intent: string,
    answer: FundingCancelAnswer | Promise<FundingCancelAnswer>,
  ) {
    this.cancelAnswers.set(intent, answer);
  }

  /** Settle an accepted cancel of session `intent` with `settle`, in place
   *  of ending it as cancelled. */
  settleAccepted(intent: string, settle: () => Promise<void>) {
    this.onAccepted.set(intent, settle);
  }

  /** The assignment of session `intent`, once the worker holds it. */
  assignment(intent: string): Promise<FundingAssignment> {
    return until(
      () => this.assigned.findLast((session) => session.intent === intent),
      `the session ${intent} assigned`,
    );
  }

  /** What went wrong in a plan run outside a case, if anything. */
  planFailures(): string[] {
    return [...this.failures];
  }
}

const planKey = (direction: string, amount: bigint | undefined) =>
  `${direction}:${amount ?? "any"}`;

function rowFactory(rows: DiagnosisRow[]) {
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

/** The id each session was requested with, by the intent the core keeps it
 *  under. */
const requestedIds = new Map<string, `0x${string}`>();

/** Request a session under a fresh id and return the intent the core keeps
 *  it under, which the provider sees. The core hashes the id with the
 *  product, so the intent is read from the host's record of the request:
 *  the one session it showed for this ask that it had not shown before. */
async function requestFunding(
  client: TrUApiClient,
  fundingLogPath: string,
  direction: "In" | "Out",
  amount: bigint,
): Promise<string> {
  const shownBefore = new Set(
    transcript(fundingLogPath)
      .filter((line) => line.kind === "presented")
      .map((line) => line.intent),
  );
  const id = fundingId();
  const requested = await client.funding.request({ id, direction, amount });
  if (requested.isErr()) {
    throw new Error(`funding.request failed: ${stringify(requested.error)}`);
  }
  const presented = await waitForTranscript(
    fundingLogPath,
    (line) =>
      line.kind === "presented" &&
      line.direction === direction &&
      line.amount === amount.toString() &&
      line.intent !== undefined &&
      !shownBefore.has(line.intent),
    `the overlay request for ${direction} ${amount}`,
  );
  const intent = presented.intent as string;
  requestedIds.set(intent, id);
  return intent;
}

/** The id session `intent` was requested with. */
function idOf(intent: string): `0x${string}` {
  const id = requestedIds.get(intent);
  if (id === undefined) throw new Error(`no request made ${intent}`);
  return id;
}

async function lastStatus(
  client: TrUApiClient,
  id: `0x${string}`,
): Promise<HostFundingStatusSubscribeItem | undefined> {
  const items = await untilComplete(
    client.funding.statusSubscribe({ request: { id } }),
    `session ${id}`,
  );
  return items.at(-1);
}

async function topUp(client: TrUApiClient, topUpId: HexString, amount: bigint) {
  const toppedUp = await client.payment.topUp({
    amount,
    source: {
      tag: "ProductAccount",
      value: { derivationIndex: { tag: "Index", value: 0 } },
    },
    id: topUpId,
  });
  if (toppedUp.isErr()) {
    throw new Error(`payment.topUp failed: ${stringify(toppedUp.error)}`);
  }
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
  provider: ScriptedProvider,
  fundingLogPath: string,
): Promise<{ rows: DiagnosisRow[]; state?: ProviderState }> {
  const rows: DiagnosisRow[] = [];
  const check = rowFactory(rows);
  let inbound = "";
  let outbound = "";
  let state: ProviderState | undefined;

  await check("provider_assigned", async () => {
    inbound = await requestFunding(client, fundingLogPath, "In", IN_AMOUNT);
    const { direction, amount, lastUpdate, quote } =
      await provider.assignment(inbound);
    if (
      direction !== "In" ||
      amount !== IN_AMOUNT ||
      lastUpdate !== undefined ||
      !quote?.quoteId.startsWith("q-")
    ) {
      throw new Error(
        `unexpected assignment ${stringify({ direction, amount, lastUpdate, quote })}`,
      );
    }
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "presented" &&
        line.intent === inbound &&
        (line.providers ?? []).length > 0 &&
        (line.quotes ?? []).some(
          (row) => row.state === `Quoted:${quote.quoteId}`,
        ),
      `the provider and quote lists for ${inbound}`,
    );
    return "the host listed the provider, its worker quoted, and the core assigned it the session on that quote";
  });

  await check("provider_present_frame", async () => {
    const framed = await client.fundingProvider.presentFrame({
      intent: inbound,
      target: { tag: "App", value: { route: "/kyc" } },
    });
    if (framed.isErr() || framed.value.outcome.tag !== "Closed") {
      throw new Error(`presentFrame: ${stringify(framed)}`);
    }
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
    const returnUrl = "https://onramp.example.com/meld/return?flow=buy";
    const framed = await client.fundingProvider.presentFrame({
      intent: inbound,
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
    return "the host showed the provider's page and handed back its return URL";
  });

  await check("provider_reports_forward_only", async () => {
    await report(client, inbound, { tag: "AwaitingPayment" });
    await report(client, inbound, {
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
            uri: undefined,
            expiresAt: undefined,
          },
        },
      },
    });
    await report(client, inbound, {
      tag: "PaymentReceived",
      value: { finalized: true, mismatch: undefined },
    });
    await report(client, inbound, { tag: "Converting" });
    const backwards = await client.fundingProvider.report({
      intent: inbound,
      update: { tag: "AwaitingPayment" },
    });
    const outOfOrder =
      backwards.isErr() && stringify(backwards.error).includes("OutOfOrder");
    if (!outOfOrder) {
      throw new Error(
        `a backwards report was not refused: ${stringify(backwards)}`,
      );
    }
    return "updates moved forward and a backwards one was refused";
  });

  await check("provider_credits_through_top_up", async () => {
    const topUpId = randomId();
    // The worker keeps its own state, and writes it before the top-up, so a
    // restart in between still knows which top-up it started.
    const kept = await client.localStorage.write({
      key: stateKey(inbound),
      value: topUpId,
    });
    if (kept.isErr()) {
      throw new Error(
        `keeping the worker's state failed: ${stringify(kept.error)}`,
      );
    }
    await topUp(client, topUpId, IN_AMOUNT);
    await report(client, inbound, {
      tag: "Crediting",
      value: { topUpId, amount: IN_AMOUNT },
    });
    state = {
      intent: inbound,
      id: idOf(inbound),
      topUpId,
      amount: IN_AMOUNT.toString(),
    };
    return "the top-up was started and named in a Crediting report";
  });

  await check("provider_out_released", async () => {
    outbound = await requestFunding(client, fundingLogPath, "Out", OUT_AMOUNT);
    await provider.assignment(outbound);
    const paymentId = randomId();
    const paid = await client.payment.request({
      amount: OUT_AMOUNT,
      destination: `0x${"11".repeat(32)}`,
      id: paymentId,
    });
    if (paid.isErr()) {
      throw new Error(`payment.request failed: ${stringify(paid.error)}`);
    }
    await report(client, outbound, {
      tag: "Collecting",
      value: { paymentId, amount: OUT_AMOUNT },
    });
    const last = await lastStatus(client, idOf(outbound));
    if (last?.tag !== "Released" || last.value.debited !== OUT_AMOUNT) {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "the core released the session once the named payment completed";
  });

  await check("provider_payout_retrying", async () => {
    if (!outbound) throw new Error("no released withdrawal");
    await report(client, outbound, { tag: "Converting" });
    await report(client, outbound, { tag: "PayoutRetrying" });
    await report(client, outbound, { tag: "PayoutRetrying" });
    await report(client, outbound, {
      tag: "Payout",
      value: { outcome: { tag: "PaidOut" } },
    });
    const late = await client.fundingProvider.report({
      intent: outbound,
      update: { tag: "PayoutRetrying" },
    });
    if (!late.isErr() || !stringify(late.error).includes("OutOfOrder")) {
      throw new Error(
        `a retry after the payout was not refused: ${stringify(late)}`,
      );
    }
    return "retries were taken after the release, and refused once the payout was reported";
  });

  await check("provider_cancel_refund_after_release", async () => {
    const intent = await requestFunding(
      client,
      fundingLogPath,
      "Out",
      RELEASED_CANCEL_AMOUNT,
    );
    await provider.assignment(intent);
    const paymentId = randomId();
    const paid = await client.payment.request({
      amount: RELEASED_CANCEL_AMOUNT,
      destination: `0x${"11".repeat(32)}`,
      id: paymentId,
    });
    if (paid.isErr()) {
      throw new Error(`payment.request failed: ${stringify(paid.error)}`);
    }
    // The provider can offer to cancel a withdrawal only once it is
    // released, so its answer waits for the release.
    const refund = RELEASED_CANCEL_AMOUNT - CANCEL_FEE;
    let release!: () => void;
    const released = new Promise<void>((resolve) => {
      release = resolve;
    });
    provider.answerCancel(
      intent,
      released.then(() => ({
        tag: "Offer" as const,
        value: { fee: CANCEL_FEE, refund },
      })),
    );
    provider.settleAccepted(intent, async () => {
      const topUpId = randomId();
      await topUp(client, topUpId, refund);
      await report(client, intent, {
        tag: "Refunding",
        value: { topUpId, amount: refund },
      });
    });
    await cancelAsUser(fundingLogPath, intent, "Requested");
    await report(client, intent, {
      tag: "Collecting",
      value: { paymentId, amount: RELEASED_CANCEL_AMOUNT },
    });
    const last = await lastStatus(client, idOf(intent));
    if (
      last?.tag !== "Released" ||
      last.value.debited !== RELEASED_CANCEL_AMOUNT
    ) {
      throw new Error(`was not released: ${stringify(last)}`);
    }
    release();
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "status" &&
        line.intent === intent &&
        typeof line.progress === "string" &&
        line.progress.startsWith(`Cancelled { refunded: Some(${refund})`),
      `${intent} ending cancelled with its refund`,
    );
    return "a released withdrawal the user cancelled on the provider's terms was refunded less the fee";
  });

  await check("provider_cancel", async () => {
    const cancelled = await requestFunding(
      client,
      fundingLogPath,
      "In",
      CANCEL_AMOUNT,
    );
    await provider.assignment(cancelled);
    provider.answerCancel(cancelled, { tag: "Stopped" });
    await cancelAsUser(fundingLogPath, cancelled, "Requested");
    const last = await lastStatus(client, idOf(cancelled));
    if (last?.tag !== "Failed" || last.value.reason.tag !== "Cancelled") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    if (!provider.cancels.includes(cancelled)) {
      throw new Error("the provider never heard the cancel");
    }
    return "before money moved, the user's cancel reached the provider, which stopped, and the core ended the session";
  });

  await check("provider_cancel_offer_accepted", async () => {
    const intent = await requestFunding(
      client,
      fundingLogPath,
      "In",
      OFFER_AMOUNT,
    );
    await provider.assignment(intent);
    await report(client, intent, {
      tag: "PaymentReceived",
      value: { finalized: true },
    });
    provider.answerCancel(intent, {
      tag: "Offer",
      value: { fee: CANCEL_FEE, refund: OFFER_AMOUNT - CANCEL_FEE },
    });
    await cancelAsUser(fundingLogPath, intent, "Requested");
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "cancel-confirm" &&
        line.intent === intent &&
        line.answer === "Accepted",
      `the user accepting the offer on ${intent}`,
    );
    const settled = await until(
      () => provider.settled.find((entry) => entry.intent === intent),
      `the provider hearing the decision on ${intent}`,
    );
    if (settled.decision.tag !== "Accepted") {
      throw new Error(`the provider heard ${stringify(settled)}`);
    }
    const last = await lastStatus(client, idOf(intent));
    if (last?.tag !== "Failed" || last.value.reason.tag !== "Cancelled") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "after the payment arrived, the provider offered terms, the user accepted them and the provider settled the cancel";
  });

  await check("provider_cancel_too_late", async () => {
    const intent = await requestFunding(
      client,
      fundingLogPath,
      "In",
      LATE_AMOUNT,
    );
    await provider.assignment(intent);
    const topUpId = randomId();
    await topUp(client, topUpId, LATE_AMOUNT);
    await cancelAsUser(fundingLogPath, intent, "TooLate");
    await report(client, intent, {
      tag: "Crediting",
      value: { topUpId, amount: LATE_AMOUNT },
    });
    await report(client, intent, { tag: "Delivered" });
    const last = await lastStatus(client, idOf(intent));
    if (last?.tag !== "Delivered" || last.value.credited !== LATE_AMOUNT) {
      throw new Error(`ended as ${stringify(last)}`);
    }
    if (provider.cancels.includes(intent)) {
      throw new Error(
        "the provider was asked to cancel a session already crediting",
      );
    }
    return "a cancel after the credit started was too late, the provider was not asked, and the session was delivered";
  });

  return { rows, state };
}

/** The run after the restart: the session is assigned again where it
 *  stopped, and the core delivers it from what its top-up claimed. */
export async function runProviderResume(
  client: TrUApiClient,
  provider: ScriptedProvider,
  fundingLogPath: string,
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
    const { lastUpdate } = await provider.assignment(state.intent);
    if (
      lastUpdate?.tag !== "Crediting" ||
      lastUpdate.value.topUpId !== state.topUpId
    ) {
      throw new Error(`assigned again with ${stringify(lastUpdate)}`);
    }
    const kept = await client.localStorage.read({
      key: stateKey(state.intent),
    });
    if (kept.isErr() || kept.value.value !== state.topUpId) {
      throw new Error(`the worker's own state was ${stringify(kept)}`);
    }
    return "the restarted worker was assigned the session with its last update, and its own state was still kept";
  });

  await check("provider_in_delivered", async () => {
    await report(client, state.intent, { tag: "Delivered" });
    const last = await lastStatus(client, state.id);
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
    await client.localStorage.clear({ key: stateKey(state.intent) });
    return "the core delivered the session from the claim of the named top-up";
  });

  return rows;
}
