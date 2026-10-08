// The fundtest provider's Worker: serves the FundingProvider trait.
//
// One stream carries everything the host asks: a price for an ask, a session
// to run, a request to cancel one. Sessions still in flight are handed over
// again whenever the stream opens, so a restarted worker resumes them.

import type { HostFundingServeSubscribeItem } from "@parity/truapi";
import { answerAsk } from "./quotes";
import { describe, sleep, truapi } from "./host";
import { Session } from "./session";

/** Wait before the stream is opened again after it failed or ended. */
const RESUBSCRIBE_MS = 5_000;

const sessions = new Map<string, Session>();

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
      const running = new Session(session);
      sessions.set(session.intent, running);
      void running.run().finally(() => sessions.delete(session.intent));
      return;
    }
    case "Cancel":
      sessions.get(item.value.intent)?.cancel();
      return;
  }
}

/** Serves the stream for as long as the worker lives, reopening it when it
 *  fails or ends. */
async function serve(): Promise<void> {
  const client = await truapi();
  for (;;) {
    await new Promise<void>((resolve) => {
      client.fundingProvider.serveSubscribe().subscribe({
        next: handle,
        error: (error) => {
          console.warn(`[fundtest] serve stream failed: ${describe(error)}`);
          resolve();
        },
        complete: resolve,
      });
    });
    await sleep(RESUBSCRIBE_MS);
  }
}

void serve().catch((error: unknown) =>
  console.error(`[fundtest] worker stopped: ${describe(error)}`),
);
