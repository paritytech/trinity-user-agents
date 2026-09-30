const KEY = "truapi-web-signing-host:sandbox-ports";
const RETIRED_KEY = "truapi-web-signing-host:sandbox-ports-retired";

const LOCK = "truapi-web-signing-host:sandbox-ports";

/** The part of the Web Locks API the ledger needs. */
export type Locks = Pick<LockManager, "request">;

/**
 * Which port of a range each product owns, kept by the host.
 *
 * A host reached by one name can give products separate origins only by port,
 * so a port must never serve two products: what one stored on it would be there
 * for the next. A product keeps the port it was first given, and a port is
 * never handed to another product. A full range is an error, not a reuse.
 *
 * Every change runs under one Web Lock, so tabs of this host take turns: the
 * ledger sits in storage shared by all of them, and a read followed by a write
 * is not atomic across tabs. A browser with no Web Locks is refused rather
 * than trusted to be alone.
 *
 * The ledger lives in this browser and can be lost. A port it forgot may still
 * hold a product's retained data, which the loader detects and reports; the
 * port is then retired and skipped, not reassigned or wiped.
 */
export class PortLedger {
  private readonly locks: Locks | undefined;

  /** `locks` defaults to the browser's; `null` stands for a browser with none. */
  constructor(
    private readonly storage: Pick<Storage, "getItem" | "setItem">,
    locks?: Locks | null,
  ) {
    this.locks =
      locks === undefined ? globalThis.navigator?.locks : (locks ?? undefined);
  }

  private exclusive<T>(change: () => T): Promise<T> {
    if (this.locks === undefined)
      return Promise.reject(
        new Error(
          "Sandbox ports cannot be assigned safely: this browser has no Web Locks. Use a current browser on https or localhost.",
        ),
      );
    return this.locks.request(LOCK, change);
  }

  private retiredPorts(): number[] {
    try {
      const saved: unknown = JSON.parse(
        this.storage.getItem(RETIRED_KEY) ?? "null",
      );
      return Array.isArray(saved) ? saved.filter(Number.isInteger) : [];
    } catch {
      return [];
    }
  }

  private read(): Record<string, number> {
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(KEY) ?? "null");
      if (typeof saved !== "object" || saved === null || Array.isArray(saved))
        return {};
      return Object.fromEntries(
        Object.entries(saved).filter((entry): entry is [string, number] =>
          Number.isInteger(entry[1]),
        ),
      );
    } catch {
      return {};
    }
  }

  private write(owned: Record<string, number>): void {
    this.storage.setItem(KEY, JSON.stringify(owned));
  }

  /**
   * Stop giving out `port`, and release any product mapped to it: its origin
   * holds data of a product the ledger does not know, so no product may keep
   * it. The mapping goes first, so a write that stops half way leaves a port
   * that is free, which the loader retires again, never one that is retired
   * and still owned.
   */
  retire(port: number): Promise<void> {
    return this.exclusive(() => {
      this.write(
        Object.fromEntries(
          Object.entries(this.read()).filter(([, owned]) => owned !== port),
        ),
      );
      const retired = new Set(this.retiredPorts());
      retired.add(port);
      this.storage.setItem(
        RETIRED_KEY,
        JSON.stringify([...retired].sort((a, b) => a - b)),
      );
    });
  }

  /**
   * The port `label` owns in `first..last`, giving it the lowest free one on
   * first use. A mapping to a retired port counts as none.
   */
  portFor(label: string, first: number, last: number): Promise<number> {
    return this.exclusive(() => {
      const owned = this.read();
      const retired = new Set(this.retiredPorts());
      const existing = owned[label];
      if (existing !== undefined && !retired.has(existing)) {
        if (existing < first || existing > last)
          throw new Error(
            `This product owns port ${existing}, outside the range ${first}-${last}. Widen the range to include it.`,
          );
        return existing;
      }
      delete owned[label];
      const taken = new Set([...Object.values(owned), ...retired]);
      for (let port = first; port <= last; port += 1) {
        if (taken.has(port)) continue;
        this.write({ ...owned, [label]: port });
        return port;
      }
      throw new Error(
        `All ${last - first + 1} sandbox ports are owned or retired. Widen the range.`,
      );
    });
  }
}
