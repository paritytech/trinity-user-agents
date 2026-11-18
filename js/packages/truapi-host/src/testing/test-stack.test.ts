// A test host builds its stack from parts, on its own path through the core's
// config reader. This holds that path to the host's reserved values, against
// the testing bundle, the only one that reads `testStack`.
import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

import {
  createMockHost,
  mockRuntimeConfig,
  mockTestStack,
  type TestStack,
} from "../web/create-mock-host.js";
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

suite("test stack against the real core", () => {
  it("holds a test stack to the host's reserved values", async () => {
    const { initSync, WasmPairingHostRuntime } = await import(glueUrl.href);
    const { createWasmRawCallbacks } =
      await import("../generated/host-callbacks-adapter.js");
    initSync({ module: readFileSync(wasmUrl) });
    const { productId: _productId, ...hostConfig } = mockRuntimeConfig();
    const starting = (testStack: TestStack) =>
      WasmPairingHostRuntime.create(
        {
          ...createWasmRawCallbacks(createMockHost().callbacks),
          requestWorkerTransition: () => {},
        },
        { ...hostConfig, testStack },
      );
    const onDot = { ...mockTestStack(), tld: "dot" };

    await expect(starting(onDot)).rejects.toThrow(
      "runtimeConfig.testStack: stack uses the reserved tld dot",
    );
    await expect(
      starting({ ...onDot, reservedValues: { allowed: true } }),
    ).resolves.toBeDefined();
    await expect(
      starting({
        ...mockTestStack(),
        reservedValues: { extraTlds: ["paseo"] },
      }),
    ).rejects.toThrow(
      "runtimeConfig.testStack: stack uses the reserved tld paseo",
    );
    await expect(
      starting({ ...onDot, allowsReservedValues: true } as TestStack),
    ).rejects.toThrow(
      "runtimeConfig.testStack.allowsReservedValues is not a field; runtimeConfig.testStack takes network, tld, relay, assetHub, people, bulletin, reservedValues",
    );
  });
});
