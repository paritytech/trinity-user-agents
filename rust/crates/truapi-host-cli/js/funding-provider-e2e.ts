// End-to-end check of a provider worker serving funding sessions, against a
// host built with `--features test-host` whose scripted overlay answers
// `provide` or `provide-cancel`: it hands the session to the product that
// asked, so this script is both the product requesting funds and the
// provider's worker serving the request.
//
// The run is split across a host restart. `runProviderStart` takes an
// inbound session as far as its top-up, and `runProviderResume`, in a new host
// process on the same storage, checks the session is handed over again and
// lets the core deliver it from the claim. Top-ups and payments go to the
// host's scripted engines, which complete them in full.
import type {
  ObservableLike,
  TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import type {
  FundingUpdate,
  HostFundingServeSubscribeItem,
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

const IN_AMOUNT = 1_000n;
const OUT_AMOUNT = 500n;
const CANCEL_AMOUNT = 300n;
const OFFER_AMOUNT = 250n;
const LATE_AMOUNT = 200n;
const RELEASED_CANCEL_AMOUNT = 400n;
const CANCEL_FEE = 3n;

function randomId(): HexString {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** Every item the provider's serve stream emits, kept as they arrive. */
class Served {
  private readonly items: HostFundingServeSubscribeItem[] = [];
  private failure: unknown;
  private readonly subscription: { unsubscribe(): void };

  /** Every quote ask is answered with a price, as a provider whose API
   *  quotes the amount one to one would. */
  constructor(
    stream: ObservableLike<HostFundingServeSubscribeItem>,
    answer: (askId: string, amount: bigint) => void,
  ) {
    this.subscription = stream.subscribe({
      next: (item) => {
        this.items.push(item);
        if (item.tag === "Quote") {
          answer(item.value.askId, item.value.ask.amount);
        }
      },
      error: (reason: unknown) => {
        this.failure = reason;
      },
      complete: () => {},
    });
  }

  /** The first item matching `predicate`, waiting up to `WAIT_MS`. */
  async next(
    predicate: (item: HostFundingServeSubscribeItem) => boolean,
    what: string,
  ): Promise<HostFundingServeSubscribeItem> {
    const deadline = Date.now() + WAIT_MS;
    for (;;) {
      const hit = this.items.find(predicate);
      if (hit) return hit;
      if (this.failure !== undefined) {
        throw new Error(`serve stream failed: ${stringify(this.failure)}`);
      }
      if (Date.now() >= deadline) {
        throw new Error(`never served ${what}`);
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }

  close() {
    this.subscription.unsubscribe();
  }
}

/** Open the provider's serve stream, answering every quote ask. */
function serve(client: TrUApiClient): Served {
  return new Served(
    client.fundingProvider.serveSubscribe(),
    (askId, amount) => {
      void client.fundingProvider.answerQuote({
        askId,
        answer: {
          tag: "Quoted",
          value: {
            quote: {
              quoteId: `q-${askId}`,
              sendAmount: amount,
              receiveAmount: amount,
              providerFee: 0n,
              networkFee: 0n,
              etaSecs: 60n,
              assetDecimals: 2,
            },
          },
        },
      });
    },
  );
}

const assignedTo = (intent: string) => (item: HostFundingServeSubscribeItem) =>
  item.tag === "Assigned" && item.value.session.intent === intent;

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

async function report(
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

/** The first run: assignment, a provider screen, forward-only reports, the
 *  outbound and cancelled sessions, and an inbound one up to its top-up. */
export async function runProviderStart(
  client: TrUApiClient,
  fundingLogPath: string,
): Promise<{ rows: DiagnosisRow[]; state?: ProviderState }> {
  const rows: DiagnosisRow[] = [];
  const check = rowFactory(rows);
  const served = serve(client);
  let inbound = "";
  let outbound = "";
  let state: ProviderState | undefined;

  await check("provider_assigned", async () => {
    inbound = await requestFunding(client, fundingLogPath, "In", IN_AMOUNT);
    const item = await served.next(
      assignedTo(inbound),
      `the session ${inbound}`,
    );
    if (item.tag !== "Assigned") throw new Error(stringify(item));
    const { direction, amount, lastUpdate, quote } = item.value.session;
    if (
      direction !== "In" ||
      amount !== IN_AMOUNT ||
      lastUpdate !== undefined ||
      !quote?.quoteId.startsWith("q-")
    ) {
      throw new Error(`unexpected assignment ${stringify(item)}`);
    }
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "quotes" &&
        line.intent === inbound &&
        (line.rows ?? []).some(
          (row) => row.state === `Quoted:${quote.quoteId}`,
        ),
      `the quote list for ${inbound}`,
    );
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "candidates" &&
        line.intent === inbound &&
        (line.providers ?? []).length > 0,
      `the provider list for ${inbound}`,
    );
    return "the host listed the provider, its worker quoted, and it was handed the session on that quote";
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
    const toppedUp = await client.payment.topUp({
      amount: IN_AMOUNT,
      source: {
        tag: "ProductAccount",
        value: { derivationIndex: { tag: "Index", value: 0 } },
      },
      id: topUpId,
    });
    if (toppedUp.isErr()) {
      throw new Error(`payment.topUp failed: ${stringify(toppedUp.error)}`);
    }
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

  await check("provider_saves_its_state", async () => {
    if (!state) throw new Error("no session in flight");
    await report(client, state.intent, {
      tag: "Details",
      value: { transactionId: `tx-${state.intent}`, reference: undefined },
    });
    const saved = await client.fundingProvider.save({
      intent: state.intent,
      state: state.topUpId,
    });
    if (saved.isErr()) {
      throw new Error(`save failed: ${stringify(saved.error)}`);
    }
    return "the provider's references and its own state were kept while funds moved";
  });

  await check("provider_out_released", async () => {
    outbound = await requestFunding(client, fundingLogPath, "Out", OUT_AMOUNT);
    await served.next(assignedTo(outbound), `the session ${outbound}`);
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

  await check("provider_cancel", async () => {
    const cancelled = await requestFunding(client, fundingLogPath, "In", CANCEL_AMOUNT);
    await served.next(assignedTo(cancelled), `the session ${cancelled}`);
    await served.next(
      (item) => item.tag === "Cancel" && item.value.intent === cancelled,
      `a cancel of ${cancelled}`,
    );
    await report(client, cancelled, {
      tag: "Failed",
      value: { reason: { tag: "Cancelled" } },
    });
    const last = await lastStatus(client, idOf(cancelled));
    if (last?.tag !== "Failed" || last.value.reason.tag !== "Cancelled") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "the user's cancel reached the provider, which ended the session";
  });

  await check("provider_cancel_offer_accepted", async () => {
    const intent = await requestFunding(client, fundingLogPath, "In", OFFER_AMOUNT);
    await served.next(assignedTo(intent), `the session ${intent}`);
    await report(client, intent, {
      tag: "PaymentReceived",
      value: { finalized: true },
    });
    await served.next(
      (item) => item.tag === "Cancel" && item.value.intent === intent,
      `a cancel of ${intent}`,
    );
    await report(client, intent, {
      tag: "CancelOffer",
      value: { fee: CANCEL_FEE, refund: OFFER_AMOUNT - CANCEL_FEE },
    });
    await served.next(
      (item) => item.tag === "CancelAccepted" && item.value.intent === intent,
      `the user accepting the offer on ${intent}`,
    );
    await report(client, intent, {
      tag: "Failed",
      value: { reason: { tag: "Cancelled" } },
    });
    const last = await lastStatus(client, idOf(intent));
    if (last?.tag !== "Failed" || last.value.reason.tag !== "Cancelled") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "cancel" &&
        line.intent === intent &&
        line.outcome === "Requested",
      `the host's cancel of ${intent}`,
    );
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "cancel-confirm" &&
        line.intent === intent &&
        line.answer === "Accepted",
      `the host accepting the offer on ${intent}`,
    );
    return "after the payment arrived, the user accepted the provider's offer and it settled the cancel";
  });

  await check("provider_cancel_too_late", async () => {
    const intent = await requestFunding(client, fundingLogPath, "In", LATE_AMOUNT);
    await served.next(assignedTo(intent), `the session ${intent}`);
    const topUpId = randomId();
    const toppedUp = await client.payment.topUp({
      amount: LATE_AMOUNT,
      source: {
        tag: "ProductAccount",
        value: { derivationIndex: { tag: "Index", value: 0 } },
      },
      id: topUpId,
    });
    if (toppedUp.isErr()) {
      throw new Error(`payment.topUp failed: ${stringify(toppedUp.error)}`);
    }
    await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "cancel" &&
        line.intent === intent &&
        line.outcome === "TooLate",
      `the host's cancel of ${intent} refused as too late`,
    );
    await report(client, intent, {
      tag: "Crediting",
      value: { topUpId, amount: LATE_AMOUNT },
    });
    await report(client, intent, { tag: "Delivered" });
    const last = await lastStatus(client, idOf(intent));
    if (last?.tag !== "Delivered" || last.value.credited !== LATE_AMOUNT) {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "a cancel after the credit started was too late, and the session was delivered";
  });

  await check("provider_cancel_refund_after_release", async () => {
    const intent = await requestFunding(client, fundingLogPath, "Out", RELEASED_CANCEL_AMOUNT);
    await served.next(assignedTo(intent), `the session ${intent}`);
    const paymentId = randomId();
    const paid = await client.payment.request({
      amount: RELEASED_CANCEL_AMOUNT,
      destination: `0x${"11".repeat(32)}`,
      id: paymentId,
    });
    if (paid.isErr()) {
      throw new Error(`payment.request failed: ${stringify(paid.error)}`);
    }
    await served.next(
      (item) => item.tag === "Cancel" && item.value.intent === intent,
      `a cancel of ${intent}`,
    );
    await report(client, intent, {
      tag: "Collecting",
      value: { paymentId, amount: RELEASED_CANCEL_AMOUNT },
    });
    const released = await lastStatus(client, idOf(intent));
    if (
      released?.tag !== "Released" ||
      released.value.debited !== RELEASED_CANCEL_AMOUNT
    ) {
      throw new Error(`was not released: ${stringify(released)}`);
    }
    const refund = RELEASED_CANCEL_AMOUNT - CANCEL_FEE;
    await report(client, intent, {
      tag: "CancelOffer",
      value: { fee: CANCEL_FEE, refund },
    });
    await served.next(
      (item) => item.tag === "CancelAccepted" && item.value.intent === intent,
      `the user accepting the offer on ${intent}`,
    );
    const topUpId = randomId();
    const toppedUp = await client.payment.topUp({
      amount: refund,
      source: {
        tag: "ProductAccount",
        value: { derivationIndex: { tag: "Index", value: 0 } },
      },
      id: topUpId,
    });
    if (toppedUp.isErr()) {
      throw new Error(`payment.topUp failed: ${stringify(toppedUp.error)}`);
    }
    await report(client, intent, {
      tag: "Refunding",
      value: { topUpId, amount: refund },
    });
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

  served.close();
  return { rows, state };
}

/** The run after the restart: the session is handed over again where it
 *  stopped, and the core delivers it from what its top-up claimed. */
export async function runProviderResume(
  client: TrUApiClient,
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
  const served = serve(client);

  await check("provider_resumes_after_restart", async () => {
    const item = await served.next(
      assignedTo(state.intent),
      `the session ${state.intent}`,
    );
    const last =
      item.tag === "Assigned" ? item.value.session.lastUpdate : undefined;
    const saved =
      item.tag === "Assigned" ? item.value.session.saved : undefined;
    if (
      last?.tag !== "Crediting" ||
      last.value.topUpId !== state.topUpId ||
      saved !== state.topUpId
    ) {
      throw new Error(`replayed ${stringify(item)}`);
    }
    return "the restarted worker was handed the session with its last update and saved state";
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
    return "the core delivered the session from the claim of the named top-up";
  });

  served.close();
  return rows;
}
