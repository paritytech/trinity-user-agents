import {
  redactUrls,
  type Activity,
  type ActivitySink,
} from "../../src/platform/activity.js";

/** The most events the log keeps. Older ones are dropped first. */
export const ACTIVITY_LIMIT = 500;

/** A recorded event: what was reported, numbered and timed. */
export interface ActivityEvent extends Activity {
  /** Unique within this tab and never reused, so a row keeps its identity. */
  id: number;
  at: number;
}

/**
 * The host's activity log: a bounded, in-memory list of what producers
 * report, kept for this tab only. Nothing is stored or sent. Every text is
 * passed through {@link redactUrls} on the way in, as a backstop for a URL
 * that carries credentials.
 */
export class ActivityLog {
  private events: ActivityEvent[] = [];
  private nextId = 1;
  private evicted = 0;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly limit = ACTIVITY_LIMIT,
    private readonly now: () => number = Date.now,
  ) {}

  readonly record: ActivitySink = (activity) => {
    const { category, action, subject, note, failed, details } = activity;
    this.events.push({
      id: this.nextId++,
      at: this.now(),
      category,
      action: redactUrls(action),
      ...(subject === undefined ? {} : { subject: redactUrls(subject) }),
      ...(note === undefined ? {} : { note: redactUrls(note) }),
      ...(failed === true ? { failed } : {}),
      ...(details === undefined || details.length === 0
        ? {}
        : {
            details: details.map(({ label, value }) => ({
              label,
              value: redactUrls(value),
            })),
          }),
    });
    if (this.events.length > this.limit) {
      const over = this.events.length - this.limit;
      this.events.splice(0, over);
      this.evicted += over;
    }
    this.notify();
  };

  /** Events, oldest first. */
  list(): readonly ActivityEvent[] {
    return this.events;
  }

  /** Events dropped to keep the limit since the log was last cleared. */
  dropped(): number {
    return this.evicted;
  }

  clear(): void {
    this.events = [];
    this.evicted = 0;
    this.notify();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}

/** How each category is named in the log. */
export const CATEGORY_LABELS: Record<Activity["category"], string> = {
  session: "Session",
  products: "Products",
  workers: "Workers",
  network: "Network",
  host: "Host",
};

const pad = (value: number): string => String(value).padStart(2, "0");

/** `HH:MM:SS` in local time. */
export function clock(at: number): string {
  const date = new Date(at);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** The text a search matches, lower case: everything an event shows. */
export function searchText(event: ActivityEvent): string {
  return [
    CATEGORY_LABELS[event.category],
    event.action,
    event.subject ?? "",
    event.note ?? "",
    ...(event.details ?? []).flatMap(({ label, value }) => [label, value]),
  ]
    .join("\n")
    .toLowerCase();
}

/** What the log's filters ask for. */
export interface ActivityFilter {
  category: Activity["category"] | null;
  failedOnly: boolean;
  /** Lower-case text every shown event contains, or empty for any. */
  query: string;
}

export const matches = (
  event: ActivityEvent,
  filter: ActivityFilter,
): boolean =>
  (filter.category === null || event.category === filter.category) &&
  (!filter.failedOnly || event.failed === true) &&
  (filter.query === "" || searchText(event).includes(filter.query));

/** One event as the log's Copy writes it: its line, then each detail indented. */
export function eventText(event: ActivityEvent): string {
  const line = [
    clock(event.at),
    CATEGORY_LABELS[event.category],
    event.failed === true ? "FAILED" : null,
    event.action,
    event.subject ?? null,
    event.note === undefined ? null : `(${event.note})`,
  ]
    .filter((part) => part !== null)
    .join(" ");
  return [
    line,
    ...(event.details ?? []).map(({ label, value }) => `  ${label}: ${value}`),
  ].join("\n");
}
