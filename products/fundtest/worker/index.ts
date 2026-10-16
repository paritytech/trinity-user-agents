// The fundtest provider's Worker: serves the FundingProvider trait.
//
// One stream carries everything the host asks: a price for an ask, a session
// to run, a request to cancel one and the user's answer to a cancel offer.
// Sessions still in flight, and cancel requests and answers the worker has
// not settled, are handed over again whenever the stream opens, so a
// restarted worker resumes them. A
// session stopped at its kill point, or by an error it could not settle,
// drops what it held in memory and the stream is opened again, which hands it
// back the way a restart would: at once after a kill, after a growing wait
// after an error.

import type { HostFundingServeSubscribeItem } from "@parity/truapi";
import { answerAsk } from "./quotes";
import { describe, sleep, truapi } from "./host";
import { Session } from "./session";

/** Wait before the stream is opened again after it failed or ended. */
const RESUBSCRIBE_MS = 5_000;
/** Longest wait before a session an error stopped is handed out again. */
const MAX_RESTART_MS = 10 * 60_000;

const sessions = new Map<string, Session>();
/** Times in a row each session was stopped by an error. */
const restarts = new Map<string, number>();

/** Closes the open stream, so `serve` opens it again. */
let reopenStream: (() => void) | undefined;

async function answerQuote(
  askId: string,
  ask: Extract<HostFundingServeSubscribeItem, { tag: "Quote" }>["value"]["ask"],
) {
  const client = await truapi();
  await client.fundingProvider
    .answerQuote({ askId, answer: answerAsk(ask, Date.now()) })
    .match(
      () => undefined,
      (error) =>
        console.warn(
          `[fundtest] quote ${askId} not answered: ${describe(error)}`,
        ),
    );
}

function handle(item: HostFundingServeSubscribeItem): void {
  switch (item.tag) {
    case "Quote":
      void answerQuote(item.value.askId, item.value.ask);
      return;
    case "Assigned": {
      const { session } = item.value;
      if (sessions.has(session.intent)) return;
      const running = new Session(session, restarts.get(session.intent));
      sessions.set(session.intent, running);
      void running
        .run()
        .then(
          (outcome) => {
            if (outcome === "ended") restarts.delete(session.intent);
            if (outcome === "killed") reopenStream?.();
            if (outcome === "retry") restartLater(session.intent);
          },
          (error: unknown) =>
            console.error(
              `[fundtest] ${session.intent} stopped: ${describe(error)}`,
            ),
        )
        .finally(() => sessions.delete(session.intent));
      return;
    }
    case "Cancel":
      sessions.get(item.value.intent)?.cancel();
      return;
    case "CancelAccepted":
      sessions.get(item.value.intent)?.cancelAccepted();
      return;
    case "CancelDeclined":
      sessions.get(item.value.intent)?.cancelDeclined();
      return;
  }
}

/** Has a session an error stopped handed out again, after a wait that
 *  doubles each time it stops in a row. */
function restartLater(intent: string): void {
  const count = (restarts.get(intent) ?? 0) + 1;
  restarts.set(intent, count);
  const wait = Math.min(RESUBSCRIBE_MS * 2 ** (count - 1), MAX_RESTART_MS);
  setTimeout(() => reopenStream?.(), wait);
}

/** Serves the stream for as long as the worker lives, reopening it when it
 *  fails or ends. */
async function serve(): Promise<void> {
  const client = await truapi();
  for (;;) {
    await new Promise<void>((resolve) => {
      const subscription = client.fundingProvider.serveSubscribe().subscribe({
        next: handle,
        error: (error) => {
          console.warn(`[fundtest] serve stream failed: ${describe(error)}`);
          resolve();
        },
        complete: resolve,
      });
      reopenStream = () => {
        subscription.unsubscribe();
        resolve();
      };
    });
    reopenStream = undefined;
    await sleep(RESUBSCRIBE_MS);
  }
}

void serve().catch((error: unknown) =>
  console.error(`[fundtest] worker stopped: ${describe(error)}`),
);
