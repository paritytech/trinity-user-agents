import { formatArgs } from "./describe.js";
import {
  WORKER_CONSOLE,
  type ConsoleLevel,
  type ConsoleMessage,
  type ConsoleSource,
} from "../worker-console.js";

/**
 * The console capture a product worker's shell page loads before the
 * worker's entry module (`serve.ts`), so it sees the module from its first
 * statement. It forwards what the worker writes to its console, its uncaught
 * errors and its unhandled rejections to the host page, which shows them in
 * the developer tools' Workers view.
 *
 * The worker sees no difference. Each console method calls the original
 * first, with the same arguments, and returns what it returns. Errors and
 * rejections are only listened to, so the browser still reports them.
 * Forwarding never throws: a line that cannot be described or posted is
 * dropped. Console lines come out at most {@link PER_SECOND} a second, and
 * uncaught errors and rejections at most as many again, so a flood of logs
 * does not hide the error that follows it. Past that they are counted, and
 * the count is sent instead.
 */

const PER_SECOND = 50;
const LEVELS: ConsoleLevel[] = ["debug", "log", "info", "warn", "error"];

function install(): void {
  if (window.parent === window) return;
  const parent = window.parent;
  const origin = location.origin;
  const now = Date.now;
  let forwarding = false;
  let windowStart = 0;
  /** Lines sent in this second: console lines, and errors apart. */
  const sent = { console: 0, error: 0 };
  let dropped = 0;

  const post = (message: ConsoleMessage): void => {
    try {
      parent.postMessage(message, origin);
    } catch {
      // The host is gone or refuses the message. The line stays in the
      // worker's own console.
    }
  };

  const flushDropped = (): void => {
    if (dropped === 0) return;
    post({
      type: WORKER_CONSOLE,
      level: "warn",
      source: "dropped",
      text: `${dropped} console lines not forwarded: more than ${PER_SECOND} a second.`,
    });
    dropped = 0;
  };

  const forward = (
    level: ConsoleLevel,
    source: ConsoleSource,
    text: () => string,
  ): void => {
    if (forwarding) return;
    forwarding = true;
    try {
      const time = now();
      if (time - windowStart >= 1000) {
        flushDropped();
        windowStart = time;
        sent.console = 0;
        sent.error = 0;
      }
      const kind = source === "console" ? "console" : "error";
      if (sent[kind] >= PER_SECOND) {
        if (dropped === 0)
          setTimeout(flushDropped, Math.max(0, windowStart + 1000 - time));
        dropped += 1;
        return;
      }
      sent[kind] += 1;
      post({ type: WORKER_CONSOLE, level, source, text: text() });
    } catch {
      // A value that could not be described is left out.
    } finally {
      forwarding = false;
    }
  };

  for (const level of LEVELS) {
    const original = console[level];
    if (typeof original !== "function") continue;
    console[level] = function (this: unknown, ...args: unknown[]) {
      const result = Reflect.apply(original, this, args) as unknown;
      forward(level, "console", () => formatArgs(args));
      return result;
    };
  }

  window.addEventListener("error", (event) => {
    if (!(event instanceof ErrorEvent)) return;
    forward("error", "error", () =>
      event.error === undefined || event.error === null
        ? `${event.message}${event.filename === "" ? "" : ` (${event.filename}:${event.lineno})`}`
        : formatArgs([event.error]),
    );
  });
  window.addEventListener("unhandledrejection", (event) =>
    forward("error", "rejection", () => formatArgs([event.reason])),
  );
}

install();
