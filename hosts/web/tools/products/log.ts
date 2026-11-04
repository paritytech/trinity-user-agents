import type {
  ContentSource,
  ProductLoadEvent,
  ProductLoadObserver,
  ProductLoadSummary,
} from "../../src/products/load-observer.js";

/** One mount's latest load: what the host resolved, and what its loader reported. */
export interface ProductLoadRecord {
  load: number;
  mount: string;
  product: string;
  execution: "App" | "Widget" | "Worker";
  record: string;
  cid: string;
  kind: "car" | "site";
  gateway: string;
  source: ContentSource;
  resolveMs?: number;
  openedAt: number;
  /** Absent until the loader reports. */
  summary?: ProductLoadSummary;
  summaryAt?: number;
  /** When the load's frame closed; absent while it is open. */
  closedAt?: number;
  /** Why the frame never opened, when it did not. */
  closeReason?: string;
}

/** Loads kept, newest first. An older one is dropped when a new mount opens. */
export const MAX_LOADS = 50;

/**
 * The loads of this session, for the inspector's Products view, kept in
 * memory: one record per mount, replaced when the mount is opened again. A
 * report joins its load only, by the load's id; one for a load this log does
 * not hold is ignored.
 *
 * A new session drops the loads whose frames are closed. A frame still open
 * keeps its row, whichever comes first, the session's start or the open:
 * the host closes a session's products and opens the one in front again
 * around a session change, and boots its last product, in no fixed order
 * with the session's start. A row kept that way is dropped once its frame
 * closes.
 */
export class ProductLoadLog {
  private loads = new Map<string, ProductLoadRecord>();
  private listeners = new Set<() => void>();
  private session: string | null = null;
  private droppedLoads = 0;
  /** Loads open when the session started, dropped when their frames close. */
  private carried = new Set<number>();

  constructor(private readonly now: () => number = Date.now) {}

  /** The observer the host's navigation and workers report to. */
  readonly record: ProductLoadObserver = (event: ProductLoadEvent) => {
    if (event.type === "opening") {
      const { type: _type, ...opening } = event;
      const replaced = this.loads.get(event.mount);
      if (replaced !== undefined) this.carried.delete(replaced.load);
      this.loads.delete(event.mount);
      this.loads.set(event.mount, { ...opening, openedAt: this.now() });
      for (const [mount, record] of this.loads) {
        if (this.loads.size <= MAX_LOADS) break;
        this.loads.delete(mount);
        this.carried.delete(record.load);
        this.droppedLoads += 1;
      }
      this.changed();
      return;
    }
    const known = this.loads.get(event.mount);
    if (known === undefined || known.load !== event.load) return;
    if (event.type === "loaded")
      this.loads.set(event.mount, {
        ...known,
        summary: event.summary,
        summaryAt: this.now(),
      });
    else if (this.carried.delete(event.load)) this.loads.delete(event.mount);
    else
      this.loads.set(event.mount, {
        ...known,
        closedAt: this.now(),
        ...(event.reason === undefined ? {} : { closeReason: event.reason }),
      });
    this.changed();
  };

  /** The loads, newest opened first. */
  list(): ProductLoadRecord[] {
    return [...this.loads.values()].reverse();
  }

  /** Loads dropped to keep {@link MAX_LOADS}. */
  dropped(): number {
    return this.droppedLoads;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** The session is `key` now. A new one starts the log again; true when it did. */
  startSession(key: string): boolean {
    if (this.session === key) return false;
    this.session = key;
    this.carried.clear();
    for (const [mount, record] of this.loads)
      if (record.closedAt === undefined) this.carried.add(record.load);
      else this.loads.delete(mount);
    this.droppedLoads = 0;
    this.changed();
    return true;
  }

  private changed(): void {
    for (const listener of this.listeners) listener();
  }
}
