// Proves the one-liner actually carries a call: product client -> real core ->
// mock host, over a real MessagePort, with the result observable on the mock.
import { describe, expect, it } from "bun:test";

import type { RemotePreimageLookupSubscribeItem } from "@parity/truapi";

import { createMockClient } from "./create-mock-client.js";
import { wasmIsBuilt } from "./require-wasm.js";

/** `0x`-prefixed lowercase hex, the shape the generated client takes. */
function hex(bytes: Uint8Array): `0x${string}` {
  const digits = Array.from(bytes, (byte) =>
    byte.toString(16).padStart(2, "0"),
  );
  return `0x${digits.join("")}`;
}

const suite = wasmIsBuilt("testing/truapi_server.js") ? describe : describe.skip;

suite("createMockClient", () => {
  it("round-trips a product call to the mock host", async () => {
    const { client, host, dispose } = await createMockClient();
    try {
      const result = await client.system.navigateTo({
        url: "https://polkadot.network/",
      });
      expect(result.isOk()).toBe(true);

      // The call reached the host seam, not just the client.
      expect(host.getNavigationLog()).toEqual(["https://polkadot.network/"]);
      expect(host.getHostCallCount()).toBeGreaterThan(0);
    } finally {
      dispose();
    }
  });

  it("round-trips product storage through the real dispatcher", async () => {
    const { client, host, dispose } = await createMockClient();
    try {
      const written = await client.localStorage.write({
        key: "greeting",
        value: "0x6869",
      });
      expect(written.isOk()).toBe(true);

      const read = await client.localStorage.read({ key: "greeting" });
      expect(read.isOk()).toBe(true);
      expect(read._unsafeUnwrap().value).toBe("0x6869");

      // And the host saw it, under the core's namespaced key.
      const stored = Object.entries(host.getProductStorage());
      expect(stored.length).toBeGreaterThan(0);
      expect(stored.some(([key]) => key.endsWith(":greeting"))).toBe(true);
    } finally {
      dispose();
    }
  });

  it("resolves a seeded preimage, so the seeded key is the one the core asks for", async () => {
    // `seedPreimage` is only worth anything if the key it hands back is the key
    // the core asks the host for. The core content-addresses preimages and
    // downgrades a hash mismatch to a miss, so a mock that keys its store any
    // other way round-trips perfectly against itself while every seeded
    // preimage stays unreachable from a product. Only a lookup driven through
    // the client, with the core in the path, can tell those apart.
    const { client, host, dispose } = await createMockClient();
    try {
      const content = new TextEncoder().encode("seeded through the core");
      const key = host.seedPreimage(content);

      const item = await new Promise<RemotePreimageLookupSubscribeItem>(
        (resolve, reject) => {
          const subscription = client.preimage
            .lookupSubscribe({ request: { key: hex(key) } })
            .subscribe({
              next(value) {
                subscription.unsubscribe();
                resolve(value);
              },
              error: reject,
            });
        },
      );

      expect(item.value).toBe(hex(content));
    } finally {
      dispose();
    }
  });

  it("answers a permission prompt from the mock's policy, once", async () => {
    const { client, host, dispose } = await createMockClient({
      mock: { devicePermissions: "deny-all" },
    });
    try {
      const denied = await client.permissions.requestDevicePermission("Camera");
      expect(denied._unsafeUnwrap().granted).toBe(false);
      expect(host.getPermissionLog()).toEqual([
        {
          tag: "Camera",
          value: "Camera",
          approved: false,
          kind: "device",
          decision: "Deny",
          timestamp: expect.any(Number),
        },
      ]);

      // The core caches a decided authorization, so a product asking twice is
      // answered from that record and the second call never reaches the host.
      const repeated = await client.permissions.requestDevicePermission("Camera");
      expect(repeated._unsafeUnwrap().granted).toBe(false);
      expect(
        host.getPermissionLog(),
        "a decided permission must not re-prompt the host",
      ).toHaveLength(1);

      // Setting the answer is the suite talking to the host, not the product
      // asking again, so it retracts that record and the next request is put to
      // the host afresh. Without this a suite can only ever observe the answer
      // its first request happened to settle on.
      host.grantPermission("Camera");
      const granted = await client.permissions.requestDevicePermission("Camera");
      expect(granted._unsafeUnwrap().granted).toBe(true);
      expect(host.getPermissionLog()).toHaveLength(2);
    } finally {
      dispose();
    }
  });
});
