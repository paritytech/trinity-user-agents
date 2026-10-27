// End-to-end funding check against the CLI signing host's funding overlay.
//
// The host shows the overlay, the auto-accepting user picks the provider
// `TRUAPI_FUNDING_PROVIDERS` names, and the provider's worker, which is this
// same script (`ScriptedProvider`), delivers, releases or fails each session
// through top-ups, payments and reports, so the core settles it as it would
// any other. The host records every request and session change in
// TRUAPI_FUNDING_LOG. These cases are the product side, and each also reads
// the host's transcript, so a pass cannot rest on the product's word alone.
import { existsSync, readFileSync } from "node:fs";
import type {
  ObservableLike,
  TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import type { HostFundingStatusSubscribeItem } from "../../../../js/packages/truapi/src/generated/types.ts";
import type { DiagnosisRow } from "./diagnosis.ts";
import {
  type ScriptedProvider,
  actAsUser,
  report,
} from "./funding-provider-e2e.ts";

export const WAIT_MS = 15_000;

/** A fresh id for one funding request; the product watches its session by it. */
export function fundingId(): `0x${string}` {
  return `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
}

export interface TranscriptLine {
  kind: string;
  intent?: string;
  direction?: string;
  amount?: string | null;
  outcome?: string;
  answer?: string;
  tag?: string;
  progress?: string | null;
  route?: string;
  providers?: string[];
  quotes?: { provider: string; state: string }[];
}

/** What the host recorded. */
export function transcript(path: string): TranscriptLine[] {
  if (!existsSync(path)) {
    return [];
  }
  // A line the host is still writing is skipped; the next read sees it whole.
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.length > 0)
    .flatMap((line) => {
      try {
        return [JSON.parse(line) as TranscriptLine];
      } catch {
        return [];
      }
    });
}

/** Every item a stream emits until it completes, or a rejection once
 *  `WAIT_MS` passes or the stream fails. */
export function untilComplete<Item>(
  stream: ObservableLike<Item>,
  what: string,
): Promise<Item[]> {
  return new Promise<Item[]>((resolve, reject) => {
    const items: Item[] = [];
    const timer = setTimeout(() => {
      subscription.unsubscribe();
      reject(new Error(`timed out waiting for ${what} to end`));
    }, WAIT_MS);
    const subscription = stream.subscribe({
      next(item) {
        items.push(item);
      },
      error(reason: unknown) {
        clearTimeout(timer);
        reject(new Error(`${what} stream failed: ${JSON.stringify(reason)}`));
      },
      complete() {
        clearTimeout(timer);
        resolve(items);
      },
    });
  });
}

/** The stream's interruption, or a rejection if it emits or completes. */
function interruption<Item>(
  stream: ObservableLike<Item>,
  what: string,
): Promise<unknown> {
  return new Promise<unknown>((resolve, reject) => {
    const timer = setTimeout(() => {
      subscription.unsubscribe();
      reject(new Error(`timed out waiting for ${what} to be refused`));
    }, WAIT_MS);
    const subscription = stream.subscribe({
      next(item) {
        clearTimeout(timer);
        reject(new Error(`${what} emitted ${JSON.stringify(item)}`));
      },
      error(reason: unknown) {
        clearTimeout(timer);
        resolve(reason);
      },
      complete() {
        clearTimeout(timer);
        reject(new Error(`${what} completed instead of being refused`));
      },
    });
  });
}

/** Wait until the host's transcript holds a line matching `predicate`. */
export async function waitForTranscript(
  path: string,
  predicate: (line: TranscriptLine) => boolean,
  what: string,
): Promise<TranscriptLine> {
  const deadline = Date.now() + WAIT_MS;
  for (;;) {
    const hit = transcript(path).find(predicate);
    if (hit) return hit;
    if (Date.now() >= deadline) {
      throw new Error(`host transcript never recorded ${what}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

export const stringify = (value: unknown) =>
  JSON.stringify(value, (_, inner) =>
    typeof inner === "bigint" ? inner.toString() : inner,
  );

/** Run sessions the provider delivers, releases and fails, one the user
 *  dismisses, and a watch of a session that does not exist. */
export async function runFundingE2e(
  client: TrUApiClient,
  provider: ScriptedProvider,
  fundingLogPath: string | undefined,
): Promise<DiagnosisRow[]> {
  const rows: DiagnosisRow[] = [];
  const row = (
    methodName: string,
    status: DiagnosisRow["status"],
    output: string,
    startedAt: number,
  ): DiagnosisRow => ({
    id: `Funding/${methodName}`,
    serviceName: "Funding",
    methodName,
    status,
    output,
    durationMs: Math.round(performance.now() - startedAt),
  });

  if (!fundingLogPath) {
    // Without it a pass could not tell "the host showed the overlay and the
    // session settled" from "the product was told it did".
    return [
      row(
        "e2e",
        "skipped",
        "TRUAPI_FUNDING_LOG not set; cannot read what the host observed",
        performance.now(),
      ),
    ];
  }

  // A started session is watched from its first stage to one terminal item,
  // and the host saw the same request and the same ending.
  const settles = async (
    methodName: string,
    direction: "In" | "Out",
    amount: bigint,
    first: HostFundingStatusSubscribeItem["tag"],
    terminal: (item: HostFundingStatusSubscribeItem) => boolean,
    hostStatus: { tag: string; amount?: string },
  ) => {
    const startedAt = performance.now();
    try {
      const id = fundingId();
      const requested = await client.funding.request({ id, direction, amount });
      if (requested.isErr()) {
        rows.push(
          row(methodName, "fail", stringify(requested.error), startedAt),
        );
        return;
      }
      const items = await untilComplete(
        client.funding.statusSubscribe({ request: { id } }),
        `session ${id}`,
      );
      // The host keeps the session under the id hashed with the product, so
      // its transcript is matched by what the case asked for, which no other
      // case asks for.
      const presented = await waitForTranscript(
        fundingLogPath,
        (line) =>
          line.kind === "presented" &&
          line.direction === direction &&
          line.amount === amount.toString(),
        `the overlay request for ${direction} ${amount}`,
      );
      const intent = presented.intent;
      await waitForTranscript(
        fundingLogPath,
        (line) =>
          line.kind === "status" &&
          line.intent === intent &&
          line.tag === hostStatus.tag &&
          (hostStatus.amount === undefined ||
            line.amount === hostStatus.amount),
        `${hostStatus.tag} reaching the host for ${intent}`,
      );
      const last = items.at(-1);
      const ok =
        items[0]?.tag === first &&
        last !== undefined &&
        terminal(last) &&
        presented.direction === direction &&
        presented.amount === amount.toString();
      rows.push(
        ok
          ? row(
              methodName,
              "pass",
              `${items.map((item) => item.tag).join(" → ")}; the host saw the request and the ending`,
              startedAt,
            )
          : row(
              methodName,
              "fail",
              `items=${stringify(items)} presented=${stringify(presented)}`,
              startedAt,
            ),
      );
    } catch (error) {
      // One stuck case is recorded and the rest still run.
      rows.push(row(methodName, "fail", String(error), startedAt));
    }
  };

  // The provider credits 900 of an inbound 1000, pays out an outbound 500
  // in full, and fails an inbound 2000.
  provider.plan("In", 1_000n, async (client, session) => {
    const topUpId = fundingId();
    const toppedUp = await client.payment.topUp({
      amount: 900n,
      source: {
        tag: "ProductAccount",
        value: { derivationIndex: { tag: "Index", value: 0 } },
      },
      id: topUpId,
      funding: session.intent,
    });
    if (toppedUp.isErr()) throw new Error(stringify(toppedUp.error));
    await report(client, session.intent, {
      tag: "Crediting",
      value: { topUpId, amount: 900n },
    });
    await report(client, session.intent, { tag: "Delivered" });
  });
  provider.plan("Out", 500n, async (client, session) => {
    const paymentId = fundingId();
    const paid = await client.payment.request({
      amount: 500n,
      destination: `0x${"22".repeat(32)}`,
      id: paymentId,
      funding: session.intent,
    });
    if (paid.isErr()) throw new Error(stringify(paid.error));
    await report(client, session.intent, {
      tag: "Collecting",
      value: { paymentId, amount: 500n },
    });
  });
  provider.plan("In", 2_000n, async (client, session) => {
    // Failing at once would end the session before the product watches it,
    // and the case checks the product sees it in progress first.
    await new Promise((resolve) => setTimeout(resolve, 500));
    await report(client, session.intent, {
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
  });

  await settles(
    "request_in_delivered",
    "In",
    1_000n,
    "InProgress",
    (item) => item.tag === "Delivered" && item.value.credited === 900n,
    { tag: "Delivered", amount: "900" },
  );
  await settles(
    "request_out_released",
    "Out",
    500n,
    "InProgress",
    (item) => item.tag === "Released" && item.value.debited === 500n,
    { tag: "Released", amount: "500" },
  );
  await settles(
    "request_failed",
    "In",
    2_000n,
    "InProgress",
    (item) =>
      item.tag === "Failed" &&
      item.value.reason.tag === "Other" &&
      item.value.reason.value.code === "provider_failed",
    { tag: "Failed" },
  );

  // A dismissed overlay is a refusal, and leaves no session behind. The user
  // switches to manual approval, and with no terminal to ask on the overlay
  // is closed.
  let startedAt = performance.now();
  try {
    actAsUser("/approval manual");
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    const dismissed = await client.funding.request({
      id: fundingId(),
      direction: "In",
      amount: 3_000n,
    });
    const dismissedAsRejected =
      dismissed.isErr() &&
      dismissed.error.tag === "Domain" &&
      dismissed.error.value.tag === "V1" &&
      dismissed.error.value.value.tag === "Rejected";
    const shown = await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "presented" &&
        line.outcome === "Dismissed" &&
        line.direction === "In" &&
        line.amount === "3000",
      "the dismissed overlay",
    );
    rows.push(
      dismissedAsRejected
        ? row(
            "request_dismissed",
            "pass",
            "the host showed the overlay, the user closed it, the product was told Rejected",
            startedAt,
          )
        : row(
            "request_dismissed",
            "fail",
            `result=${stringify(dismissed)} shown=${stringify(shown)}`,
            startedAt,
          ),
    );
  } catch (error) {
    rows.push(row("request_dismissed", "fail", String(error), startedAt));
  } finally {
    // The provider cases after this one need the user to approve again.
    actAsUser("/approval automatic");
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }

  // A session that does not exist, or is not the caller's, is not found.
  startedAt = performance.now();
  try {
    const refused = await interruption(
      client.funding.statusSubscribe({
        request: { id: fundingId() },
      }),
      "a watch of an unknown session",
    );
    const notFound = stringify(refused).includes("NotFound");
    rows.push(
      notFound
        ? row(
            "status_subscribe_unknown",
            "pass",
            "refused with NotFound",
            startedAt,
          )
        : row(
            "status_subscribe_unknown",
            "fail",
            stringify(refused),
            startedAt,
          ),
    );
  } catch (error) {
    rows.push(
      row("status_subscribe_unknown", "fail", String(error), startedAt),
    );
  }

  return rows;
}
