// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Boots the Rust core under plain node: no Worker, no DOM, no fetch.
//
// `@parity/truapi-host` ships a `wasm-bindgen --target web` build, but the
// browser coupling lives in the wrappers, not the core. `initSync({module})`
// instantiates the wasm from a plain Buffer, and the generated typed-to-raw
// callback adapter (`createWasmRawCallbacks`) references nothing beyond plain
// JS. The adapter is NOT in the package's exports map, so it is reached by
// file URL relative to the one wasm path that IS exported. This is an
// upstream packaging ask: a `./node` export would delete this file's second
// half.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { RequiredHostCallbacks } from "@parity/truapi-host";

type WasmBindings = typeof import("@parity/truapi-host/wasm/web");

export interface WasmCore {
  bindings: WasmBindings;
  /** The package's own generated typed-to-raw adapter, reached by file URL. */
  createRawCallbacks: (callbacks: RequiredHostCallbacks) => unknown;
}

/**
 * Locate `@parity/truapi-host`'s install directory. `import.meta.resolve` is
 * the honest way. The node_modules walk covers runtimes that transform
 * modules and lose it (vitest's vite-node, some bundlers).
 */
function findHostPackageDir(): string {
  try {
    const glue = fileURLToPath(
      import.meta.resolve("@parity/truapi-host/wasm/web"),
    );
    // dist/wasm/web/truapi_server.js -> package root
    return dirname(dirname(dirname(dirname(glue))));
  } catch {
    let dir = dirname(fileURLToPath(import.meta.url));
    for (;;) {
      const candidate = join(dir, "node_modules", "@parity", "truapi-host");
      if (existsSync(join(candidate, "package.json"))) {
        return candidate;
      }
      const parent = dirname(dir);
      if (parent === dir) {
        throw new Error(
          "Could not locate @parity/truapi-host. Pass `wasmDir` (its dist/ directory) to createCliHost.",
        );
      }
      dir = parent;
    }
  }
}

// Keyed by the resolved dist directory, not held as one process-wide
// singleton: a caller passing a different `wasmDir` gets its own core
// instance (module identity follows the file URL), instead of silently
// receiving whichever core loaded first.
const loadedByDistDir = new Map<string, Promise<WasmCore>>();

async function loadFrom(distDir: string): Promise<WasmCore> {
  const glueUrl = pathToFileURL(
    join(distDir, "wasm", "web", "truapi_server.js"),
  ).href;
  const bindings = (await import(glueUrl)) as WasmBindings;
  bindings.initSync({
    module: readFileSync(join(distDir, "wasm", "web", "truapi_server_bg.wasm")),
  });
  const adapterUrl = pathToFileURL(
    join(distDir, "generated", "host-callbacks-adapter.js"),
  ).href;
  const adapter = (await import(adapterUrl)) as {
    createWasmRawCallbacks: (callbacks: RequiredHostCallbacks) => unknown;
  };
  return { bindings, createRawCallbacks: adapter.createWasmRawCallbacks };
}

/**
 * Load and instantiate the wasm core, once per dist directory.
 *
 * The wasm holds module-level state, so everything sharing a dist directory
 * must go through ONE instance. The glue is always imported by file URL to
 * keep module identity stable (a bare-specifier import elsewhere would
 * materialize a second instance).
 */
export function loadWasmCore(
  options: { wasmDir?: string } = {},
): Promise<WasmCore> {
  const distDir = options.wasmDir ?? join(findHostPackageDir(), "dist");
  let pending = loadedByDistDir.get(distDir);
  if (pending === undefined) {
    pending = loadFrom(distDir);
    loadedByDistDir.set(distDir, pending);
    // A failed load (wrong wasmDir) must not cache its rejection forever.
    pending.catch(() => {
      loadedByDistDir.delete(distDir);
    });
  }
  return pending;
}
