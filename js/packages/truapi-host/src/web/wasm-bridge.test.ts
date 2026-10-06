import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

import { createMockHost, mockRuntimeConfig } from "./create-mock-host.js";
import { wasmIsBuilt } from "../testing/require-wasm.js";

// Drives the REAL truapi WASM core against createMockHost's callbacks —
// headless, no browser, no worker — to prove the JS↔SCALE↔WASM callback bridge.
// Requires the built WASM artifact (`npm run build:wasm`); skipped when it is
// absent so a plain `bun test` on a fresh checkout stays green.
//
// The `host-wasm` CI job builds both bundles and sets `REQUIRE_WASM=1`, so a
// missing artefact fails that run loudly rather than skipping green. `ts-host`
// does not build the WASM, so this suite skips there, which is why the two jobs
// are separate.
const wasmUrl = new URL(
  "../../dist/wasm/web/truapi_server_bg.wasm",
  import.meta.url,
);
const glueUrl = new URL(
  "../../dist/wasm/web/truapi_server.js",
  import.meta.url,
);
// The gate lives in `require-wasm.ts` so rewriting any one suite cannot
// quietly disable it for the others.
const suite = wasmIsBuilt("web/truapi_server_bg.wasm") ? describe : describe.skip;

suite("real WASM core ↔ createMockHost bridge", () => {
  it("the core invokes createMockHost callbacks across the JS↔SCALE↔WASM boundary", async () => {
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });

    const mock = createMockHost();
    const invoked: string[] = [];
    const secretCoreStorage = mock.callbacks.secretCoreStorage;
    const readSecretCoreStorage =
      secretCoreStorage.readSecretCoreStorage.bind(secretCoreStorage);
    secretCoreStorage.readSecretCoreStorage = async (key) => {
      invoked.push(`readSecretCoreStorage:${key.tag}`);
      return readSecretCoreStorage(key);
    };

    // The pairing-host runtime takes the platform callbacks and host config;
    // the per-product core is derived from it with its own frame sink.
    // `workerDemandChanged` is a raw bridge callback rather than a generated
    // host callback, so the worker runtime supplies it outside the adapter and
    // a harness has to do the same.
    const raw = {
      ...createWasmRawCallbacks(mock.callbacks),
      workerDemandChanged: () => {},
    };
    const { productId, ...hostConfig } = mockRuntimeConfig();
    const runtime = new WasmPairingHostRuntime(raw, hostConfig);
    // The core emits response frames through `emitFrame`; the worker supplies
    // it per-core outside the generated adapter, so the harness does too.
    runtime.productRuntime({ productId }, { emitFrame: () => {} });
    // The real core reads its auth session on startup, which crosses the bridge
    // into the mock's readSecretCoreStorage with a SCALE-decoded SecretCoreStorageKey.
    await new Promise((resolve) => setTimeout(resolve, 200));

    expect(invoked.some((c) => c.startsWith("readSecretCoreStorage:"))).toBe(
      true,
    );

    // The same traffic must be visible through the control surface, because
    // that count is what a harness waits on to decide the wire is live.
    expect(mock.getHostCallCount()).toBeGreaterThan(0);
  });

  it("isolates one real-core boot from the next through reset()", async () => {
    // The surface-agreement test proves the Rust and JS mocks expose the same
    // control methods. It cannot prove those methods hold against a real core.
    // Test isolation is the property a product suite actually depends on: a
    // recording made while one core ran must not leak into the next case.
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } = await import(
      "../generated/host-callbacks-adapter.js"
    );
    initSync({ module: readFileSync(wasmUrl) });

    const mock = createMockHost();
    const { productId, ...hostConfig } = mockRuntimeConfig();
    const boot = async () => {
      const runtime = new WasmPairingHostRuntime(
        {
          ...createWasmRawCallbacks(mock.callbacks),
          workerDemandChanged: () => {},
        },
        hostConfig,
      );
      runtime.productRuntime({ productId }, { emitFrame: () => {} });
      await new Promise((resolve) => setTimeout(resolve, 200));
    };

    // A real core boot leaves state behind in the mock's storage: it reads its
    // auth session, and anything a test seeded is still there.
    mock.seedPreimage(new Uint8Array([1, 2, 3]));
    await boot();
    expect(mock.getPreimages()).toHaveLength(1);

    mock.reset();

    // The same mock must now be as good as new for a second core.
    expect(mock.getPreimages()).toEqual([]);
    expect(mock.reviews()).toEqual([]);
    expect(mock.getPermissionLog()).toEqual([]);
    await boot();
    expect(
      mock.getPreimages(),
      "a second core boot must not resurrect what reset() cleared",
    ).toEqual([]);
  });
});
