import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { HostLocalStorageChangeItem } from "@parity/truapi";
import { encodeCoreStorageKey } from "@parity/truapi-host";
import type {
  CoreStorage,
  CoreStorageKey,
  ProductStorage,
} from "@parity/truapi-host";
import { normalizeProductId } from "./allowance-ledger.js";
import { subscription } from "./subscription.js";

const PREFIX = "truapi-web-signing-host";
const SIGNED_OUT_SCOPE = "signed-out";
/**
 * The device encryption key outlives logout and any per-user namespacing, as
 * `CoreStorageKey::DeviceEncryptionKey` requires, so it is kept once per
 * browser profile rather than once per wallet.
 */
const INSTALL_SCOPE = "install";

/** Subscribes to writes made by other tabs; returns the unsubscribe. */
export type ExternalChanges = (
  listener: (slot: string | null) => void,
) => () => void;

/**
 * Product and core storage, namespaced by the active wallet.
 *
 * Tabs share `localStorage`, and each tab can hold a different wallet. Without
 * a namespace one wallet's grants, AutoSigning keys and product data would be
 * read, and overwritten, by a session activated from another. Call
 * {@link useWallet} before activating a session and after disconnecting it.
 */
export class HostStorage {
  private scope = SIGNED_OUT_SCOPE;
  private readonly listeners = new Map<string, Set<() => void>>();

  constructor(
    private readonly storage: Storage,
    private readonly externalChanges: ExternalChanges,
  ) {}

  /** Namespace every later read and write under `walletId`, or signed out. */
  useWallet(walletId: string | null): void {
    this.scope = walletId ?? SIGNED_OUT_SCOPE;
  }

  readonly product: ProductStorage = {
    read: (key) => Promise.resolve(this.readBytes(this.productSlot(key))),
    write: (key, value) => {
      this.writeBytes(this.productSlot(key), value);
      return Promise.resolve();
    },
    clear: (key) => {
      this.clearSlot(this.productSlot(key));
      return Promise.resolve();
    },
    subscribeStorage: (key) => {
      const slot = this.productSlot(key);
      return subscription<HostLocalStorageChangeItem>((emit) => {
        const current = (): HostLocalStorageChangeItem => {
          const raw = this.storage.getItem(slot);
          return raw === null ? {} : { value: raw as HexString };
        };
        emit(current());
        const onChange = () => emit(current());
        const local = this.listen(slot, onChange);
        const external = this.externalChanges((changed) => {
          if (changed === slot || changed === null) onChange();
        });
        return () => {
          local();
          external();
        };
      });
    },
  };

  /**
   * Remove what the core stored for `productId` under the active wallet, and
   * nothing else: the key carries the wallet, and the core's own key carries
   * the product and its length, so another product, another wallet, a grant,
   * an allowance and the signing state are all outside the prefix. Returns how
   * many entries were removed. The product must be closed first, so it cannot
   * write them back.
   */
  clearProductData(productId: string): number {
    const id = normalizeProductId(productId);
    const prefix = `${this.productSlot("")}truapi:product-storage:v1:${new TextEncoder().encode(id).length}:${id}:`;
    const slots: string[] = [];
    for (let index = 0; index < this.storage.length; index += 1) {
      const slot = this.storage.key(index);
      if (slot?.startsWith(prefix)) slots.push(slot);
    }
    for (const slot of slots) this.clearSlot(slot);
    return slots.length;
  }

  readonly core: CoreStorage = {
    readCoreStorage: (key) =>
      Promise.resolve(this.readBytes(this.coreSlot(key))),
    writeCoreStorage: (key, value) => {
      this.writeBytes(this.coreSlot(key), value);
      return Promise.resolve();
    },
    clearCoreStorage: (key) => {
      this.clearSlot(this.coreSlot(key));
      return Promise.resolve();
    },
  };

  private productSlot(key: string): string {
    return `${PREFIX}:product:${this.scope}:${key}`;
  }

  private coreSlot(key: CoreStorageKey): string {
    const scope =
      key.tag === "DeviceEncryptionKey" ? INSTALL_SCOPE : this.scope;
    return `${PREFIX}:core:${scope}:${bytesToHex(encodeCoreStorageKey(key))}`;
  }

  private readBytes(slot: string): Uint8Array | undefined {
    const raw = this.storage.getItem(slot);
    return raw === null ? undefined : hexToBytes(raw);
  }

  private writeBytes(slot: string, value: Uint8Array): void {
    this.storage.setItem(slot, bytesToHex(value));
    this.notify(slot);
  }

  private clearSlot(slot: string): void {
    this.storage.removeItem(slot);
    this.notify(slot);
  }

  // A tab never receives `storage` events for its own writes, so those are
  // fanned out here.
  private listen(slot: string, listener: () => void): () => void {
    let listeners = this.listeners.get(slot);
    if (!listeners) {
      listeners = new Set();
      this.listeners.set(slot, listeners);
    }
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) this.listeners.delete(slot);
    };
  }

  private notify(slot: string): void {
    for (const listener of this.listeners.get(slot) ?? []) listener();
  }
}

/** {@link ExternalChanges} for this tab's `localStorage`. */
export function localStorageChanges(
  listener: (slot: string | null) => void,
): () => void {
  const onStorage = (event: StorageEvent) => {
    if (event.storageArea === localStorage) listener(event.key);
  };
  window.addEventListener("storage", onStorage);
  return () => window.removeEventListener("storage", onStorage);
}
