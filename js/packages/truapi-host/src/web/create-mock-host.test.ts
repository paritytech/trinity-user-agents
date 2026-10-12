import { describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";

import type { ChatRoom } from "@parity/truapi";
import type {
  CoreStorageKey,
  ProductContext,
} from "../generated/host-callbacks.js";
import {
  createMockHost,
  MOCK_GENESIS,
  mockRuntimeConfig,
} from "./create-mock-host.js";
import { createWebWorkerPairingHostRuntime } from "./index.js";
import { encodeStatement } from "./loopback-statements.js";

const PRODUCT: ProductContext = { productId: "mock.dot", executionKind: "App" };

/** Lowercase hex without `0x`. */
function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

/**
 * The next `count` items of `subscription`, or a failure naming how many
 * arrived.
 *
 * A dropped item parks the iterator, and waiting on it would end the test as a
 * timeout, which reads as a flake. Racing a timer names the behaviour instead.
 */
async function drain<T>(
  subscription: AsyncIterator<T>,
  count: number,
): Promise<T[]> {
  const items: T[] = [];
  for (let taken = 0; taken < count; taken += 1) {
    const next = await Promise.race([
      subscription.next().then((result) => result.value as T),
      new Promise<"dropped">((resolve) =>
        setTimeout(() => resolve("dropped"), 50),
      ),
    ]);
    if (next === "dropped") {
      throw new Error(
        `subscription delivered ${items.length} of ${count} items`,
      );
    }
    items.push(next as T);
  }
  return items;
}

describe("createMockHost callbacks", () => {
  it("product storage round-trips and is namespaced from core", async () => {
    const { callbacks } = createMockHost();
    await callbacks.productStorage.write("k", new Uint8Array([1, 2, 3]));
    expect(await callbacks.productStorage.read("k")).toEqual(
      new Uint8Array([1, 2, 3]),
    );
    // A product key never collides with a core slot.
    expect(
      await callbacks.coreStorage.readCoreStorage({ tag: "AuthSession" }),
    ).toBeUndefined();
    await callbacks.productStorage.clear("k");
    expect(await callbacks.productStorage.read("k")).toBeUndefined();
  });

  it("core storage round-trips per slot", async () => {
    const { callbacks } = createMockHost();
    const key: CoreStorageKey = {
      tag: "PermissionAuthorization",
      value: { productId: "p", request: { tag: "Device", value: "Camera" } },
    };
    await callbacks.coreStorage.writeCoreStorage(key, new Uint8Array([9]));
    expect(await callbacks.coreStorage.readCoreStorage(key)).toEqual(
      new Uint8Array([9]),
    );
    await callbacks.coreStorage.clearCoreStorage(key);
    expect(await callbacks.coreStorage.readCoreStorage(key)).toBeUndefined();
  });

  it("permissions follow per-capability policy", async () => {
    const { callbacks } = createMockHost({
      devicePermissions: "allow-all",
      remotePermissions: "deny-all",
    });
    expect(await callbacks.permissions.devicePermission(PRODUCT, "Notifications")).toBe(
      "AllowAlways",
    );
    expect(
      await callbacks.permissions.remotePermission(PRODUCT, {
        permission: { tag: "WebRtc" },
      }),
    ).toBe("Deny");
  });

  it("feature support and theme reflect config", async () => {
    const { callbacks } = createMockHost({
      featureSupported: false,
      theme: "Light",
    });
    expect(
      (
        await callbacks.features.featureSupported({
          tag: "Chain",
          value: { genesisHash: "0x00" },
        })
      ).supported,
    ).toBe(false);
    const theme = await callbacks.theme
      .subscribeTheme()
      [Symbol.asyncIterator]()
      .next();
    expect(theme.value).toEqual(
      ok({ name: { tag: "Default" }, variant: "Light" }),
    );
  });

  it("records navigations and assigns monotonic notification ids", async () => {
    const host = createMockHost();
    await host.callbacks.navigation.navigateTo("https://a");
    await host.callbacks.navigation.navigateTo("https://b");
    expect(host.getNavigationLog()).toEqual(["https://a", "https://b"]);

    const first = await host.callbacks.notifications.pushNotification({
      text: "one",
    });
    const second = await host.callbacks.notifications.pushNotification({
      text: "two",
    });
    expect([first.id, second.id]).toEqual([1, 2]);
    expect(host.getNotificationLog().length).toBe(2);
  });

  it("confirms per config and records chain sends", async () => {
    const denied = createMockHost({ confirmUserActions: false });
    expect(
      await denied.callbacks.userConfirmation.confirmUserAction({
        tag: "ResourceAllocation",
        value: { callingProductId: "mock.dot", resources: [] },
      }),
    ).toBe(false);

    const host = createMockHost();
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    conn.send("rpc-1");
    expect(host.sentRpc()).toEqual(["rpc-1"]);
  });

  it("replays scripted chain frames", async () => {
    const host = createMockHost({ chainResponses: ["f1", "f2"] });
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    const frames: string[] = [];
    for await (const frame of conn.responses()) {
      frames.push(frame);
    }
    expect(frames).toEqual(["f1", "f2"]);
  });

  it("records confirmations and cancelled notifications", async () => {
    const host = createMockHost();
    await host.callbacks.userConfirmation.confirmUserAction({
      tag: "ResourceAllocation",
      value: { callingProductId: "mock.dot", resources: [] },
    });
    expect(host.confirmations()).toEqual(["ResourceAllocation"]);

    const { id } = await host.callbacks.notifications.pushNotification({
      text: "x",
    });
    await host.callbacks.notifications.cancelNotification(id);
    expect(host.cancelledNotifications()).toEqual([id]);
  });

  it("chainClosed ends the response stream immediately", async () => {
    const host = createMockHost({ chainClosed: true });
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    const first = await conn.responses()[Symbol.asyncIterator]().next();
    expect(first.done).toBe(true);
  });

  it("preimage insert then lookup round-trips", async () => {
    // The core owns Bulletin submission on current core; the host only
    // retrieves content, so tests seed the content store directly.
    const host = createMockHost();
    const key = host.seedPreimage(new Uint8Array([1, 2, 3]));
    // The key is the content address the core asks for, not an arbitrary
    // digest: the core recomputes blake2b-256 over whatever comes back and
    // reports a mismatch as a miss, so a key derived any other way makes every
    // seeded preimage unreachable through the core. The expected value is the
    // published blake2b-256 of `[1, 2, 3]`, so this fails even if the mock and
    // its Rust sibling change algorithm together.
    expect(hex(key)).toBe(
      "11c0e79b71c3976ccd0c02d1310e2516c08edc9d8b6f57ccd680d63a4d8e72da",
    );
    const found = await host.callbacks.preimage
      .lookupPreimage(key)
      [Symbol.asyncIterator]()
      .next();
    expect(found.value).toEqual(ok(new Uint8Array([1, 2, 3])));
  });

  it("preimage lookup misses on an unknown key", async () => {
    const host = createMockHost();
    host.seedPreimage(new Uint8Array([1, 2, 3]));
    const miss = await host.callbacks.preimage
      .lookupPreimage(new Uint8Array(32).fill(9))
      [Symbol.asyncIterator]()
      .next();
    expect(miss.value).toEqual(ok(undefined));
  });

  it("permission policy can deny device and allow remote", async () => {
    const { callbacks } = createMockHost({
      devicePermissions: "deny-all",
      remotePermissions: "allow-all",
    });
    expect(await callbacks.permissions.devicePermission(PRODUCT, "Notifications")).toBe(
      "Deny",
    );
    expect(
      await callbacks.permissions.remotePermission(PRODUCT, {
        permission: { tag: "WebRtc" },
      }),
    ).toBe("AllowAlways");
  });

  it("records auth-state transitions in order", () => {
    const host = createMockHost();
    host.callbacks.auth.authStateChanged({ tag: "Disconnected" });
    host.callbacks.auth.authStateChanged({
      tag: "Pairing",
      value: { deeplink: "dl" },
    });
    expect(host.authStates().map((state) => state.tag)).toEqual([
      "Disconnected",
      "Pairing",
    ]);
  });

  it("silent chain records sends but never yields a response", async () => {
    const host = createMockHost();
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    conn.send("req");
    expect(host.sentRpc()).toEqual(["req"]);
    // Silent (no frames, not closed): the stream parks rather than yielding or
    // ending, so a race against a timer must be won by the timer.
    const outcome = await Promise.race([
      conn
        .responses()
        [Symbol.asyncIterator]()
        .next()
        .then(() => "yielded" as const),
      new Promise<"parked">((resolve) =>
        setTimeout(() => resolve("parked"), 20),
      ),
    ]);
    expect(outcome).toBe("parked");
  });
});

/** Minimal `Worker` stand-in: records posted messages and lets the test drive
 *  the `message` event by hand, so the provider initializes without real WASM. */
class FakeWorker {
  listeners = new Map<string, Set<(event: unknown) => void>>();
  messages: Record<string, unknown>[] = [];

  addEventListener(name: string, fn: (event: unknown) => void) {
    const set = this.listeners.get(name) ?? new Set();
    set.add(fn);
    this.listeners.set(name, set);
  }

  removeEventListener(name: string, fn: (event: unknown) => void) {
    this.listeners.get(name)?.delete(fn);
  }

  postMessage(message: Record<string, unknown>) {
    this.messages.push(message);
  }

  terminate() {}

  emit(message: Record<string, unknown>) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }
}

describe("createMockHost with createWebWorkerPairingHostRuntime", () => {
  it("initializes a worker provider with the mock callbacks (no real WASM)", async () => {
    const worker = new FakeWorker();
    const host = createMockHost();
    const { productId, ...hostConfig } = mockRuntimeConfig();
    const runtimePromise = createWebWorkerPairingHostRuntime(
      worker as unknown as Worker,
      host.callbacks,
      { hostConfig },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const runtime = await runtimePromise;

    const providerPromise = runtime.createProvider({ productId });
    const createCore = [...worker.messages]
      .reverse()
      .find((m) => m.kind === "createCore");
    expect(createCore).toBeDefined();
    worker.emit({ kind: "coreReady", coreId: createCore!.coreId });

    const provider = await providerPromise;
    expect(provider).toBeDefined();
    const init = worker.messages.find((message) => message.kind === "init");
    expect(init).toBeDefined();

    provider.dispose();
    runtime.dispose();
  });
});

describe("createMockHost control surface", () => {
  const review = (callingProductId: string) =>
    ({
      tag: "ResourceAllocation",
      value: { callingProductId, resources: [] },
    }) as const;

  it("records review payloads, not just kinds", async () => {
    // Two reviews of the same kind with different payloads: a kind-only
    // recording cannot tell these apart.
    const host = createMockHost();
    await host.callbacks.userConfirmation.confirmUserAction(
      review("first.dot"),
    );
    await host.callbacks.userConfirmation.confirmUserAction(
      review("second.dot"),
    );

    expect(host.confirmations()).toEqual([
      "ResourceAllocation",
      "ResourceAllocation",
    ]);
    expect(
      host
        .reviews()
        .map(
          (r) =>
            (r as { value: { callingProductId: string } }).value
              .callingProductId,
        ),
    ).toEqual(["first.dot", "second.dot"]);
  });

  it("answers permissions per capability, overriding the policy", async () => {
    const host = createMockHost({ devicePermissions: "deny-all" });
    const ask = () => host.callbacks.permissions.devicePermission(PRODUCT, "Camera");

    expect(await ask()).toBe("Deny");
    host.grantPermission("Camera");
    expect(await ask()).toBe("AllowAlways");
    // Per permission, not a policy flip.
    expect(
      await host.callbacks.permissions.devicePermission(PRODUCT, "Microphone"),
    ).toBe("Deny");
    expect(host.getGrantedPermissions()).toEqual(["Camera"]);

    host.resetPermission("Camera");
    expect(await ask()).toBe("Deny");
  });

  it("denies whatever was not explicitly granted when enforcing", async () => {
    const host = createMockHost();
    host.setEnforcePermissions(true);
    expect(await host.callbacks.permissions.devicePermission(PRODUCT, "Camera")).toBe(
      "Deny",
    );
    host.grantPermission("Camera");
    expect(await host.callbacks.permissions.devicePermission(PRODUCT, "Camera")).toBe(
      "AllowAlways",
    );
  });

  it("records the surface, key and answer of every permission prompt", async () => {
    const host = createMockHost();
    host.revokePermission("Camera");
    await host.callbacks.permissions.devicePermission(PRODUCT, "Camera");
    expect(host.getPermissionLog()).toEqual([
      {
        tag: "Camera",
        value: "Camera",
        approved: false,
        kind: "device",
        // The lifetime, not just the boolean: a suite asserting a denial was
        // durable rather than one-shot has nothing else to read.
        decision: "Deny",
        timestamp: expect.any(Number),
      },
    ]);
  });

  it("throws descriptively for domains the mock cannot model", () => {
    const host = createMockHost();
    // Never a faked success for a path the real host cannot execute, and the
    // error has to say WHY so the reader knows whether it is a gap or a
    // deliberate limit.
    expect(
      () => (host.payment as unknown as { setBalance: unknown }).setBalance,
    ).toThrow(/no host implements them/);
    expect(
      () => (host.coinPayment as unknown as { transfer: unknown }).transfer,
    ).toThrow(/no host implements them/);
    expect(
      () =>
        (host.statements as unknown as { getSubmitted: unknown }).getSubmitted,
    ).toThrow(/people chain/);
  });

  it("reset returns the mock to its constructed state", async () => {
    const host = createMockHost();
    await host.callbacks.navigation.navigateTo("https://a");
    await host.callbacks.userConfirmation.confirmUserAction(review("mock.dot"));
    host.seedPreimage(new Uint8Array([1]));
    host.setTheme("Light");
    host.setEnforcePermissions(true);
    host.revokePermission("Camera");

    host.reset();

    expect(host.getNavigationLog()).toEqual([]);
    expect(host.reviews()).toEqual([]);
    expect(host.getPermissionLog()).toEqual([]);
    expect(host.getGrantedPermissions()).toEqual([]);
    expect(host.getPreimages()).toEqual([]);
    expect(host.getTheme()).toBe("Dark");
    expect(await host.callbacks.permissions.devicePermission(PRODUCT, "Camera")).toBe(
      "AllowAlways",
    );
  });
});

describe("createMockHost TestHostAPI parity", () => {
  const signRaw = {
    tag: "SignRaw",
    value: { Product: { request: { account: "a", payload: { Bytes: [1] } } } },
  } as const;
  const allocation = {
    tag: "ResourceAllocation",
    value: { callingProductId: "mock.dot", resources: [] },
  } as const;

  it("getSigningLog reports only the reviews that gate a signature", async () => {
    const host = createMockHost();
    // A non-signing review must not appear in a signing log.
    await host.callbacks.userConfirmation.confirmUserAction(allocation);
    await host.callbacks.userConfirmation.confirmUserAction(signRaw);

    expect(host.reviews()).toHaveLength(2);
    const log = host.getSigningLog();
    expect(log).toHaveLength(1);
    expect(log[0].type).toBe("raw");
    expect(log[0].payload).toEqual(signRaw.value);

    host.clearSigningLog();
    expect(host.getSigningLog()).toEqual([]);
  });

  it("getIsAuthenticated follows the last auth state the core reported", async () => {
    const host = createMockHost();
    expect(host.getIsAuthenticated()).toBe(false);
    await host.callbacks.auth.authStateChanged({ tag: "Connected", value: {} });
    expect(host.getIsAuthenticated()).toBe(true);
    await host.callbacks.auth.authStateChanged({ tag: "Disconnected" });
    expect(host.getIsAuthenticated()).toBe(false);
  });

  it("setPermissionBehavior switches the fallback for both prompts", async () => {
    const host = createMockHost();
    host.setPermissionBehavior("deny-all");
    expect(await host.callbacks.permissions.devicePermission(PRODUCT, "Camera")).toBe(
      "Deny",
    );
    expect(
      await host.callbacks.permissions.remotePermission(PRODUCT, {
        permission: { tag: "ChainSubmit" },
      }),
    ).toBe("Deny");
    // An explicit grant still wins over the policy.
    host.grantPermission("Camera");
    expect(await host.callbacks.permissions.devicePermission(PRODUCT, "Camera")).toBe(
      "AllowAlways",
    );
  });

  it("getConnectionStatus and dispose track and release state", async () => {
    const host = createMockHost();
    expect(host.getConnectionStatus()).toBe("Idle");
    host.simulateDisconnect();
    expect(host.getConnectionStatus()).toBe("Disconnected");

    await host.callbacks.navigation.navigateTo("https://a");
    host.dispose();
    expect(host.getNavigationLog()).toEqual([]);
    expect(host.getConnectionStatus()).toBe("Idle");
  });

  it("a connect that fails leaves the status alone", async () => {
    // Reporting Connected for a dial that threw makes the mock disagree with
    // itself, and a suite asserting an offline host reads it as online.
    const host = createMockHost({ chainProxies: [{ rpcUrl: "not a url" }] });
    await expect(
      host.callbacks.chain.connect(new Uint8Array(32)),
    ).rejects.toThrow();

    expect(host.getChainStatus()).toBe("Idle");
  });

  it("connect reports Connected and is refused while disconnected", async () => {
    const host = createMockHost();
    await host.callbacks.chain.connect(new Uint8Array(32));
    expect(host.getConnectionStatus()).toBe("Connected");

    host.simulateDisconnect();
    // A knob that only relabelled the status would let this connect succeed,
    // so a suite testing offline behaviour would never see an offline host.
    await expect(
      host.callbacks.chain.connect(new Uint8Array(32)),
    ).rejects.toThrow("mock chain is disconnected");
    expect(host.getConnectionStatus()).toBe("Disconnected");

    host.simulateReconnect();
    await host.callbacks.chain.connect(new Uint8Array(32));
    expect(host.getConnectionStatus()).toBe("Connected");
  });

  it("serves the three mock chains by default", async () => {
    // The Rust mock declares the same three. A suite that routes on a chain
    // has to be answered the same way by either, and dropping one here would
    // otherwise only show up as a chain-routed call failing much later.
    const host = createMockHost();
    expect(await host.callbacks.features.supportedChains()).toEqual({
      network: "mock",
      chains: [
        { identifier: "People", genesisHash: MOCK_GENESIS.people },
        { identifier: "Bulletin", genesisHash: MOCK_GENESIS.bulletin },
        { identifier: "AssetHub", genesisHash: MOCK_GENESIS.assetHub },
      ],
    });
  });

  it("keys a remote permission on its tag, not the domains it names", async () => {
    // The Rust mock asserts the same thing. Keying on the domains would make
    // the grant below cover only the exact list it was issued for, so it could
    // never be set up before knowing what the product would ask for.
    const host = createMockHost();
    const ask = (domains: string[]) =>
      host.callbacks.permissions.remotePermission(PRODUCT, {
        permission: { tag: "Remote", value: { domains } },
      });

    host.revokePermission("Remote");
    expect(await ask(["a.example"])).toBe("Deny");
    expect(await ask(["b.example", "c.example"])).toBe("Deny");

    host.grantPermission("Remote");
    expect(await ask(["d.example"])).toBe("AllowAlways");
  });

  it("addresses each core storage slot separately", async () => {
    // Keying on the variant alone would put every product's manifest in one
    // slot, so a test writing one and reading another reads back the wrong
    // value instead of finding nothing.
    const host = createMockHost();
    const { coreStorage } = host.callbacks;
    const manifest = (productId: string) =>
      ({ tag: "ProductManifest", value: { productId } }) as const;

    await coreStorage.writeCoreStorage(manifest("a.dot"), new Uint8Array([1]));
    await coreStorage.writeCoreStorage(manifest("b.dot"), new Uint8Array([2]));

    expect(await coreStorage.readCoreStorage(manifest("a.dot"))).toEqual(
      new Uint8Array([1]),
    );
    expect(
      await coreStorage.readCoreStorage(manifest("unwritten.dot")),
    ).toBeUndefined();
  });

  it("lists chat rooms in id order, as the Rust mock does", async () => {
    // Insertion order would put a different list in the subscription payload
    // than the sibling host sends for the same rooms, and that list is
    // protocol payload, not just an assertion oracle.
    const host = createMockHost();
    const product = {
      productId: "p",
      executionKind: { tag: "Unknown" } as const,
    };
    for (const roomId of ["zulu", "alpha", "mike"]) {
      await host.callbacks.chat!.createChatRoom(product, {
        roomId,
        name: roomId,
        icon: "https://example.invalid/i.png",
      });
    }

    expect(host.getChatRooms().map((room) => room.roomId)).toEqual([
      "alpha",
      "mike",
      "zulu",
    ]);
    const rooms = host.callbacks.chat!.subscribeChatRooms();
    const [seeded] = await drain(rooms, 1);
    expect(
      seeded!._unsafeUnwrap().rooms.map((room: ChatRoom) => room.roomId),
    ).toEqual(["alpha", "mike", "zulu"]);
  });

  it("simulateDisconnect ends a live response stream, not just the status", async () => {
    // A knob that relabelled the status and refused reconnect would still
    // leave a product parked on `responses()` forever, which is the one thing
    // a dropped transport must not look like.
    const host = createMockHost();
    const connection = await host.callbacks.chain.connect(new Uint8Array(32));
    const responses = connection.responses();
    const ended = responses.next();

    host.simulateDisconnect();

    expect(
      await Promise.race([
        ended.then((result) => (result.done ? "ended" : "yielded")),
        new Promise<"parked">((resolve) => setTimeout(() => resolve("parked"), 50)),
      ]),
    ).toBe("ended");
  });

  it("closing a subscription that is parked on a read settles both promises", async () => {
    // Unregistering the push without waking the body leaves it waiting for a
    // change that can no longer reach it, so the close and the read it was
    // parked on would both hang rather than end the stream.
    const host = createMockHost();
    const themes = host.callbacks.theme.subscribeTheme();
    await drain(themes, 1);
    const parked = themes.next();
    const closed = themes.return(undefined);
    host.setTheme("Dark");

    expect(
      await Promise.race([
        Promise.all([parked, closed]).then(() => "settled"),
        new Promise<"hung">((resolve) => setTimeout(() => resolve("hung"), 50)),
      ]),
    ).toBe("settled");
    expect((await parked).done).toBe(true);
  });

  it("closing one subscription leaves the others live", async () => {
    // One `release` serves both the generator's `finally` and `return`, so a
    // close that dropped the wrong push would silence a bystander instead.
    const host = createMockHost();
    const closed = host.callbacks.theme.subscribeTheme();
    const live = host.callbacks.theme.subscribeTheme();
    await drain(live, 1);
    await closed.return(undefined);

    host.setTheme("Dark");

    expect(await drain(live, 1)).toEqual([
      ok({ name: { tag: "Default" }, variant: "Dark" }),
    ]);
  });

  it("dispose ends subscriptions that were never closed", async () => {
    // Nothing unwinds a stream still parked on `next()`, so a consumer has to
    // be told the host is gone. Dropping the registration alone would leave it
    // parked forever, which is indistinguishable from a host with no changes
    // to report.
    const host = createMockHost();
    const themes = host.callbacks.theme.subscribeTheme();
    const stored = host.callbacks.productStorage.subscribeStorage("k");
    await drain(themes, 1);
    await drain(stored, 1);
    const parked = [themes.next(), stored.next()];

    host.dispose();

    expect(
      await Promise.race([
        Promise.all(parked).then((results) =>
          results.every((result) => result.done) ? "ended" : "yielded",
        ),
        new Promise<"parked">((resolve) =>
          setTimeout(() => resolve("parked"), 50),
        ),
      ]),
    ).toBe("ended");
  });

  it("a new room reaches a live room subscription", async () => {
    // The room list is a live subscription on Rust, and a product that
    // subscribes before the first room is created is the normal order.
    const host = createMockHost();
    const rooms = host.callbacks.chat!.subscribeChatRooms();
    await host.callbacks.chat!.createChatRoom(
      { productId: "p", executionKind: { tag: "Unknown" } },
      { roomId: "lobby", name: "Lobby", icon: "https://example.invalid/i.png" },
    );

    const seen = await drain(rooms, 2);
    expect(seen[1]).toEqual(ok({ rooms: [...host.getChatRooms()] }));
  });

  it("reset returns the policies and the open operations too", async () => {
    // `reset` says it returns the mock to its constructed state. A policy that
    // survives it lets one case govern the next, which is the whole point.
    const host = createMockHost({ devicePermissions: "allow-all" });
    host.setPermissionBehavior("deny-all");
    await host.callbacks.productOperations.beginOperation(
      { productId: "p", executionKind: { tag: "Unknown" } },
      "sync",
    );

    host.reset();

    expect(host.getOpenOperations()).toEqual([]);
    expect(await host.callbacks.permissions.devicePermission(PRODUCT, "Camera")).toBe(
      "AllowAlways",
    );
  });

  it("setTheme reaches a subscription that has not been read yet", async () => {
    // The change lands before the first `next()`, so nothing has driven the
    // generator body. A subscription that only registers once it is read
    // drops this one and then parks on "Light" forever.
    const host = createMockHost({ theme: "Light" });
    const subscription = host.callbacks.theme.subscribeTheme();
    host.setTheme("Dark");

    expect(await drain(subscription, 2)).toEqual([
      ok({ name: { tag: "Default" }, variant: "Light" }),
      ok({ name: { tag: "Default" }, variant: "Dark" }),
    ]);
  });

  it("a write reaches a subscription that has not been read yet", async () => {
    const host = createMockHost();
    const subscription = host.callbacks.productStorage.subscribeStorage("k");
    await host.callbacks.productStorage.write("k", new Uint8Array([1, 2, 3]));

    expect(await drain(subscription, 2)).toEqual([
      ok({ value: undefined }),
      ok({ value: "0x010203" }),
    ]);
  });

  it("every fault knob refuses the calls its doc names", async () => {
    const storage = createMockHost({ faults: { storageError: "disk" } });
    await expect(storage.callbacks.productStorage.read("k")).rejects.toThrow(
      "disk",
    );
    await expect(
      storage.callbacks.productStorage.write("k", new Uint8Array([1])),
    ).rejects.toThrow("disk");
    await expect(storage.callbacks.productStorage.clear("k")).rejects.toThrow(
      "disk",
    );
    await expect(
      storage.callbacks.coreStorage.readCoreStorage({ tag: "AuthSession" }),
    ).rejects.toThrow("disk");
    await expect(
      storage.callbacks.coreStorage.writeCoreStorage(
        { tag: "AuthSession" },
        new Uint8Array([1]),
      ),
    ).rejects.toThrow("disk");
    await expect(
      storage.callbacks.coreStorage.clearCoreStorage({ tag: "AuthSession" }),
    ).rejects.toThrow("disk");

    const navigation = createMockHost({ faults: { navigateError: "blocked" } });
    await expect(
      navigation.callbacks.navigation.navigateTo("https://a"),
    ).rejects.toThrow("blocked");
    expect(navigation.getNavigationLog()).toEqual([]);

    const notification = createMockHost({
      faults: { notificationError: "denied" },
    });
    await expect(
      notification.callbacks.notifications.pushNotification({ text: "x" }),
    ).rejects.toThrow("denied");
    expect(notification.getNotificationLog()).toEqual([]);

    const permission = createMockHost({
      faults: { permissionError: "no prompt" },
    });
    await expect(
      permission.callbacks.permissions.devicePermission(PRODUCT, "Camera"),
    ).rejects.toThrow("no prompt");
    await expect(
      permission.callbacks.permissions.remotePermission(PRODUCT, {
        permission: { tag: "ChainSubmit" },
      }),
    ).rejects.toThrow("no prompt");
    // A refused prompt was never answered, so it is not a recorded decision.
    expect(permission.getPermissionLog()).toEqual([]);

    const feature = createMockHost({ faults: { featureError: "unknown" } });
    await expect(
      feature.callbacks.features.featureSupported({
        tag: "Chain",
        value: { genesisHash: "0x00" },
      }),
    ).rejects.toThrow("unknown");
    await expect(feature.callbacks.features.supportedChains()).rejects.toThrow(
      "unknown",
    );

    const confirmation = createMockHost({
      faults: { confirmationError: "no ui" },
    });
    const review = {
      tag: "ResourceAllocation",
      value: { callingProductId: "mock.dot", resources: [] },
    } as const;
    await expect(
      confirmation.callbacks.userConfirmation.confirmUserAction(review),
    ).rejects.toThrow("no ui");
    // One knob answers both entry points, as on Rust.
    await expect(
      confirmation.callbacks.userConfirmation.confirmPermission(review),
    ).rejects.toThrow("no ui");
    // Unlike a refused permission prompt, the review is recorded before the
    // throw. Both mocks do this, so a suite reading the log sees the question
    // that could not be put to the user rather than losing it.
    expect(confirmation.confirmations()).toEqual([
      "ResourceAllocation",
      "ResourceAllocation",
    ]);

    const product = {
      productId: "p",
      executionKind: { tag: "Unknown" } as const,
    };
    const chat = createMockHost({ faults: { chatError: "chat down" } });
    await expect(
      chat.callbacks.chat!.createChatRoom(product, {
        roomId: "r",
        name: "R",
        icon: "https://example.invalid/i.png",
      }),
    ).rejects.toThrow("chat down");
    await expect(
      chat.callbacks.chat!.registerChatBot(product, {
        botId: "greeter",
        name: "Greeter",
        icon: "https://example.invalid/i.png",
      }),
    ).rejects.toThrow("chat down");
    await expect(
      chat.callbacks.chat!.postChatMessage(product, {
        roomId: "r",
        payload: { tag: "Text", value: { text: "hi" } },
      }),
    ).rejects.toThrow("chat down");
    // The subscription reports the same fault rather than opening a stream
    // that looks healthy and never carries a room.
    const rooms = chat.callbacks.chat!.subscribeChatRooms();
    expect(await drain(rooms, 1)).toEqual([err({ reason: "chat down" })]);
    // And ends there. A stream that kept the consumer waiting after refusing
    // is the stalled stream this guard exists to avoid.
    expect((await rooms.next()).done).toBe(true);
  });
});

describe("the notification log", () => {
  it("records an entry per push and flips cancelled by id", async () => {
    const host = createMockHost();
    const { callbacks } = host;

    const first = await callbacks.notifications.pushNotification({
      text: "one",
      scheduledAt: 1_700_000_000_000n,
    });
    const second = await callbacks.notifications.pushNotification({
      text: "two",
      deeplink: "https://example.test/x",
    });
    // Ids are what the product cancels by, so they have to be distinct -- and
    // positive, since a product that reads 0 as "no id" cannot cancel by it.
    expect(first.id).not.toBe(second.id);
    expect(first.id).toBeGreaterThan(0);

    const scheduled = host.getNotificationLog();
    expect(scheduled).toHaveLength(2);
    expect(scheduled[0]).toMatchObject({
      id: first.id,
      text: "one",
      scheduledAt: 1_700_000_000_000n,
      cancelled: false,
    });
    expect(scheduled[1]).toMatchObject({
      id: second.id,
      deeplink: "https://example.test/x",
      cancelled: false,
    });

    await callbacks.notifications.cancelNotification(first.id);

    const afterCancel = host.getNotificationLog();
    // The cancelled one flips in place; the other is untouched. Asserting both
    // is what catches a cancel that marks the whole log.
    expect(afterCancel.find((n) => n.id === first.id)?.cancelled).toBe(true);
    expect(afterCancel.find((n) => n.id === second.id)?.cancelled).toBe(false);
  });

  it("hands out copies, so a caller cannot mutate the host's log", async () => {
    const host = createMockHost();
    const { callbacks } = host;
    await callbacks.notifications.pushNotification({ text: "one" });

    host.getNotificationLog()[0]!.cancelled = true;

    expect(host.getNotificationLog()[0]!.cancelled).toBe(false);
  });
});

describe("statement injection through the chain connection", () => {
  // A minimal socket the proxy can drive, so the test exercises the real
  // subscribe-reply parsing rather than a stand-in for it.
  class FakeSocket {
    static instances: FakeSocket[] = [];
    listeners: Record<string, ((e: unknown) => void)[]> = {};
    sent: string[] = [];
    constructor(public url: string) {
      FakeSocket.instances.push(this);
      queueMicrotask(() => this.emit("open", {}));
    }
    addEventListener(type: string, fn: (e: unknown) => void) {
      (this.listeners[type] ??= []).push(fn);
    }
    emit(type: string, event: unknown) {
      for (const fn of this.listeners[type] ?? []) fn(event);
    }
    send(data: string) {
      this.sent.push(data);
    }
    close() {
      this.emit("close", {});
    }
  }

  async function connected() {
    const original = globalThis.WebSocket;
    FakeSocket.instances = [];
    (globalThis as { WebSocket: unknown }).WebSocket = FakeSocket;
    const host = createMockHost({
      chainProxies: [{ rpcUrl: "ws://chain.test" }],
    });
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    (globalThis as { WebSocket: unknown }).WebSocket = original;
    return { host, conn, socket: FakeSocket.instances[0]! };
  }

  it("delivers an injected statement to a live subscription", async () => {
    const { host, conn, socket } = await connected();
    const reader = conn.responses()[Symbol.asyncIterator]();

    conn.send(
      JSON.stringify({
        jsonrpc: "2.0",
        id: "truapi:1",
        method: "statement_subscribeStatement",
        params: [{ matchAll: [] }],
      }),
    );
    // The chain answers with the subscription id; that reply is what makes
    // injection addressable.
    socket.emit("message", {
      data: JSON.stringify({ jsonrpc: "2.0", id: "truapi:1", result: "sub-1" }),
    });
    await reader.next();

    // The entry the store now holds, which is what a suite asserts on; the
    // delivery itself is the envelope compared below.
    const injected = host.injectStatement(new Uint8Array([1, 2, 3]));
    expect(injected.fromProduct).toBe(false);
    expect(injected.timestamp).toBeGreaterThan(0);

    // Compared whole: the core reads `result.data.statements`, and asserting
    // the fields one by one would not catch an envelope carrying extra keys.
    expect(JSON.parse((await reader.next()).value as string)).toEqual({
      jsonrpc: "2.0",
      method: "statement_subscribeStatement",
      params: {
        subscription: "sub-1",
        result: {
          event: "newStatements",
          data: { statements: ["0x010203"], remaining: 0 },
        },
      },
    });
  });

  it("retains a statement injected before the product subscribes", async () => {
    const { host, conn } = await connected();
    const reader = conn.responses()[Symbol.asyncIterator]();
    // No subscribe reply seen yet, so there is no id to address and nothing can
    // be delivered. The statement is still retained, so a suite that injected
    // early has something to read back rather than a silent loss.
    host.injectStatement("0xab");
    expect(host.getStatements().map((entry) => entry.fromProduct)).toEqual([
      false,
    ]);

    // Nothing was addressed to the product: a delivery here would let a suite
    // think it received something.
    const raced = await Promise.race([
      reader.next().then(() => "delivered"),
      Promise.resolve("nothing"),
    ]);
    expect(raced).toBe("nothing");
  });

  it("records what was injected and clears it", async () => {
    const { host } = await connected();
    host.injectStatement(new Uint8Array([0xaa]));
    host.injectStatement("0xbb");
    expect(host.getInjectedStatements()).toEqual(["0xaa", "0xbb"]);
    host.clearStatements();
    expect(host.getInjectedStatements()).toEqual([]);
  });
});

describe("reading back submitted statements", () => {
  it("returns the hex the core put on the wire, in order", async () => {
    const original = globalThis.WebSocket;
    (globalThis as { WebSocket: unknown }).WebSocket = class {
      constructor(public url: string) {}
      addEventListener() {}
      send() {}
      close() {}
    };
    const host = createMockHost({
      chainProxies: [{ rpcUrl: "ws://chain.test" }],
    });
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    (globalThis as { WebSocket: unknown }).WebSocket = original;

    const submit = (hex: string, id: number) =>
      JSON.stringify({
        jsonrpc: "2.0",
        id: `truapi:${id}`,
        method: "statement_submit",
        params: [hex],
      });

    // Real statements, so the entries decode and the assertion below can tell
    // them apart by topic rather than only by how many there are.
    const first = encodeStatement({ topics: [`0x${"11".repeat(32)}`] });
    const second = encodeStatement({ topics: [`0x${"22".repeat(32)}`] });

    conn.send(submit(first, 1));
    // Other chain traffic must not be mistaken for a submission.
    conn.send(
      JSON.stringify({
        jsonrpc: "2.0",
        id: "truapi:2",
        method: "statement_subscribeStatement",
        params: [{ matchAll: [] }],
      }),
    );
    // An unsubscribe carries a STRING first param, so it is what a filter that
    // forgot to check the method would wrongly report as a submitted statement.
    conn.send(
      JSON.stringify({
        jsonrpc: "2.0",
        id: "truapi:3",
        method: "statement_unsubscribeStatement",
        params: ["z9VCGBlbLFl58Rp4"],
      }),
    );
    conn.send(submit(second, 4));

    // Two, not three: the unsubscribe carries a STRING first param and is what
    // a filter that forgot to check the method would report as a submission.
    expect(host.getSubmittedStatements().map((entry) => entry.topics)).toEqual([
      [`0x${"11".repeat(32)}`],
      [`0x${"22".repeat(32)}`],
    ]);
    expect(
      host.getSubmittedStatements().every((entry) => entry.fromProduct),
    ).toBe(true);
  });

  it("is empty when the product has submitted nothing", async () => {
    const host = createMockHost();
    expect(host.getSubmittedStatements()).toEqual([]);
  });

  it("drops what the chain path retained when the statements are cleared", async () => {
    // `getStatements` and `getSubmittedStatements` read the chain path's own
    // list whenever the loopback store is off. A clear that leaves it carries
    // one case's statements into the next, which reads as a product that
    // submitted something it never did.
    const original = globalThis.WebSocket;
    (globalThis as { WebSocket: unknown }).WebSocket = class {
      constructor(public url: string) {}
      addEventListener() {}
      send() {}
      close() {}
    };
    const host = createMockHost({
      chainProxies: [{ rpcUrl: "ws://chain.test" }],
    });
    const conn = await host.callbacks.chain.connect(new Uint8Array(32));
    (globalThis as { WebSocket: unknown }).WebSocket = original;

    conn.send(
      JSON.stringify({
        jsonrpc: "2.0",
        id: "truapi:1",
        method: "statement_submit",
        params: [encodeStatement({ topics: [`0x${"33".repeat(32)}`] })],
      }),
    );
    expect(host.getSubmittedStatements()).toHaveLength(1);

    host.clearStatements();
    expect(host.getSubmittedStatements()).toEqual([]);
    expect(host.getStatements()).toEqual([]);
  });
});
