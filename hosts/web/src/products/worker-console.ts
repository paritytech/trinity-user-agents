import { redactUrls } from "../platform/activity.js";

/**
 * What a product worker's frame tells the host about its own console: one
 * line per `console` call, uncaught error or unhandled rejection. The worker
 * shell loads a capture script (`loader/console.ts`) before the worker's
 * entry module, so lines from the module's first statement on are seen.
 *
 * A line is the worker's own text. It can hold whatever the product logs,
 * user data included, so the host keeps it in memory for the developer tools
 * and nowhere else. The frame that sends it is its only proof of origin: the
 * host attributes a line to the worker whose frame posted it, never to a name
 * the message carries.
 */

/** The `type` of a console line a worker frame posts to the host page. */
export const WORKER_CONSOLE = "truapi-worker-console";

export type ConsoleLevel = "debug" | "log" | "info" | "warn" | "error";

/** Where a line came from: a `console` call, an uncaught error, an unhandled rejection, or a count of lines not sent. */
export type ConsoleSource = "console" | "error" | "rejection" | "dropped";

/** What a worker frame posts. */
export interface ConsoleMessage {
  type: typeof WORKER_CONSOLE;
  level: ConsoleLevel;
  source: ConsoleSource;
  text: string;
}

/** A console line as the host keeps it. */
export interface WorkerConsoleEntry {
  level: ConsoleLevel;
  source: ConsoleSource;
  text: string;
  /** When the host received it. */
  at: number;
}

/** The most characters one line keeps. Longer text is cut. */
export const CONSOLE_TEXT_LIMIT = 4096;

const LEVELS: readonly string[] = ["debug", "log", "info", "warn", "error"];
const SOURCES: readonly string[] = ["console", "error", "rejection", "dropped"];

/** Cut `text` to {@link CONSOLE_TEXT_LIMIT}, saying how much was left out. */
export function boundText(text: string): string {
  return text.length <= CONSOLE_TEXT_LIMIT
    ? text
    : `${text.slice(0, CONSOLE_TEXT_LIMIT)}… (${text.length - CONSOLE_TEXT_LIMIT} more characters)`;
}

/**
 * The console line in a message from a worker frame, or null when it is not
 * one. The frame's script already bounds what it sends, but the worker's own
 * code can post the same shape, so the text is bounded again here, and any
 * URL in it loses its credentials.
 */
export function readConsoleMessage(
  data: unknown,
  at: number,
): WorkerConsoleEntry | null {
  if (typeof data !== "object" || data === null) return null;
  const { type, level, source, text } = data as Record<string, unknown>;
  if (
    type !== WORKER_CONSOLE ||
    typeof level !== "string" ||
    !LEVELS.includes(level) ||
    typeof source !== "string" ||
    !SOURCES.includes(source) ||
    typeof text !== "string"
  )
    return null;
  return {
    level: level as ConsoleLevel,
    source: source as ConsoleSource,
    text: redactUrls(boundText(text)),
    at,
  };
}
