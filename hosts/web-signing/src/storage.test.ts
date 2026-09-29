import { describe, expect, test } from "bun:test";
import type { HostLocalStorageChangeItem } from "@parity/truapi";
import { HostStorage } from "./storage.js";
import { MemoryStorage } from "./test-support.js";

const bytes = (...values: number[]) => new Uint8Array(values);

function storageWithExternalChanges() {
  const listeners = new Set<(slot: string | null) => void>();
  const backing = new MemoryStorage();
  const storage = new HostStorage(backing, (listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  });
  const otherTabWrote = (slot: string | null) => {
    for (const listener of listeners) listener(slot);
  };
  return { backing, storage, otherTabWrote, listeners };
}

describe("HostStorage", () => {
  // Tabs share localStorage and each can hold a different wallet, so one
  // wallet's state must not be readable or overwritable from another.
  test("keeps each wallet's product and core state apart", async () => {
    const { storage } = storageWithExternalChanges();
    storage.useWallet("alice");
    await storage.product.write("counter", bytes(1));
    await storage.core.writeCoreStorage({ tag: "AutoSigningKeys" }, bytes(2));

    storage.useWallet("bob");
    expect(await storage.product.read("counter")).toBeUndefined();
    expect(
      await storage.core.readCoreStorage({ tag: "AutoSigningKeys" }),
    ).toBeUndefined();
    await storage.product.write("counter", bytes(9));

    storage.useWallet("alice");
    expect(await storage.product.read("counter")).toEqual(bytes(1));
    expect(
      await storage.core.readCoreStorage({ tag: "AutoSigningKeys" }),
    ).toEqual(bytes(2));
  });

  // Peers address this device by the key, so it must outlive logout and any
  // per-user namespace (`CoreStorageKey::DeviceEncryptionKey`).
  test("keeps the device encryption key across wallets and sign-out", async () => {
    const { storage } = storageWithExternalChanges();
    storage.useWallet("alice");
    await storage.core.writeCoreStorage(
      { tag: "DeviceEncryptionKey" },
      bytes(7),
    );

    storage.useWallet(null);
    expect(
      await storage.core.readCoreStorage({ tag: "DeviceEncryptionKey" }),
    ).toEqual(bytes(7));
    storage.useWallet("bob");
    expect(
      await storage.core.readCoreStorage({ tag: "DeviceEncryptionKey" }),
    ).toEqual(bytes(7));
  });

  test("streams the current value, then writes from this tab and from others", async () => {
    const { backing, storage, otherTabWrote, listeners } =
      storageWithExternalChanges();
    storage.useWallet("alice");
    await storage.product.write("theme", bytes(1));

    const iterator = storage.product
      .subscribeStorage("theme")
      [Symbol.asyncIterator]();
    const next = async (): Promise<HostLocalStorageChangeItem> => {
      const result = await iterator.next();
      if (result.done) throw new Error("stream ended");
      return result.value._unsafeUnwrap();
    };

    expect(await next()).toEqual({ value: "0x01" });
    await storage.product.write("theme", bytes(2));
    expect(await next()).toEqual({ value: "0x02" });

    const [slot] = backing.keys().filter((key) => key.endsWith(":theme"));
    backing.setItem(slot!, "0x03");
    otherTabWrote(slot!);
    expect(await next()).toEqual({ value: "0x03" });

    await storage.product.clear("theme");
    expect(await next()).toEqual({});

    await iterator.return?.();
    expect(listeners.size).toBe(0);
  });
});
