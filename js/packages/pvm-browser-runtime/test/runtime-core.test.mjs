import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";

await import("../src/pvm-wasm-translated.js");
await import("../src/pvm-runtime-core.js");

const packageRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(packageRoot, "../../..");

function bytesBuffer(bytes) {
  return bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  );
}

function endpoint() {
  const messages = [];
  const receiver = {
    onmessage: null,
    postMessage(message) {
      messages.push(message);
    },
  };
  globalThis.createPvmRuntime(receiver);
  return { messages, receiver };
}

async function settle() {
  await new Promise((resolve) => setImmediate(resolve));
}

async function waitForMessage(messages, type, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const message = messages.find((candidate) => candidate.type === type);
    if (message) {
      return message;
    }
    const error = messages.find((candidate) => candidate.type === "error");
    if (error) {
      throw new Error(error.message);
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`timed out waiting for browser runtime message ${type}`);
}

function invalidStart(overrides = {}) {
  return {
    type: "start",
    runtime: new Uint8Array([0]),
    program: new Uint8Array([1]),
    assets: [],
    graphicsProfile: "framebuffer",
    audioEnabled: false,
    cacheKey: "invalid",
    ...overrides,
  };
}

test("browser runtime rejects unbounded launch inputs before compilation", async () => {
  for (const [message, expected] of [
    [invalidStart({ program: new Uint8Array() }), /program must contain/],
    [
      invalidStart({
        assets: [
          { path: "same.bin", bytes: new Uint8Array() },
          { path: "same.bin", bytes: new Uint8Array() },
        ],
      }),
      /duplicated/,
    ],
    [
      invalidStart({
        assets: [{ path: "../escape", bytes: new Uint8Array() }],
      }),
      /invalid PolkaVM browser asset path/,
    ],
    [
      invalidStart({ graphicsProfile: "webgpu-raster" }),
      /WebGPU capabilities are required/,
    ],
  ]) {
    const { messages, receiver } = endpoint();
    receiver.onmessage({ data: message });
    await settle();
    assert.match(
      messages.find((candidate) => candidate.type === "error")?.message ?? "",
      expected,
    );
  }
});

test("compiler backend enforces the declared graphics profile", async () => {
  const runtime = await readFile(
    resolve(packageRoot, "dist/pvm-browser-runtime.wasm"),
  );
  const program = await readFile(
    resolve(
      repositoryRoot,
      "rust/crates/pvm-runtime/tests/fixtures/framebuffer-test.polkavm",
    ),
  );
  const { messages, receiver } = endpoint();
  receiver.onmessage({
    data: {
      type: "start",
      runtime: bytesBuffer(runtime),
      program: bytesBuffer(program),
      assets: [],
      graphicsProfile: "tri2d",
      audioEnabled: false,
      cacheKey: "profile-enforcement",
    },
  });
  const ready = await waitForMessage(messages, "ready");
  assert.equal(ready.backend, "compiler");
  await new Promise((resolve) => setTimeout(resolve, 100));
  assert.equal(
    messages.some((message) => message.type === "frame"),
    false,
    "a framebuffer submission must not escape a tri2d declaration",
  );
  receiver.onmessage({ data: { type: "stop" } });
  await waitForMessage(messages, "terminated");
});
