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

describe("HostStorage networks", () => {
  // Paseo is the network that was there first, so its keys must not move.
  test("keeps the default network's original keys", async () => {
    const { backing, storage } = storageWithExternalChanges();
    storage.useWallet("alice");
    await storage.product.write("counter", bytes(1));
    expect(backing.getItem("truapi-web-signing-host:product:alice:counter")).toBe(
      "0x01",
    );
  });

  // The same wallet on two networks must not share grants, keys or product data.
  test("keeps the same wallet's data apart on two networks", async () => {
    const { storage } = storageWithExternalChanges();
    storage.useWallet("alice", "paseo");
    await storage.product.write("counter", bytes(1));
    await storage.core.writeCoreStorage({ tag: "AutoSigningKeys" }, bytes(2));

    storage.useWallet("alice", "previewnet");
    expect(await storage.product.read("counter")).toBeUndefined();
    expect(
      await storage.core.readCoreStorage({ tag: "AutoSigningKeys" }),
    ).toBeUndefined();
    await storage.product.write("counter", bytes(9));

    storage.useWallet("alice", "paseo");
    expect(await storage.product.read("counter")).toEqual(bytes(1));
    expect(
      await storage.core.readCoreStorage({ tag: "AutoSigningKeys" }),
    ).toEqual(bytes(2));
  });

  // Peers address this device by the key, so it is one per browser profile and
  // takes no network namespace.
  test("keeps the device encryption key shared across networks", async () => {
    const { storage } = storageWithExternalChanges();
    storage.useWallet("alice", "previewnet");
    await storage.core.writeCoreStorage(
      { tag: "DeviceEncryptionKey" },
      bytes(7),
    );

    storage.useWallet("alice", "paseo");
    expect(
      await storage.core.readCoreStorage({ tag: "DeviceEncryptionKey" }),
    ).toEqual(bytes(7));
    storage.useWallet(null, "previewnet");
    expect(
      await storage.core.readCoreStorage({ tag: "DeviceEncryptionKey" }),
    ).toEqual(bytes(7));
  });

  test("resets only the active network's product data", async () => {
    const { storage } = storageWithExternalChanges();
    const coreKey = (productId: string, key: string) =>
      `truapi:product-storage:v1:${new TextEncoder().encode(productId).length}:${productId}:${key}`;
    storage.useWallet("alice", "paseo");
    await storage.product.write(coreKey("app.paseo", "a"), bytes(1));
    storage.useWallet("alice", "previewnet");
    await storage.product.write(coreKey("app.paseo", "a"), bytes(2));

    expect(storage.clearProductData("app.paseo")).toBe(1);

    expect(await storage.product.read(coreKey("app.paseo", "a"))).toBeUndefined();
    storage.useWallet("alice", "paseo");
    expect(await storage.product.read(coreKey("app.paseo", "a"))).toEqual(
      bytes(1),
    );
  });
});

describe("HostStorage.clearProductData", () => {
  /** The key the core hands the host: product-scoped, length-prefixed. */
  const coreKey = (productId: string, key: string) =>
    `truapi:product-storage:v1:${new TextEncoder().encode(productId).length}:${productId}:${key}`;

  // The reset is only worth having if it cannot reach anything else.
  test("removes one product's data for the active wallet and nothing else", async () => {
    const { backing, storage } = storageWithExternalChanges();
    storage.useWallet("alice");
    await storage.product.write(coreKey("app.paseo", "a"), bytes(1));
    await storage.product.write(coreKey("app.paseo", "b"), bytes(2));
    await storage.product.write(coreKey("app.paseo.x", "a"), bytes(3));
    await storage.product.write(coreKey("other.paseo", "a"), bytes(4));
    await storage.core.writeCoreStorage({ tag: "AutoSigningKeys" }, bytes(5));
    await storage.core.writeCoreStorage(
      {
        tag: "PermissionAuthorization",
        value: {
          productId: "app.paseo",
          request: { tag: "IdentityDisclosure" },
        },
      },
      bytes(6),
    );
    await storage.core.writeCoreStorage(
      { tag: "StatementRenewalTargets" },
      bytes(7),
    );
    backing.setItem("page-owned-key", "kept");
    storage.useWallet("bob");
    await storage.product.write(coreKey("app.paseo", "a"), bytes(8));
    await storage.core.writeCoreStorage(
      { tag: "DeviceEncryptionKey" },
      bytes(9),
    );
    storage.useWallet("alice");
    const before = backing.keys().length;

    expect(storage.clearProductData(" App.Paseo ")).toBe(2);

    expect(backing.keys()).toHaveLength(before - 2);
    expect(
      await storage.product.read(coreKey("app.paseo", "a")),
    ).toBeUndefined();
    expect(await storage.product.read(coreKey("app.paseo.x", "a"))).toEqual(
      bytes(3),
    );
    expect(await storage.product.read(coreKey("other.paseo", "a"))).toEqual(
      bytes(4),
    );
    expect(
      await storage.core.readCoreStorage({ tag: "AutoSigningKeys" }),
    ).toEqual(bytes(5));
    expect(
      await storage.core.readCoreStorage({ tag: "StatementRenewalTargets" }),
    ).toEqual(bytes(7));
    expect(backing.getItem("page-owned-key")).toBe("kept");
    storage.useWallet("bob");
    expect(await storage.product.read(coreKey("app.paseo", "a"))).toEqual(
      bytes(8),
    );
    expect(
      await storage.core.readCoreStorage({ tag: "DeviceEncryptionKey" }),
    ).toEqual(bytes(9));
  });

  test("removes nothing for a product that stored nothing", () => {
    const { storage } = storageWithExternalChanges();
    storage.useWallet("alice");
    expect(storage.clearProductData("none.paseo")).toBe(0);
  });
});
