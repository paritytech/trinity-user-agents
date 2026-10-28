import type { WorkerObserver } from "../../src/products/product-workers.js";
import type { WorkerConsoleEntry } from "../../src/products/worker-console.js";

/** The most lines kept for one worker, across its runs in this session. */
export const LINES_PER_WORKER = 200;

/** A console line as the Workers view keeps it. */
export interface ConsoleLine extends WorkerConsoleEntry {
  /** Unique in this tab, so a drawn line keeps its identity. */
  id: number;
  /** Which run of the worker wrote it. */
  run: number;
}

/**
 * What the product workers wrote to their consoles, for the developer tools'
 * Workers view. Lines are kept in memory, per worker, at most
 * {@link LINES_PER_WORKER} each, the oldest dropped first. A restart keeps
 * the earlier run's lines, each labelled with its run. A new session, or a
 * worker leaving the session, forgets them.
 */
export class WorkerConsoleLog implements WorkerObserver {
  private readonly byWorker = new Map<
    string,
    { lines: ConsoleLine[]; dropped: number }
  >();
  private nextId = 1;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly limit = LINES_PER_WORKER) {}

  sessionChanged(): void {
    this.byWorker.clear();
    this.notify();
  }

  console(
    worker: { name: string; run: number },
    entry: WorkerConsoleEntry,
  ): void {
    let kept = this.byWorker.get(worker.name);
    if (kept === undefined) {
      kept = { lines: [], dropped: 0 };
      this.byWorker.set(worker.name, kept);
    }
    kept.lines.push({ ...entry, id: this.nextId++, run: worker.run });
    if (kept.lines.length > this.limit) {
      const over = kept.lines.length - this.limit;
      kept.lines.splice(0, over);
      kept.dropped += over;
    }
    this.notify();
  }

  /** `name`'s lines, oldest first. */
  lines(name: string): readonly ConsoleLine[] {
    return this.byWorker.get(name)?.lines ?? [];
  }

  /** Lines of `name` dropped to keep the limit since they were last cleared. */
  dropped(name: string): number {
    return this.byWorker.get(name)?.dropped ?? 0;
  }

  clear(name: string): void {
    if (this.byWorker.delete(name)) this.notify();
  }

  /** Forget the lines of every worker not in `names`, as when one is removed. */
  keepOnly(names: ReadonlySet<string>): void {
    let changed = false;
    for (const name of this.byWorker.keys())
      if (!names.has(name)) {
        this.byWorker.delete(name);
        changed = true;
      }
    if (changed) this.notify();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}

const pad = (value: number, width = 2): string =>
  String(value).padStart(width, "0");

/** `HH:MM:SS.mmm` in local time. */
export function clock(at: number): string {
  const date = new Date(at);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
}

const SOURCE_LABELS: Record<WorkerConsoleEntry["source"], string> = {
  console: "",
  error: "uncaught",
  rejection: "unhandled rejection",
  dropped: "not forwarded",
};

/** A line as Copy writes it. */
export function lineText(line: ConsoleLine): string {
  const source = SOURCE_LABELS[line.source];
  return `${clock(line.at)} run ${line.run} ${line.level}${source === "" ? "" : ` (${source})`} ${line.text}`;
}

/** What the console's filter shows. */
export type LevelFilter = "all" | "warnings" | "errors";

export const passes = (line: ConsoleLine, filter: LevelFilter): boolean =>
  filter === "all" ||
  line.level === "error" ||
  (filter === "warnings" && line.level === "warn");
