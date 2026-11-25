import {
  asProductId,
  createClient,
  createMessagePortProvider,
  createTransport,
} from "@parity/truapi";
import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

import {
  CoreStorageKey,
  type JsonRpcConnection,
} from "../generated/host-callbacks.js";
import { createMockHost, mockRuntimeConfig } from "./create-mock-host.js";
import type {
  BundledStack,
  ReservedValuesPolicy,
  StackCallbacks,
  StackFetchRequest,
  StackFetchResponse,
  StackStatus,
} from "../runtime.js";
import { bundledTestStack } from "../test-support.js";
import { wasmIsBuilt } from "../testing/require-wasm.js";

/**
 * {@link mockRuntimeConfig} with the bundled previewnet stack in place of its
 * test stack: the production bundle reads only a signed `stack`. The product
 * is one previewnet serves.
 */
function bundledRuntimeConfig() {
  const { testStack: _testStack, ...config } = mockRuntimeConfig();
  return {
    ...config,
    productId: asProductId("mock.testnet"),
    stack: bundledTestStack(),
  };
}

/**
 * A connection to a node that gives `genesisHash` as its chain's, as one whose
 * chain restarted from genesis does. It answers no other request.
 */
function chainWithGenesis(genesisHash: string): JsonRpcConnection {
  const answers: string[] = [];
  let closed = false;
  let wake = () => {};
  return {
    send(request) {
      const { id, method } = JSON.parse(request);
      if (method !== "chainSpec_v1_genesisHash") return;
      answers.push(JSON.stringify({ jsonrpc: "2.0", id, result: genesisHash }));
      wake();
    },
    async *responses() {
      while (!closed) {
        // Waits only with nothing to deliver, so no answer or close that
        // comes while one is being delivered is missed.
        if (answers.length === 0) {
          await new Promise<void>((resolve) => (wake = resolve));
        }
        yield* answers.splice(0);
      }
    },
    close() {
      closed = true;
      wake();
    },
  };
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
async function pairingHostStarter(): Promise<
  (stack: BundledStack) => Promise<unknown>
> {
  const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
  const { createWasmRawCallbacks } =
    await import("../generated/host-callbacks-adapter.js");
  initSync({ module: readFileSync(wasmUrl) });
  const { productId: _productId, ...hostConfig } = bundledRuntimeConfig();
  return (stack) =>
    WasmPairingHostRuntime.create(
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
    const runtime = await WasmPairingHostRuntime.create(raw, hostConfig);
    // The core emits response frames through `emitFrame`; the worker supplies
    // it per-core outside the generated adapter, so the harness does too.
    runtime.productRuntime({ productId }, { emitFrame: () => {} });
    // The real core reads its auth session on startup, which crosses the bridge
    // into the mock's readCoreStorage with a SCALE-decoded CoreStorageKey.
    await new Promise((resolve) => setTimeout(resolve, 200));

    // Building the runtime also reads the stack record, so the auth session
    // read is named: it is the one that proves the startup traffic crossed.
    expect(invoked).toContain("readCoreStorage:AuthSession");

    // The same traffic must be visible through the control surface, because
    // that count is what a harness waits on to decide the wire is live.
    expect(mock.getHostCallCount()).toBeGreaterThan(
      invoked.filter((call) => call === "readCoreStorage:StackRecord").length,
    );
  });

  it("refuses a stack pointer without a key", async () => {
    // A copy the host fetches later is checked against the pointer's key, so
    // the production bundle refuses a pointer without one.
    const start = await pairingHostStarter();
    const stack = bundledTestStack();
    const pointer = new URL(stack.pointer);
    pointer.searchParams.delete("key");

    await expect(start({ ...stack, pointer: pointer.href })).rejects.toThrow(
      "runtimeConfig.stack: stack pointer has no key",
    );
  });

  it("holds a stack to the reserved values its build adds", async () => {
    // A build that reserves previewnet's TLD or one of its genesis hashes does
    // not start previewnet, and one that allows its stack every reserved value
    // has none of its own to add.
    const start = await pairingHostStarter();
    const stack = bundledTestStack();
    const envelope = JSON.parse(new TextDecoder().decode(stack.descriptor));
    const genesisHash: string = JSON.parse(atob(envelope.payload)).chains[0]
      .genesisHash;
    const reserving = (reservedValues: ReservedValuesPolicy) =>
      start({ ...stack, reservedValues });

    await expect(reserving({ extraTlds: ["testnet"] })).rejects.toThrow(
      "runtimeConfig.stack: stack uses the reserved tld testnet",
    );
    await expect(
      reserving({ extraGenesisHashes: [genesisHash] }),
    ).rejects.toThrow(
      `runtimeConfig.stack: stack uses the reserved genesis hash ${genesisHash}`,
    );
    await expect(
      reserving({ allowed: true, extraTlds: ["testnet"] }),
    ).rejects.toThrow(
      "runtimeConfig.stack: reserved values that allow the stack every reserved value cannot reserve extra ones",
    );
    await expect(reserving({ allowed: true })).resolves.toBeDefined();
  });

  it("refuses a stack field it does not have", async () => {
    // A retired or misspelt field would be read as absent, and a host on an
    // older shape would meet a refusal that does not name its mistake.
    const start = await pairingHostStarter();
    const stack = { ...bundledTestStack(), allowsReservedValues: true };

    await expect(start(stack)).rejects.toThrow(
      "runtimeConfig.stack.allowsReservedValues is not a field; runtimeConfig.stack takes pointer, descriptor, reservedValues",
    );
  });

  it("refuses reserved values it cannot read in full", async () => {
    // A field or entry the core skipped would leave the build reserving less
    // than it wrote, with nothing to say so.
    const start = await pairingHostStarter();
    const stack = bundledTestStack();
    const path = "runtimeConfig.stack.reservedValues";
    const refusal = async (reservedValues: unknown) => {
      try {
        await start({ ...stack, reservedValues } as BundledStack);
        return "started";
      } catch (error) {
        return String(error);
      }
    };

    expect(
      await Promise.all(
        [
          ["testnet"],
          { extraTLDs: ["testnet"] },
          { allowed: "true" },
          { extraTlds: "testnet" },
          { extraTlds: ["paseo", null] },
          { extraTlds: [".testnet"] },
          { extraTlds: ["li"] },
          { extraGenesisHashes: ["0x1234"] },
          { extraGenesisHashes: [new Uint8Array(31)] },
        ].map(refusal),
      ),
    ).toEqual([
      `${path} must be an object`,
      `${path}.extraTLDs is not a field; ${path} takes allowed, extraGenesisHashes, extraTlds`,
      `${path}.allowed must be a boolean`,
      `${path}.extraTlds must be an array`,
      `${path}.extraTlds[1] must be a string`,
      'runtimeConfig.stack: reserved tld ".testnet" is not a TLD a stack can have',
      'runtimeConfig.stack: reserved tld "li" is not a TLD a stack can have',
      `${path}.extraGenesisHashes[0]: expected 32-byte hex string, got 4 hex chars`,
      `${path}.extraGenesisHashes[0] must be exactly 32 bytes, got 31`,
    ]);
  });

  it("refreshes the stack through the host's fetch and reports each change", async () => {
    // The published descriptor moves when a stack restarts its chains. The
    // core has no HTTP client, so a web host follows it only if the request,
    // the answer, the status and the record all cross the bridge intact.
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });
    const { productId: _productId, ...hostConfig } = bundledRuntimeConfig();
    const requests: StackFetchRequest[] = [];
    const statuses: StackStatus[] = [];
    const answers: StackFetchResponse[] = [
      { status: "document", bytes: hostConfig.stack.descriptor, etag: '"v1"' },
    ];
    let online = false;
    const stackCallbacks: StackCallbacks = {
      fetchStackDescriptor: async (request) => {
        requests.push(request);
        const answer = online ? answers.shift() : undefined;
        if (!answer) throw new Error("offline");
        return answer;
      },
      stackChanged: (status) => statuses.push(status),
    };
    const settled = () => new Promise((resolve) => setTimeout(resolve, 200));
    // One host, so the core storage the first runtime writes is what the
    // second one reads.
    const mock = createMockHost();
    const start = () =>
      WasmPairingHostRuntime.create(
        {
          ...createWasmRawCallbacks(mock.callbacks),
          requestWorkerTransition: () => {},
          ...stackCallbacks,
        },
        hostConfig,
      );

    const runtime = await start();
    await settled();
    online = true;
    runtime.refreshStack();
    await settled();

    const pointer = new URL(hostConfig.stack.pointer);
    const request = {
      url: pointer.searchParams.get("url"),
      maxBytes: 1024 * 1024,
    };
    expect(requests).toEqual([request, request]);
    expect(statuses.map((status) => status.lastRefresh)).toEqual([
      undefined,
      { outcome: "unreachable", reason: "offline" },
      { outcome: "current" },
    ]);
    expect(runtime.stackStatus()).toEqual(statuses[2]);
    const descriptor = JSON.parse(statuses[2].descriptor);
    expect([descriptor.id, statuses[2].generation]).toEqual([
      "previewnet",
      descriptor.generation,
    ]);

    // The record went to core storage. A core built from the bundled copy
    // alone would fetch at once; one that read the record knows its copy was
    // fetched moments ago and waits for the hour.
    const restarted = await start();
    await settled();
    expect([requests.length, restarted.stackStatus().descriptor]).toEqual([
      2,
      statuses[2].descriptor,
    ]);
  });

  it("tells a host without fetchStackDescriptor the status at start", async () => {
    // stackChanged is documented to report the status at start. A host that
    // does not fetch the descriptor still shows which stack it runs.
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });
    const { productId: _productId, ...hostConfig } = bundledRuntimeConfig();
    const statuses: StackStatus[] = [];

    const runtime = await WasmPairingHostRuntime.create(
      {
        ...createWasmRawCallbacks(createMockHost().callbacks),
        requestWorkerTransition: () => {},
        stackChanged: (status: StackStatus) => statuses.push(status),
      },
      hostConfig,
    );

    try {
      expect(statuses).toEqual([runtime.stackStatus()]);
      expect(statuses[0].lastRefresh).toBeUndefined();
      expect(statuses[0].mismatches).toEqual([]);
    } finally {
      runtime.free();
    }
  });

  it("tells a host without fetchStackDescriptor of a chain that restarted", async () => {
    // A stack can restart its chains while a page stays open. A host that
    // does not fetch the descriptor still has to keep its own features off
    // such a chain, so what the chain answered has to reach `stackChanged`
    // in the shape `StackMismatch` documents.
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });
    const { productId, ...hostConfig } = bundledRuntimeConfig();
    const statuses: StackStatus[] = [];
    let mismatchReported = () => {};
    const mismatchReport = new Promise<void>(
      (resolve) => (mismatchReported = resolve),
    );
    const mock = createMockHost();
    const restarted = `0x${"ab".repeat(32)}`;
    mock.callbacks.chain.connect = async () => chainWithGenesis(restarted);
    const runtime = await WasmPairingHostRuntime.create(
      {
        ...createWasmRawCallbacks(mock.callbacks),
        requestWorkerTransition: () => {},
        stackChanged: (status: StackStatus) => {
          statuses.push(status);
          if (status.mismatches.length > 0) mismatchReported();
        },
      },
      hostConfig,
    );
    const channel = new MessageChannel();
    const core = runtime.productRuntime(
      { productId },
      { emitFrame: (frame: Uint8Array) => channel.port1.postMessage(frame) },
    );
    channel.port1.onmessage = (event: MessageEvent) =>
      void core.receiveFrame(event.data);
    channel.port1.start();
    const provider = createMessagePortProvider(channel.port2);
    const client = createClient(createTransport(provider));
    const people = JSON.parse(statuses[0].descriptor).chains.find(
      (chain: { role: string }) => chain.role === "People",
    );

    try {
      const answer = await client.chain.getSpecGenesisHash({
        genesisHash: people.genesisHash,
      });
      // A task of the core's own sends the report, in no set order with the
      // answer.
      let giveUp: ReturnType<typeof setTimeout> | undefined;
      await Promise.race([
        mismatchReport,
        new Promise((_, reject) => {
          giveUp = setTimeout(
            () => reject(new Error("no status listed the mismatch")),
            2000,
          );
        }),
      ]);
      clearTimeout(giveUp);

      // The call fails as a host failure, and the mismatch names People:
      // together they show it was this chain's connection the core refused.
      expect([
        answer.isErr() && answer.error.tag,
        statuses.map((status) => status.mismatches),
      ]).toEqual([
        "HostFailure",
        [
          [],
          [
            {
              kind: "genesisHash",
              role: "People",
              named: people.genesisHash,
              answered: restarted,
            },
          ],
        ],
      ]);
    } finally {
      provider.dispose();
      core.dispose();
      runtime.free();
      channel.port1.close();
      channel.port2.close();
    }
  });

  it("tells a host of its stack only once the runtime exists", async () => {
    // A config the core refuses must not leave the host holding the status of
    // a runtime that was never built.
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });
    const {
      productId: _productId,
      pairing: _pairing,
      ...withoutPairing
    } = bundledRuntimeConfig();
    const statuses: StackStatus[] = [];

    await expect(
      WasmPairingHostRuntime.create(
        {
          ...createWasmRawCallbacks(createMockHost().callbacks),
          requestWorkerTransition: () => {},
          stackChanged: (status: StackStatus) => statuses.push(status),
        },
        withoutPairing,
      ),
    ).rejects.toThrow("runtimeConfig.pairing");
    expect(statuses).toEqual([]);
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
      const runtime = await WasmPairingHostRuntime.create(
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
