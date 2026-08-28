import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const dist = resolve(root, "dist");

test("browser runtime artifacts match their published checksums", async () => {
  const expected = new Map(
    (await readFile(resolve(dist, "SHA256SUMS"), "utf8"))
      .trim()
      .split("\n")
      .map((line) => {
        const [digest, file] = line.split(/\s+/, 2);
        return [file, digest];
      }),
  );
  for (const file of [
    "pvm-browser-runtime.wasm",
    "pvm-worker.js",
    "pvm-gpu-worker.js",
    "pvm-wasm-translated.js",
    "pvm-runtime-core.js",
    "pvm-wasm-worker-entry.js",
  ]) {
    const bytes = await readFile(resolve(dist, file));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), expected.get(file));
  }
});

test("Wasm runtime exports the neutral graphics profile ABI", async () => {
  const bytes = await readFile(resolve(dist, "pvm-browser-runtime.wasm"));
  const module = await WebAssembly.compile(bytes);
  const exports = new Set(WebAssembly.Module.exports(module).map(({ name }) => name));
  for (const name of [
    "pvm_browser_launch_begin_v2",
    "pvm_browser_take_tri2d",
    "pvm_browser_set_gpu_capabilities",
    "pvm_browser_send_gpu_event",
    "pvm_browser_take_gpu_batch",
  ]) {
    assert.ok(exports.has(name), `missing ${name}`);
  }
});
