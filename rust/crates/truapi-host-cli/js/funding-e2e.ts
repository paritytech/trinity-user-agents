// End-to-end funding check against the CLI signing host's funding overlay.
//
// The host shows the overlay, the auto-accepting user picks the provider
// `TRUAPI_FUNDING_PROVIDERS` names, and the host launches that provider's
// worker (`scripts/funding-worker.ts`) as its own process once the core asks
// for it. The worker delivers, releases or fails each session through
// top-ups, payments and reports, by the session's direction and amount, so
// the core settles it as it would any other. The host records every request,
// session change and worker launch or stop in TRUAPI_FUNDING_LOG. These cases
// are the product side, and each also reads the host's transcript, so a pass
// cannot rest on the product's word alone.
import { existsSync, readFileSync } from "node:fs";
import type {
  ObservableLike,
  TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import type { HostFundingStatusSubscribeItem } from "../../../../js/packages/truapi/src/generated/types.ts";
import type { DiagnosisRow } from "./diagnosis.ts";
import {
  DELIVERED_AMOUNT,
  DELIVERED_CREDIT,
  FAILED_AMOUNT,
  RELEASED_AMOUNT,
  actAsUser,
  journaled,
  requestFunding,
  until,
} from "./funding-provider-e2e.ts";

export const WAIT_MS = 15_000;

/** A fresh id for a top-up or payment the product starts. */
export function randomId(): `0x${string}` {
  return `0x${crypto.getRandomValues(new Uint8Array(32)).toHex()}`;
}

export interface TranscriptLine {
  kind: string;
  intent?: string;
  direction?: string;
  amount?: string | null;
  outcome?: string;
  tag?: string;
  route?: string;
  answer?: string;
  provider?: string;
  transition?: string;
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

/** The host's launches and stops of `provider`'s worker, in order, each
 *  with its position in the transcript. */
function workerTransitions(path: string, provider: string) {
  return transcript(path).flatMap((line, index) =>
    line.kind === "worker" && line.provider === provider && line.transition
      ? [{ index, transition: line.transition }]
      : [],
  );
}

/** Run sessions the provider delivers, releases and fails, with the core
 *  starting its worker on demand and again after it stopped, one the user
 *  dismisses, and a watch of a session that does not exist. */
export async function runFundingE2e(
  client: TrUApiClient,
  fundingLogPath: string | undefined,
  journalPath: string | undefined,
  provider: string | undefined,
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

  if (!fundingLogPath || !journalPath || !provider) {
    // Without them a pass could not tell "the host showed the overlay, its
    // worker served the session and it settled" from "the product was told
    // it did".
    return [
      row(
        "e2e",
        "skipped",
        "TRUAPI_FUNDING_LOG, TRUAPI_FUNDING_WORKER_LOG or TRUAPI_FUNDING_PROVIDERS not set; cannot read what the host and the worker observed",
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
  ): Promise<string | undefined> => {
    const startedAt = performance.now();
    try {
      const requested = await client.funding.request({ direction, amount });
      if (requested.isErr()) {
        rows.push(
          row(methodName, "fail", stringify(requested.error), startedAt),
        );
        return undefined;
      }
      const { intent } = requested.value;
      const items = await untilComplete(
        client.funding.statusSubscribe({ request: { intent } }),
        `session ${intent}`,
      );
      const presented = await waitForTranscript(
        fundingLogPath,
        (line) => line.kind === "presented" && line.intent === intent,
        `the overlay request for ${intent}`,
      );
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
      return intent;
    } catch (error) {
      // One stuck case is recorded and the rest still run.
      rows.push(row(methodName, "fail", String(error), startedAt));
      return undefined;
    }
  };

  // A case that passes with what `run` returns and fails with what it
  // throws.
  const check = async (methodName: string, run: () => Promise<string>) => {
    const startedAt = performance.now();
    try {
      rows.push(row(methodName, "pass", await run(), startedAt));
    } catch (error) {
      rows.push(
        row(
          methodName,
          "fail",
          error instanceof Error ? error.message : String(error),
          startedAt,
        ),
      );
    }
  };

  // Nothing has asked for the provider's worker yet, so the host runs none.
  const runningBefore =
    workerTransitions(fundingLogPath, provider).at(-1)?.transition === "Start";
  const delivered = await settles(
    "request_in_delivered",
    "In",
    DELIVERED_AMOUNT,
    "InProgress",
    (item) =>
      item.tag === "Delivered" && item.value.credited === DELIVERED_CREDIT,
    { tag: "Delivered", amount: DELIVERED_CREDIT.toString() },
  );

  // The core asked for the worker once the user picked the provider, and the
  // host launched it then: it was not running before, and its launch comes
  // after the overlay in the host's transcript.
  let served: number | undefined;
  await check("worker_started_on_demand", async () => {
    if (runningBefore) {
      throw new Error(
        "the provider's worker was running before anything asked for it",
      );
    }
    if (!delivered) throw new Error("the delivered session never started");
    const presented = transcript(fundingLogPath).findIndex(
      (line) => line.kind === "presented" && line.intent === delivered,
    );
    const started = workerTransitions(fundingLogPath, provider).find(
      ({ index, transition }) => transition === "Start" && index > presented,
    );
    if (presented < 0 || !started) {
      throw new Error("the host never launched the worker after the overlay");
    }
    served = (await journaled(journalPath, "assigned", delivered)).pid;
    return `the host launched the worker after the user picked ${provider}, and it was assigned the session`;
  });

  // Once its last session ended the core let the worker go and the host
  // stopped it; the next session starts a new worker process, which is
  // assigned it.
  await check("worker_reassigned_after_stop", async () => {
    const stopped = await until(
      () =>
        workerTransitions(fundingLogPath, provider)
          .filter(({ transition }) => transition === "Stop")
          .at(-1),
      "the host stopping the worker",
    );
    const intent = await requestFunding(client, "In", DELIVERED_AMOUNT);
    const assigned = await journaled(journalPath, "assigned", intent);
    const restarted = workerTransitions(fundingLogPath, provider).some(
      ({ index, transition }) =>
        transition === "Start" && index > stopped.index,
    );
    if (!restarted || assigned.pid === served) {
      throw new Error(
        `restarted=${restarted}, served by ${served} and then by ${assigned.pid}`,
      );
    }
    const items = await untilComplete(
      client.funding.statusSubscribe({ request: { intent } }),
      `session ${intent}`,
    );
    const last = items.at(-1);
    if (last?.tag !== "Delivered") {
      throw new Error(`ended as ${stringify(last)}`);
    }
    return "the host stopped the worker once its sessions ended, launched a new one for the next session, and it delivered it";
  });

  await settles(
    "request_out_released",
    "Out",
    RELEASED_AMOUNT,
    "InProgress",
    (item) => item.tag === "Released" && item.value.debited === RELEASED_AMOUNT,
    { tag: "Released", amount: RELEASED_AMOUNT.toString() },
  );
  await settles(
    "request_failed",
    "In",
    FAILED_AMOUNT,
    "InProgress",
    (item) =>
      item.tag === "Failed" &&
      item.value.reason.tag === "Other" &&
      item.value.reason.value.code === "provider_failed",
    { tag: "Failed" },
  );

  // A dismissed overlay leaves no session behind. The user switches to
  // manual approval, and with no terminal to ask on the overlay is closed.
  let startedAt = performance.now();
  try {
    actAsUser("/approval manual");
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    const dismissed = await client.funding.request({
      direction: "In",
      amount: 3_000n,
    });
    if (dismissed.isErr()) {
      throw new Error(`funding.request failed: ${stringify(dismissed.error)}`);
    }
    const { intent } = dismissed.value;
    const shown = await waitForTranscript(
      fundingLogPath,
      (line) =>
        line.kind === "presented" &&
        line.intent === intent &&
        line.outcome === "Dismissed",
      "the dismissed overlay",
    );
    const watched = await interruption(
      client.funding.statusSubscribe({ request: { intent } }),
      "a watch of the dismissed session",
    );
    rows.push(
      stringify(watched).includes("NotFound")
        ? row(
            "request_dismissed",
            "pass",
            "the host showed the overlay, the user closed it, and no session was left",
            startedAt,
          )
        : row(
            "request_dismissed",
            "fail",
            `watched=${stringify(watched)} shown=${stringify(shown)}`,
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
        request: { intent: "fs_unknown" },
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
