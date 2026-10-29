// A web host booted with `role: "signing"` talks to the same worker script as a
// pairing host, so every message the main thread can send reaches a signing
// runtime too. This drives the real main-thread runtime against the real worker
// script and the signing-enabled testing core, in one process.
import { describe, expect, it, mock } from "bun:test";
import { readFileSync } from "node:fs";

import { createClient, createTransport } from "@parity/truapi";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";

import { createMockHost, mockRuntimeConfig } from "./web/create-mock-host.js";
import { createWebWorkerPairingHostRuntime } from "./web/index.js";
import { DEV_ACCOUNTS } from "./testing/dev-accounts.js";
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

  /** The one person the host lists as a contact, and whom its picker answers with. */
  const CONTACT = `0x${"51".repeat(32)}` as HexString;
  const STRANGER = `0x${"53".repeat(32)}` as HexString;

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

    const { productId, ...hostConfig } = mockRuntimeConfig();
    const host = createMockHost();
    const runtime = await createWebWorkerPairingHostRuntime(
      worker as unknown as Worker,
      {
        ...host.callbacks,
        // The browser has blocked every device capability for this origin.
        permissionStatus: { devicePermissionStatus: async () => "Denied" },
        contacts: {
          contacts: async (lookup) => ({
            accounts: lookup.handles.map(() => undefined),
          }),
          pickContact: async () => ({
            tag: "Picked",
            value: { account: CONTACT },
          }),
        },
      },
      { hostConfig: hostConfig as never, role: "signing" },
    );
    await runtime.activateLocalSession(DEV_ACCOUNTS.alice);
    return { runtime, host, productId, worker };
  }

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
        roomId: "lobby",
        name: "Lobby",
        icon: "",
      });
      expect(created._unsafeUnwrap()).toEqual({ status: "New" });
      expect(host.getChatRooms()).toEqual([
        { roomId: "lobby", participatingAs: "RoomHost" },
      ]);
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

  it("mints the handles contacts.pick gives a product, per session", async () => {
    const { runtime, productId } = await signingHost();
    const provider = await runtime.createProvider({ productId });
    try {
      const client = createClient(createTransport(provider));
      const outcome = (await client.contacts.pick({}))._unsafeUnwrap().outcome;
      if (outcome.tag !== "Picked") throw new Error(`picked ${outcome.tag}`);
      const asAlice = await runtime.contactHandles([
        hexToBytes(STRANGER),
        hexToBytes(CONTACT),
      ]);
      expect(asAlice?.map(bytesToHex)[1]).toBe(outcome.value.handle.bytes);
      expect(bytesToHex(asAlice![0])).not.toBe(outcome.value.handle.bytes);

      // Another session keys another user's handles: an answer minted under
      // one must never be taken for the next.
      await runtime.activateLocalSession(DEV_ACCOUNTS.bob);
      const asBob = await runtime.contactHandles([hexToBytes(CONTACT)]);
      expect(bytesToHex(asBob![0])).not.toBe(outcome.value.handle.bytes);
    } finally {
      provider.dispose();
      await runtime.activateLocalSession(DEV_ACCOUNTS.alice);
    }
  });

  it("refuses packed accounts that are not whole in the core itself", async () => {
    const { worker } = await signingHost();
    const answer = new Promise<unknown>((resolve) =>
      worker.addEventListener("message", (event) => {
        const data = (event as MessageEvent).data as {
          kind: string;
          requestId?: number;
        };
        if (data.kind === "contactHandlesResponse" && data.requestId === -1)
          resolve(data);
      }),
    );
    globalThis.dispatchEvent(
      new MessageEvent("message", {
        data: {
          kind: "contactHandles",
          requestId: -1,
          accounts: new Uint8Array(33),
        },
      }),
    );
    expect(await answer).toEqual({
      kind: "contactHandlesResponse",
      requestId: -1,
      ok: false,
      error: "contact accounts are 32 bytes each, got 33 bytes",
    });
  });
});
