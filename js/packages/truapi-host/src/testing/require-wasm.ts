// Shared gate for suites that need the built WASM bundles.
//
// Those suites skip when the artefact is absent, so a plain `bun test` on a
// fresh checkout stays green. That is also how they went unrun in CI for a long
// time, so `REQUIRE_WASM=1` turns absence into a loud failure instead, and the
// `host-wasm` CI job sets it.
//
// The check lives here rather than in one suite so that rewriting any single
// test cannot quietly disable the gate for the others.
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Absolute path of a file inside the built `dist/wasm` tree. */
export function wasmArtifact(relativePath: string): string {
  return fileURLToPath(
    new URL(`../../dist/wasm/${relativePath}`, import.meta.url),
  );
}

/**
 * Whether the WASM bundles are built.
 *
 * Throws instead of returning `false` when `REQUIRE_WASM=1`, so a suite that
 * would otherwise skip fails the run and names the fix.
 */
export function wasmIsBuilt(...relativePaths: string[]): boolean {
  const missing = relativePaths
    .map(wasmArtifact)
    .filter((path) => !existsSync(path));
  if (missing.length === 0) return true;
  if (process.env.REQUIRE_WASM === "1") {
    throw new Error(
      `REQUIRE_WASM=1 but WASM artefacts are missing:\n  ${missing.join(
        "\n  ",
      )}\nRun \`npm run build:wasm\` first.`,
    );
  }
  return false;
}
