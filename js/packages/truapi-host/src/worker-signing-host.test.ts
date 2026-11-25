// A web host booted with `role: "signing"` talks to the same worker script as a
// pairing host, so every message the main thread can send reaches a signing
// runtime too. This drives the real main-thread runtime against the real worker
// script and the signing-enabled testing core, in one process.
import { describe, expect, it, mock } from "bun:test";
import { readFileSync } from "node:fs";

import { createClient, createTransport } from "@parity/truapi";

import { createMockHost, mockRuntimeConfig } from "./web/create-mock-host.js";
import { createWebWorkerPairingHostRuntime } from "./web/index.js";
import { DEV_ACCOUNTS } from "./testing/dev-accounts.js";
import { bundledTestStack } from "./test-support.js";
import { wasmIsBuilt } from "./testing/require-wasm.js";

const wasmUrl = new URL(
  "../dist/wasm/testing/truapi_server_bg.wasm",
  import.meta.url,
);
const glueUrl = new URL(
  "../dist/wasm/testing/truapi_server.js",
  import.meta.url,
);
const suite = wasmIsBuilt("testing/truapi_server_bg.wasm")
  ? describe
  : describe.skip;

/** The worker side of a `Worker`, with the worker script running on this thread. */
class InProcessWorker extends EventTarget {
  postMessage(data: unknown): void {
    queueMicrotask(() =>
      globalThis.dispatchEvent(new MessageEvent("message", { data })),
    );
  }
  terminate(): void {}
}

suite("worker serving a signing host", () => {
  // The worker script keeps one runtime per global scope, as a real worker
  // does, so the suite boots it once.
  let booted: ReturnType<typeof boot> | undefined;
  const signingHost = () => (booted ??= boot());

  async function boot() {
    const glue = await import(glueUrl.href);
    glue.initSync({ module: readFileSync(wasmUrl) });
    mock.module("./wasm/web/truapi_server.js", () => ({
      ...glue,
      default: async () => {},
    }));
    const worker = new InProcessWorker();
    globalThis.postMessage = (data: unknown) =>
      queueMicrotask(() =>
        worker.dispatchEvent(new MessageEvent("message", { data })),
      );
    await import("./worker-runtime.js");

    // A bundled stack rather than `testStack`, so building the runtime reads
    // the stack record through the main thread before the worker is ready.
    const { productId, testStack: _testStack, ...config } = mockRuntimeConfig();
    const hostConfig = { ...config, stack: bundledTestStack() };
    const host = createMockHost();
    const coreStorageReads: string[] = [];
    const { coreStorage } = host.callbacks;
    const runtime = await createWebWorkerPairingHostRuntime(
      worker as unknown as Worker,
      {
        ...host.callbacks,
        coreStorage: {
          ...coreStorage,
          readCoreStorage: (key) => {
            coreStorageReads.push(key.tag);
            return coreStorage.readCoreStorage(key);
          },
        },
        // The browser has blocked every device capability for this origin.
        permissionStatus: { devicePermissionStatus: async () => "Denied" },
      },
      { hostConfig: hostConfig as never, role: "signing" },
    );
    const readsBeforeReady = [...coreStorageReads];
    await runtime.activateLocalSession(DEV_ACCOUNTS.alice);
    return { runtime, host, productId, readsBeforeReady };
  }

  it("answers the stack record read before the worker is ready", async () => {
    // The core reads its stored stack while it is built, so the main thread
    // has to serve host calls before `ready`, or `init` waits out its timeout.
    const { readsBeforeReady } = await signingHost();
    expect(readsBeforeReady).toContain("StackRecord");
  });

  it("reads stored permissions through the browser's device gate", async () => {
    // A host mounting a product reads these first to build the iframe `allow`
    // attribute, so a camera the browser blocks must not read as usable.
    const { runtime, productId } = await signingHost();
    const identity = { tag: "IdentityDisclosure" } as const;
    const camera = { tag: "Device", value: "Camera" } as const;
    await runtime.setPermissionAuthorizationStatus(
      productId,
      identity,
      "Authorized",
    );
    await runtime.setPermissionAuthorizationStatus(
      productId,
      camera,
      "Authorized",
    );
    expect(
      await runtime.getPermissionAuthorizationStatuses(productId, [
        identity,
        camera,
      ]),
    ).toEqual(["Authorized", "Denied"]);
  });

  it("hands a Worker product's chat calls to the host", async () => {
    const { runtime, host, productId } = await signingHost();
    const provider = await runtime.createProvider({
      productId,
      executionKind: "Worker",
    });
    try {
      const client = createClient(createTransport(provider));
      const created = await client.chat.createRoom({
        roomIdSource: "lobby",
        name: "Lobby",
        icon: "",
        config: {
          productSource: false,
          reactionDetails: false,
        },
      });
      const { status, roomId } = created._unsafeUnwrap();
      expect(status).toBe("New");
      // The core makes the id, and the host stores the room under it.
      expect(roomId).toMatch(/^0x[0-9a-f]{64}$/);
      expect(host.getChatRooms()).toEqual([
        { roomId, participatingAs: "RoomHost" },
      ]);
    } finally {
      provider.dispose();
    }
  });

  it("stores a feed update only in a room the host keeps", async () => {
    // The core reads the configuration of the room and tells the host, across
    // the wasm bridge, whether to store each update: a room whose messages the
    // product keeps only shows it.
    const { runtime, host, productId } = await signingHost();
    const provider = await runtime.createProvider({
      productId,
      executionKind: "Worker",
    });
    try {
      const client = createClient(createTransport(provider));
      const update = { tag: "Deleted", value: { messageId: "m1" } } as const;
      for (const productSource of [false, true]) {
        const created = await client.chat.createRoom({
          roomIdSource: `feed-${productSource}`,
          name: "Feed",
          icon: "",
          config: { productSource, reactionDetails: false },
        });
        const { roomId } = created._unsafeUnwrap();
        (await client.chat.updateFeed({ roomId, update }))._unsafeUnwrap();
        expect(host.chatRoomDeclarations(roomId)?.feed).toEqual([
          { update, persist: !productSource },
        ]);

        (await client.chat.deleteRoom({ roomId }))._unsafeUnwrap();
        expect(host.chatRoomDeclarations(roomId)).toBeUndefined();
        // The host answers no room for the deleted one, across the wasm bridge:
        // a second delete succeeds, and a feed update names an unknown room.
        (await client.chat.deleteRoom({ roomId }))._unsafeUnwrap();
        expect(
          (await client.chat.updateFeed({ roomId, update }))._unsafeUnwrapErr(),
        ).toEqual({
          tag: "Domain",
          value: { tag: "V2", value: "UnknownRoom" },
        });
      }
    } finally {
      provider.dispose();
    }
  });

  it("drops a preview that shows the user when the host takes the messages over", async () => {
    // A product that keeps the messages may show the user in the preview. Once
    // the host stores the messages, it shows the user from its own store, so
    // the preview must not stay, and the product cannot set it again.
    const { runtime, host, productId } = await signingHost();
    const provider = await runtime.createProvider({
      productId,
      executionKind: "Worker",
    });
    try {
      const client = createClient(createTransport(provider));
      const create = (productSource: boolean) =>
        client.chat.createRoom({
          roomIdSource: "preview",
          name: "Preview",
          icon: "",
          config: { productSource, reactionDetails: false },
        });
      const preview = {
        timestamp: undefined,
        badge: undefined,
        lastMessage: {
          messageId: "m1",
          origin: { participantId: "user" },
          timestamp: 1_700_000_000_000n,
          body: { tag: "Text", value: { text: "On my way" } },
          status: "Sent",
          replyTo: undefined,
          editedAt: undefined,
          request: false,
        },
        lastReaction: undefined,
        unseenReactions: 0,
        pinned: false,
        activity: undefined,
      } as const;
      const { roomId } = (await create(true))._unsafeUnwrap();
      (await client.chat.setRoomPreview({ roomId, preview }))._unsafeUnwrap();

      (await create(false))._unsafeUnwrap();
      expect(host.chatRoomDeclarations(roomId)?.preview).toBeUndefined();
      expect(
        (
          await client.chat.setRoomPreview({ roomId, preview })
        )._unsafeUnwrapErr(),
      ).toEqual({ tag: "Domain", value: { tag: "V2", value: "NotOwner" } });
    } finally {
      provider.dispose();
    }
  });

  it("refuses a pairing-only request by name", async () => {
    const { runtime } = await signingHost();
    await expect(runtime.getSessionChatIdentityKey()).rejects.toThrow(
      "getSessionChatIdentityKey needs a pairing host; this runtime is a signing host",
    );
  });
});
