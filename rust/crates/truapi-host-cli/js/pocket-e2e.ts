// End-to-end Pocket protocol check against a host that actually serves Pocket
// (`truapi-host <role> --execution-kind worker` with TRUAPI_POCKET_CARDS set).
//
// The host owns the collection, so the two sides have to agree on what
// happened: it seeds a card set, answers removals, and records every removal it
// was asked for in TRUAPI_POCKET_LOG. These cases are the product side, and
// each one that could pass on the product's word alone also reads the host's
// transcript.
import { existsSync, readFileSync } from "node:fs";
import type {
  ObservableLike,
  TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import type { HostPocketListSubscribeItem } from "../../../../js/packages/truapi/src/generated/types.ts";
import type { DiagnosisRow } from "./diagnosis.ts";

/** Cards the battery seeds the host with; see `scripts/battery.sh`. */
export const SEEDED_CARDS = { removable: "loyalty", privileged: "humanity" };

const WAIT_MS = 15_000;

interface TranscriptLine {
  kind: string;
  cardId?: string;
}

/** Removals as the host recorded them. */
function transcript(path: string): TranscriptLine[] {
  if (!existsSync(path)) {
    return [];
  }
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as TranscriptLine);
}

/**
 * First stream item satisfying `predicate`, or a rejection once `WAIT_MS`
 * passes. The CLI runner resolves packages next to this script, where rxjs is
 * not installed, so this subscribes to the stream directly.
 */
function firstMatch<Item>(
  stream: ObservableLike<Item>,
  predicate: (item: Item) => boolean,
  what: string,
): Promise<Item> {
  return new Promise<Item>((resolve, reject) => {
    const timer = setTimeout(() => {
      subscription.unsubscribe();
      reject(new Error(`timed out waiting for ${what}`));
    }, WAIT_MS);
    const settle = (finish: () => void) => {
      clearTimeout(timer);
      // Unsubscribing from inside `next` would race the stream's own
      // bookkeeping, so let it finish delivering first.
      setTimeout(() => subscription.unsubscribe(), 0);
      finish();
    };
    const subscription = stream.subscribe({
      next(item) {
        if (predicate(item)) settle(() => resolve(item));
      },
      error(reason: unknown) {
        settle(() =>
          reject(new Error(`${what} stream failed: ${String(reason)}`)),
        );
      },
      complete() {
        settle(() => reject(new Error(`${what} stream ended first`)));
      },
    });
  });
}

/** Wait until the host's transcript holds a line matching `predicate`. */
async function waitForTranscript(
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

/**
 * Observe the seeded collection, then exercise the three removal rules the
 * protocol declares.
 */
export async function runPocketE2e(
  client: TrUApiClient,
  pocketLogPath: string | undefined,
): Promise<DiagnosisRow[]> {
  const rows: DiagnosisRow[] = [];
  const row = (
    methodName: string,
    status: DiagnosisRow["status"],
    output: string,
    startedAt: number,
  ): DiagnosisRow => ({
    id: `Pocket/${methodName}`,
    serviceName: "Pocket",
    methodName,
    status,
    output,
    durationMs: Math.round(performance.now() - startedAt),
  });

  if (!pocketLogPath) {
    // Without it a pass could not tell "the host removed this card" from
    // "the product was told it did", which is what these cases exist to check.
    return [
      row(
        "e2e",
        "skipped",
        "TRUAPI_POCKET_LOG not set; cannot read what the host observed",
        performance.now(),
      ),
    ];
  }

  // The whole set arrives on subscribe, privileged flags intact.
  let startedAt = performance.now();
  const lists = client.pocket.listSubscribe();
  const first: HostPocketListSubscribeItem = await firstMatch(
    lists,
    () => true,
    "the current card list",
  );
  const removable = first.cards.find(
    (card) => card.cardId === SEEDED_CARDS.removable,
  );
  const privileged = first.cards.find(
    (card) => card.cardId === SEEDED_CARDS.privileged,
  );
  rows.push(
    removable && !removable.privileged && privileged?.privileged
      ? row(
          "list_subscribe",
          "pass",
          `saw ${first.cards.length} cards with the expected privileged flags`,
          startedAt,
        )
      : row("list_subscribe", "fail", JSON.stringify(first), startedAt),
  );

  // A removal reaches the host and the same subscription reports the shrunken
  // set, so the product never has to re-subscribe to see its own change.
  startedAt = performance.now();
  const shrunk = firstMatch(
    lists,
    (item) =>
      !item.cards.some((card) => card.cardId === SEEDED_CARDS.removable),
    "the card list without the removed card",
  );
  const removed = await client.pocket.removeCard({
    cardId: SEEDED_CARDS.removable,
  });
  if (removed.isErr()) {
    rows.push(
      row("remove_card", "fail", JSON.stringify(removed.error), startedAt),
    );
  } else {
    await shrunk;
    await waitForTranscript(
      pocketLogPath,
      (line) =>
        line.kind === "removed" && line.cardId === SEEDED_CARDS.removable,
      "the removal reaching the host",
    );
    rows.push(
      row(
        "remove_card",
        "pass",
        "the host removed the card and the live list shrank",
        startedAt,
      ),
    );
  }

  // A privileged card stays with the typed error; an absent one is already
  // removed, so asking again succeeds.
  startedAt = performance.now();
  const refused = await client.pocket.removeCard({
    cardId: SEEDED_CARDS.privileged,
  });
  const refusedAsPrivileged =
    refused.isErr() &&
    refused.error.tag === "Domain" &&
    refused.error.value.tag === "V1" &&
    refused.error.value.value.tag === "Privileged";
  const absent = await client.pocket.removeCard({ cardId: "never-added" });
  const stillHeld = transcript(pocketLogPath).some(
    (line) =>
      line.kind === "removed" && line.cardId === SEEDED_CARDS.privileged,
  );
  rows.push(
    refusedAsPrivileged && absent.isOk() && !stillHeld
      ? row(
          "remove_card_rules",
          "pass",
          "privileged refused with Privileged and never removed; absent card succeeded",
          startedAt,
        )
      : row(
          "remove_card_rules",
          "fail",
          `privileged=${JSON.stringify(refused)} absent=${JSON.stringify(absent)} removed=${stillHeld}`,
          startedAt,
        ),
  );

  return rows;
}
