// Loads the trinity-stack crate's wasm build once, at import, so the rest of the package can call
// it synchronously. Node and Bun read the file; browsers, and Node-enabled renderers loading it
// over http, fetch it next to this module. The file system is reached through
// `process.getBuiltinModule`, so a browser bundler sees no `node:` import to resolve.
import init, { initSync } from "../wasm/trinity_stack.js";

const wasmUrl = new URL("../wasm/trinity_stack_bg.wasm", import.meta.url);
const fs =
  typeof process === "undefined" || wasmUrl.protocol !== "file:"
    ? undefined
    : (process.getBuiltinModule?.("node:fs") as
        | typeof import("node:fs")
        | undefined);

if (fs) {
  initSync({ module: fs.readFileSync(wasmUrl) });
} else {
  await init({ module_or_path: wasmUrl });
}

export * as wasm from "../wasm/trinity_stack.js";
