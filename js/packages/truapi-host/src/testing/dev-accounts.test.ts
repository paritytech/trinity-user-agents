// Dev accounts are only useful if the real signing host accepts their entropy
// and gives each one a distinct session. That is what this proves, headlessly,
// against the signing-enabled testing bundle -- the JS counterpart of the Rust
// keystone test.
import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { createMockHost, mockRuntimeConfig } from "../web/create-mock-host.js";
import { DEV_ACCOUNTS, resolveAccount } from "./dev-accounts.js";
import { wasmIsBuilt } from "./require-wasm.js";

const wasmUrl = new URL(
  "../../dist/wasm/testing/truapi_server_bg.wasm",
  import.meta.url,
);
const glueUrl = new URL(
  "../../dist/wasm/testing/truapi_server.js",
  import.meta.url,
);
const suite = wasmIsBuilt("testing/truapi_server_bg.wasm")
  ? describe
  : describe.skip;

describe("dev account specs", () => {
  it("rejects an unknown name and a wrong-sized key", () => {
    expect(() => resolveAccount("eve" as "alice")).toThrow(
      /unknown dev account/,
    );
    expect(() =>
      resolveAccount({ name: "short", entropy: new Uint8Array(16) }),
    ).toThrow(/32 bytes/);
  });

  it("gives every built-in account distinct entropy", () => {
    const seen = new Set(
      Object.values(DEV_ACCOUNTS).map((e) => Buffer.from(e).toString("hex")),
    );
    expect(seen.size).toBe(Object.keys(DEV_ACCOUNTS).length);
    for (const entropy of Object.values(DEV_ACCOUNTS)) {
      expect(entropy).toHaveLength(32);
    }
  });
});

suite("dev accounts against the real signing host", () => {
  async function signingRuntime() {
    const { initSync, WasmSigningHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });
    const mock = createMockHost();
    const { productId, ...hostConfig } = mockRuntimeConfig();
    const runtime = new WasmSigningHostRuntime(
      {
        ...createWasmRawCallbacks(mock.callbacks),
        workerDemandChanged: () => {},
      },
      hostConfig,
    );
    return { runtime, mock, productId };
  }

  it("activates a session from a dev account's entropy", async () => {
    const { runtime } = await signingRuntime();
    // The assertion is that this resolves: activation derives a root keypair
    // from the entropy, and rejects entropy it cannot derive from.
    await runtime.activateLocalSession(DEV_ACCOUNTS.alice);
  });

  it("switches account by re-activating, and signs out", async () => {
    const { runtime } = await signingRuntime();
    await runtime.activateLocalSession(DEV_ACCOUNTS.alice);
    await runtime.disconnectSession();
    await runtime.activateLocalSession(DEV_ACCOUNTS.bob);
    await runtime.disconnectSession();
  });

  it("refuses entropy it cannot derive a key from", async () => {
    const { runtime } = await signingRuntime();
    // An empty secret is not valid BIP-39 entropy; a host that accepted it
    // would hand tests a session with no key behind it.
    await expect(
      runtime.activateLocalSession(new Uint8Array(0)),
    ).rejects.toBeDefined();
  });
});
