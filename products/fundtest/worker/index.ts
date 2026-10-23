// The fundtest provider's Worker: serves the FundingProvider trait.
//
// The core calls the worker, and each handler sends one answer: a price for
// an ask, a session to run, a request to cancel one and the user's answer to
// a cancel offer. When the worker connects, the core assigns it every session
// it still serves and repeats the cancel calls it never answered, so a
// restarted worker resumes them from the state each session kept in storage.
// A session stopped at its kill point runs again at once from that state, and
// one an error stopped runs again after a growing wait, as a restart would.
// Calls come in no order between them, so they find their session by intent.

import type { FundingAssignment } from "@parity/truapi";
import { answerAsk } from "./quotes";
import { describe, sleep, truapi } from "./host";
import { loadSaved } from "./saved";
import { Session } from "./session";

/** First wait before a session an error stopped runs again. */
const RESTART_MS = 5_000;
/** Longest wait before a session an error stopped runs again. */
const MAX_RESTART_MS = 10 * 60_000;
/** How long a cancel call waits for the session it names to be assigned. */
const SESSION_WAIT_MS = 30_000;

/** The running sessions, by intent, each once its state is loaded. */
const sessions = new Map<string, Promise<Session>>();
/** Times in a row each session was stopped by an error. */
const restarts = new Map<string, number>();

/** Runs `assignment` from the state it kept in storage, unless it runs
 *  already. */
function start(assignment: FundingAssignment): void {
  const { intent } = assignment;
  if (sessions.has(intent)) return;
  const loading = loadSaved(intent).then(
    (stored) => new Session(assignment, stored, restarts.get(intent)),
  );
  sessions.set(intent, loading);
  void loading
    .then((session) => session.run())
    .then(
      (outcome) => {
        sessions.delete(intent);
        if (outcome === "ended") restarts.delete(intent);
        if (outcome === "killed") start(assignment);
        if (outcome === "retry") restartLater(assignment);
      },
      (error: unknown) => {
        sessions.delete(intent);
        console.error(`[fundtest] ${intent} stopped: ${describe(error)}`);
        restartLater(assignment);
      },
    );
}

/** Runs a session an error stopped again, after a wait that doubles each
 *  time it stops in a row. */
function restartLater(assignment: FundingAssignment): void {
  const count = (restarts.get(assignment.intent) ?? 0) + 1;
  restarts.set(assignment.intent, count);
  const wait = Math.min(RESTART_MS * 2 ** (count - 1), MAX_RESTART_MS);
  setTimeout(() => start(assignment), wait);
}

/** The running session `intent` names, waiting a while for its assignment,
 *  which the core may send after the call. */
async function sessionOf(intent: string): Promise<Session | undefined> {
  const deadline = Date.now() + SESSION_WAIT_MS;
  for (;;) {
    const session = sessions.get(intent);
    if (session) return session;
    if (Date.now() >= deadline) return undefined;
    await sleep(250);
  }
}

async function serve(): Promise<void> {
  const client = await truapi();
  client.fundingProvider.onQuote(({ ask }, send) => {
    send(answerAsk(ask, Date.now()));
  });
  client.fundingProvider.onAssign(({ session }, send) => {
    start(session);
    send(undefined);
  });
  client.fundingProvider.onCancel(({ intent }, send) => {
    void sessionOf(intent)
      .then(
        (session) =>
          session?.cancel() ?? {
            tag: "Refused" as const,
            value: { reason: "This session is not running here" },
          },
      )
      .then(send, (error: unknown) =>
        console.warn(
          `[fundtest] ${intent}: answering the cancel failed: ${describe(error)}`,
        ),
      );
  });
  client.fundingProvider.onSettleCancel(({ intent, decision }, send) => {
    void sessionOf(intent)
      .then((session) =>
        decision.tag === "Accepted"
          ? session?.cancelAccepted()
          : session?.cancelDeclined(),
      )
      .then(
        () => send(undefined),
        (error: unknown) =>
          console.warn(
            `[fundtest] ${intent}: keeping the cancel decision failed: ${describe(error)}`,
          ),
      );
  });
}

void serve().catch((error: unknown) =>
  console.error(`[fundtest] worker stopped: ${describe(error)}`),
);
