// End-to-end funding check against a host that serves the funding overlay
// (`truapi-host signing-host` built with `--features test-host` and
// TRUAPI_FUNDING_OUTCOMES set).
//
// The host plays the overlay and the provider: it answers each request with
// the next scripted outcome, settles a started session as though its funds
// had moved, and records every request and session change in
// TRUAPI_FUNDING_LOG. These cases are the product side, and each also reads
// the host's transcript, so a pass cannot rest on the product's word alone.
import { existsSync, readFileSync } from "node:fs";
import type {
  ObservableLike,
  TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import type { HostFundingStatusSubscribeItem } from "../../../../js/packages/truapi/src/generated/types.ts";
import type { DiagnosisRow } from "./diagnosis.ts";

// The cases make their requests in the order `scripts/battery.sh` scripts the
// host's outcomes: deliver:900, release:500, fail, dismiss.

export const WAIT_MS = 15_000;

export interface TranscriptLine {
  kind: string;
  intent?: string;
  direction?: string;
  amount?: string | null;
  outcome?: string;
  tag?: string;
  route?: string;
  providers?: string[];
  rows?: { provider: string; state: string }[];
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

/** Run the scripted sessions: delivered, released, failed, dismissed, and a
 *  watch of a session that does not exist. */
export async function runFundingE2e(
  client: TrUApiClient,
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
      const requested = await client.funding.request({ direction, amount });
      if (requested.isErr()) {
        rows.push(
          row(methodName, "fail", stringify(requested.error), startedAt),
        );
        return;
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
    } catch (error) {
      // One stuck case is recorded and the rest still run.
      rows.push(row(methodName, "fail", String(error), startedAt));
    }
  };

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

  // A dismissed overlay is a refusal, and leaves no session behind.
  let startedAt = performance.now();
  try {
    const dismissed = await client.funding.request({
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
        line.outcome === "Dismiss" &&
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
  }

  // A session that does not exist, or is not the caller's, is not found.
  startedAt = performance.now();
  try {
    const refused = await interruption(
      client.funding.statusSubscribe({
        request: { intent: "fs_never_opened" },
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
