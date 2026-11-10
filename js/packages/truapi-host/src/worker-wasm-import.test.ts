import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// The worker names the wasm glue in a literal import so bundlers resolve it
// statically and emit it alongside `truapi_server_bg.wasm`. Hidden behind a
// variable or `@vite-ignore` the import leaves the module graph, every bundler
// silently omits both files, and the failure only surfaces when a host's worker
// tries to instantiate the core. These assertions run against the compiled
// output because that is what a consuming bundler reads.

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const workerEntry = join(packageRoot, "dist/worker-runtime.js");
const GLUE_SPECIFIER = "./wasm/web/truapi_server.js";
const gluePath = join(packageRoot, "dist", GLUE_SPECIFIER);

// Every binding the worker reaches for through `WasmModuleShape`, and that
// `src/wasm/web/truapi_server.d.ts` declares. `default` is the wasm-pack init.
const GLUE_EXPORTS = [
  "default",
  "WasmPairingHostRuntime",
  "WasmProductRuntime",
  "setLogLevel",
] as const;

describe("worker wasm import", () => {
  it("names the glue in a statically analysable import", () => {
    expect(
      existsSync(workerEntry),
      `${workerEntry} is missing; run \`npm run build\` first`,
    ).toBe(true);
    const compiled = readFileSync(workerEntry, "utf8");

    expect(compiled).toContain(`import("${GLUE_SPECIFIER}")`);
    expect(compiled).not.toContain("@vite-ignore");
  });

  // `make wasm` is a local step, so CI reaches here without the artifact. Skip
  // visibly rather than returning early: a silent `return` reports as a pass
  // and hides that the assertions never ran.
  it.skipIf(!existsSync(gluePath))(
    "resolves that specifier against a glue exporting the surface the worker uses",
    async () => {
      expect(readFileSync(gluePath, "utf8")).toContain("truapi_server_bg.wasm");

      // The ambient declaration in `src/wasm/web/truapi_server.d.ts` is
      // hand-written, so nothing else checks it against what `make wasm`
      // actually emits. Importing the real glue catches a rename or removal
      // that would otherwise only fail inside a host's worker at runtime.
      const glue: Record<string, unknown> = await import(
        pathToFileURL(gluePath).href
      );

      for (const name of GLUE_EXPORTS) {
        expect(typeof glue[name], `glue must export \`${name}\``).toBe(
          "function",
        );
      }
    },
  );
});
