// Proves the server can actually serve a runnable page. The risk it covers is
// the bundling step: the browser entry reaches modules that import bare
// specifiers, and if esbuild cannot resolve them the failure surfaces here
// rather than as a blank page and a fixture timeout.
import { describe, expect, it } from "bun:test";

import { wasmIsBuilt } from "./require-wasm.js";
import { createTestHostServer } from "./server.js";

// `wasmIsBuilt`, not a bare `existsSync`: it is what turns a missing artefact
// into a failure under `REQUIRE_WASM=1` instead of a silent skip.
const suite = wasmIsBuilt("testing/truapi_server.js")
  ? describe
  : describe.skip;

suite("test host server", () => {
  it("serves a page and a bundle with its bare imports resolved", async () => {
    const server = await createTestHostServer();
    try {
      const page = await fetch(server.url).then((r) => r.text());
      expect(page).toContain('id="product-container"');
      expect(page).toContain("/test-host.js");

      const bundle = await fetch(`${server.url}/test-host.js`);
      expect(bundle.headers.get("content-type")).toContain("text/javascript");
      const source = await bundle.text();

      // Bare specifiers must be gone: anything still importing by package name
      // would fail to resolve in the browser.
      expect(source).not.toMatch(
        /from\s*"(@parity\/truapi|@noble\/hashes|neverthrow)/,
      );
      // And the mock has to actually be in there.
      expect(source).toContain("__TRUAPI_TEST_HOST__");
    } finally {
      await server.close();
    }
  });

  it("serves the wasm glue from disk, and 404s what it does not have", async () => {
    const server = await createTestHostServer();
    try {
      const glue = await fetch(`${server.url}/wasm/testing/truapi_server.js`);
      expect(glue.status).toBe(200);
      expect(glue.headers.get("content-type")).toContain("text/javascript");

      const wasm = await fetch(
        `${server.url}/wasm/testing/truapi_server_bg.wasm`,
      );
      expect(wasm.status).toBe(200);
      expect(wasm.headers.get("content-type")).toBe("application/wasm");

      // Note: the escape guard in `serveFromDist` is not exercised here. A
      // conforming client normalises `..` out of the URL before sending, so a
      // traversal never reaches that branch through `fetch`; the guard is
      // defence against a raw client that sends the path unnormalised.
      const missing = await fetch(`${server.url}/wasm/testing/nope.js`);
      expect(missing.status).toBe(404);
    } finally {
      await server.close();
    }
  });
});
