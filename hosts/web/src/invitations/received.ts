import type { RequestContext } from "./binding.js";
import type { RequestHandle } from "./contract.js";

/**
 * The requests a user's host received, by the host's own handle for each:
 * who sent it, the authenticated request, and the reply the product gave.
 * The Notifications list shows the request; this keeps what only the request
 * service needs. It is kept so a repeat is recognised after a reload, and a
 * dismissed request stays here.
 */

export interface ReceivedRequest {
  handle: RequestHandle;
  /** The sender's person id. Their identity is in `context.from`. */
  from: string;
  context: RequestContext;
  payload: string;
  /** The digest of the exact request. */
  digest: string;
  /** The sealed reply this host posts, once the product replied. */
  replied: { sealed: string; at: number } | null;
}

function isReceived(value: unknown): value is ReceivedRequest {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Partial<ReceivedRequest>;
  return (
    typeof item.handle === "string" &&
    typeof item.from === "string" &&
    typeof item.context?.requestId === "string" &&
    typeof item.payload === "string" &&
    typeof item.digest === "string" &&
    (item.replied === null || typeof item.replied?.sealed === "string")
  );
}

export class ReceivedRequests {
  private items: ReceivedRequest[];

  constructor(
    private readonly storage: Storage,
    private readonly key: string,
  ) {
    this.items = this.load();
  }

  private load(): ReceivedRequest[] {
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(this.key) ?? "[]");
      return Array.isArray(saved) ? saved.filter(isReceived) : [];
    } catch {
      return [];
    }
  }

  private save(): void {
    this.storage.setItem(this.key, JSON.stringify(this.items));
  }

  /** Read the list again, after another document of this origin wrote it. */
  reload(): void {
    this.items = this.load();
  }

  list(): readonly ReceivedRequest[] {
    return this.items;
  }

  get(handle: RequestHandle): ReceivedRequest | undefined {
    return this.items.find((item) => item.handle === handle);
  }

  /** The request `from` sent with `requestId`, whatever its contents. */
  find(from: string, requestId: string): ReceivedRequest | undefined {
    return this.items.find(
      (item) => item.from === from && item.context.requestId === requestId,
    );
  }

  add(item: ReceivedRequest): void {
    this.items.push(structuredClone(item));
    this.save();
  }

  markReplied(handle: RequestHandle, sealed: string, at: number): void {
    const item = this.get(handle);
    if (item === undefined || item.replied !== null) return;
    item.replied = { sealed, at };
    this.save();
  }
}
