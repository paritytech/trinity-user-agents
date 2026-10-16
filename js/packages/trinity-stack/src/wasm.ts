// Loads the trinity-stack crate's wasm build once, at import, so the rest of the package can call
// it synchronously. Node and Bun read the file; browsers fetch it next to this module.
import init, { initSync } from "../wasm/trinity_stack.js";

const wasmUrl = new URL("../wasm/trinity_stack_bg.wasm", import.meta.url);

if (typeof process !== "undefined" && process.versions?.node) {
  const { readFile } = await import(/* @vite-ignore */ "node:fs/promises");
  initSync({ module: await readFile(wasmUrl) });
} else {
  await init({ module_or_path: wasmUrl });
}

export * as wasm from "../wasm/trinity_stack.js";
