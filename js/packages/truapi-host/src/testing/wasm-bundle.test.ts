// Precompressed WASM sidecars belong to host deployments, not SDK archives.
import { describe, expect, it } from "bun:test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { wasmIsBuilt } from "./require-wasm.js";

const packageRoot = dirname(
  fileURLToPath(new URL("../../package.json", import.meta.url)),
);
const testingGlue = join(packageRoot, "dist/wasm/testing/truapi_server.js");
const webGlue = join(packageRoot, "dist/wasm/web/truapi_server.js");
const suite = wasmIsBuilt(
  "testing/truapi_server_bg.wasm",
  "web/truapi_server_bg.wasm",
)
  ? describe
  : describe.skip;

/** Only a release `dist` carries the `.gz`/`.br` sidecars to exclude. */
const sidecarsBuilt = existsSync(
  join(packageRoot, "dist/wasm/web/truapi_server_bg.wasm.gz"),
)
  ? it
  : it.skip;

suite("testing wasm bundle", () => {
  it("carries a signing host", () => {
    expect(readFileSync(testingGlue, "utf8")).toContain(
      "export class WasmSigningHostRuntime",
    );
  });

  it("is the only bundle that can answer allocation as granted", () => {
    // `setGrantAllowancesUnchecked` hands a product a grant nothing allocated.
    // It is gated on the non-default `test-host` Cargo feature, which
    // `scripts/build-wasm.mjs` turns on only for this bundle. A shipping
    // signing host built with `--signing-host` must not expose it either.
    expect(readFileSync(testingGlue, "utf8")).toContain(
      "setGrantAllowancesUnchecked",
    );
    expect(readFileSync(webGlue, "utf8")).not.toContain(
      "setGrantAllowancesUnchecked",
    );
  });

  it("is the only bundle that can keep a preimage submission local", () => {
    // `setSubmitPreimagesLocally` answers a submission without the Bulletin
    // chain. Like the allocation shortcut above it is gated on `test-host`.
    expect(readFileSync(testingGlue, "utf8")).toContain(
      "setSubmitPreimagesLocally",
    );
    expect(readFileSync(webGlue, "utf8")).not.toContain(
      "setSubmitPreimagesLocally",
    );
  });

  // Only a release build writes the sidecars, so on a dev-profile `dist` there
  // is nothing for the exclusion to exclude and a green result would prove
  // nothing. Skipping says that; passing would not.
  sidecarsBuilt("publishes the wasm without its precompressed sidecars", () => {
    // `.wasm.gz` and `.wasm.br` are for a host app's static server to serve.
    // Nothing in the package resolves them and no bundler reads them, so in
    // the tarball they were 23MB every consumer downloaded and never opened.
    // Checked against the real file list rather than the `files` field, so an
    // exclusion dropped there fails here too.
    const listed: { path: string }[] = JSON.parse(
      execFileSync("npm", ["pack", "--dry-run", "--ignore-scripts", "--json"], {
        cwd: packageRoot,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      }),
    )[0].files;
    const paths = listed.map((file) => file.path);

    // The bundles themselves must still ship, so an exclusion widened to
    // `*.wasm*` is a failure here rather than a package that loads nothing.
    expect(paths).toContain("dist/wasm/testing/truapi_server_bg.wasm");
    expect(paths).toContain("dist/wasm/web/truapi_server_bg.wasm");

    expect(paths.filter((path) => /\.wasm\.(gz|br)$/.test(path))).toEqual([]);
  });
});
