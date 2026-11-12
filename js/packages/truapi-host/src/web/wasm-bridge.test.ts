import { asProductId } from "@parity/truapi";
import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

import { CoreStorageKey } from "../generated/host-callbacks.js";
import { createMockHost, mockRuntimeConfig } from "./create-mock-host.js";
import type { BundledStack, ReservedValuesPolicy } from "../runtime.js";
import { bundledTestStack } from "../test-support.js";
import { wasmIsBuilt } from "../testing/require-wasm.js";

/**
 * {@link mockRuntimeConfig} with the bundled previewnet stack in place of its
 * test stack: the production bundle reads only a signed `stack`.
 */
function bundledRuntimeConfig() {
  const { testStack: _testStack, ...config } = mockRuntimeConfig();
  return { ...config, stack: bundledTestStack() };
}

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
/** Starts a pairing host on a stack, as the production bundle does. */
async function pairingHostStarter(): Promise<(stack: BundledStack) => unknown> {
  const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
  const { createWasmRawCallbacks } =
    await import("../generated/host-callbacks-adapter.js");
  initSync({ module: readFileSync(wasmUrl) });
  const { productId: _productId, ...hostConfig } = bundledRuntimeConfig();
  return (stack) =>
    new WasmPairingHostRuntime(
      {
        ...createWasmRawCallbacks(createMockHost().callbacks),
        requestWorkerTransition: () => {},
      },
      { ...hostConfig, stack },
    );
}

// The gate lives in `require-wasm.ts` so rewriting any one suite cannot
// quietly disable it for the others.
const suite = wasmIsBuilt("web/truapi_server_bg.wasm")
  ? describe
  : describe.skip;

suite("real WASM core ↔ createMockHost bridge", () => {
  it("constructs normalized product IDs through the existing Rust core", async () => {
    const { initSync, parseProductId } = await import(glueUrl.href);
    initSync({ module: readFileSync(wasmUrl) });
    expect([
      parseProductId(" PEOPL.DOT "),
      parseProductId("localhost:3000"),
      parseProductId("worker.truapi-playground.paseo"),
    ]).toEqual([
      "peopl.dot",
      "localhost:3000",
      "worker.truapi-playground.paseo",
    ]);
    expect(() => parseProductId("not a product")).toThrow();
  });

  it("distinguishes product IDs from shared labels in persisted storage keys", async () => {
    const { initSync, describeCoreStorageKey } = await import(glueUrl.href);
    initSync({ module: readFileSync(wasmUrl) });
    const keys: CoreStorageKey[] = [
      { tag: "ProductManifest", value: { productLabel: "peopl" } },
      { tag: "AutoSigningKey", value: { productId: asProductId("peopl.dot") } },
      {
        tag: "PermissionAuthorization",
        value: {
          scope: "caller",
          request: {
            tag: "CrossProductSignature",
            value: { target: "target", kind: "Raw" },
          },
        },
      },
    ];
    expect(
      keys.map((key) => describeCoreStorageKey(CoreStorageKey.enc(key))),
    ).toEqual([
      { kind: "ProductManifest", productLabel: "peopl" },
      { kind: "AutoSigningKey", productId: "peopl.dot" },
      { kind: "PermissionAuthorization", productLabel: "caller" },
    ]);
  });

  it("the core invokes createMockHost callbacks across the JS↔SCALE↔WASM boundary", async () => {
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });

    const mock = createMockHost();
    const invoked: string[] = [];
    const coreStorage = mock.callbacks.coreStorage;
    const readCoreStorage = coreStorage.readCoreStorage.bind(coreStorage);
    coreStorage.readCoreStorage = async (key) => {
      invoked.push(`readCoreStorage:${key.tag}`);
      return readCoreStorage(key);
    };

    // The pairing-host runtime takes the platform callbacks and host config;
    // the per-product core is derived from it with its own frame sink.
    // `requestWorkerTransition` is a raw bridge callback rather than a generated
    // host callback, so the worker runtime supplies it outside the adapter and
    // a harness has to do the same.
    const raw = {
      ...createWasmRawCallbacks(mock.callbacks),
      requestWorkerTransition: () => {},
    };
    const { productId, ...hostConfig } = bundledRuntimeConfig();
    const runtime = new WasmPairingHostRuntime(raw, hostConfig);
    // The core emits response frames through `emitFrame`; the worker supplies
    // it per-core outside the generated adapter, so the harness does too.
    runtime.productRuntime({ productId }, { emitFrame: () => {} });
    // The real core reads its auth session on startup, which crosses the bridge
    // into the mock's readCoreStorage with a SCALE-decoded CoreStorageKey.
    await new Promise((resolve) => setTimeout(resolve, 200));

    expect(invoked.some((c) => c.startsWith("readCoreStorage:"))).toBe(true);

    // The same traffic must be visible through the control surface, because
    // that count is what a harness waits on to decide the wire is live.
    expect(mock.getHostCallCount()).toBeGreaterThan(0);
  });

  it("refuses a stack pointer without a key", async () => {
    // A copy the host fetches later is checked against the pointer's key, so
    // the production bundle refuses a pointer without one.
    const start = await pairingHostStarter();
    const stack = bundledTestStack();
    const pointer = new URL(stack.pointer);
    pointer.searchParams.delete("key");

    expect(() => start({ ...stack, pointer: pointer.href })).toThrow(
      "runtimeConfig.stack: stack pointer has no key",
    );
  });

  it("holds a stack to the reserved values its build adds", async () => {
    // A build that reserves previewnet's TLD or one of its genesis hashes
    // starts previewnet only when it allows the stack reserved values.
    const start = await pairingHostStarter();
    const stack = bundledTestStack();
    const envelope = JSON.parse(new TextDecoder().decode(stack.descriptor));
    const genesisHash: string = JSON.parse(atob(envelope.payload)).chains[0]
      .genesisHash;
    const reserving = (reservedValues: ReservedValuesPolicy) => () =>
      start({ ...stack, reservedValues });

    expect(reserving({ extraTlds: ["testnet"] })).toThrow(
      "runtimeConfig.stack: stack uses the reserved tld testnet",
    );
    expect(reserving({ extraGenesisHashes: [genesisHash] })).toThrow(
      `runtimeConfig.stack: stack uses the reserved genesis hash ${genesisHash}`,
    );
    expect(
      reserving({
        allowed: true,
        extraTlds: ["testnet"],
        extraGenesisHashes: [genesisHash],
      }),
    ).not.toThrow();
  });

  it("refuses reserved values it cannot read in full", async () => {
    // A field or entry the core skipped would leave the build reserving less
    // than it wrote, with nothing to say so.
    const start = await pairingHostStarter();
    const stack = bundledTestStack();
    const path = "runtimeConfig.stack.reservedValues";
    const refusal = (reservedValues: unknown) => {
      try {
        start({ ...stack, reservedValues } as BundledStack);
        return "started";
      } catch (error) {
        return String(error);
      }
    };

    expect(
      [
        ["testnet"],
        { extraTLDs: ["testnet"] },
        { allowed: "true" },
        { extraTlds: "testnet" },
        { extraTlds: ["paseo", null] },
        { extraTlds: [".testnet"] },
        { extraGenesisHashes: ["0x1234"] },
        { extraGenesisHashes: [new Uint8Array(31)] },
      ].map(refusal),
    ).toEqual([
      `${path} must be an object`,
      `${path}.extraTLDs is not a field; ${path} takes allowed, extraGenesisHashes, extraTlds`,
      `${path}.allowed must be a boolean`,
      `${path}.extraTlds must be an array`,
      `${path}.extraTlds[1] must be a string`,
      'runtimeConfig.stack: reserved tld ".testnet" must be one lowercase label of a-z, 0-9 and -, without the leading dot',
      `${path}.extraGenesisHashes[0]: expected 32-byte hex string, got 4 hex chars`,
      `${path}.extraGenesisHashes[0] must be exactly 32 bytes, got 31`,
    ]);
  });

  it("isolates one real-core boot from the next through reset()", async () => {
    // The surface-agreement test proves the Rust and JS mocks expose the same
    // control methods. It cannot prove those methods hold against a real core.
    // Test isolation is the property a product suite actually depends on: a
    // recording made while one core ran must not leak into the next case.
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });

    const mock = createMockHost();
    const { productId, ...hostConfig } = bundledRuntimeConfig();
    const boot = async () => {
      const runtime = new WasmPairingHostRuntime(
        {
          ...createWasmRawCallbacks(mock.callbacks),
          requestWorkerTransition: () => {},
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
