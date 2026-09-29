import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

// The worker runtime imports the production core, `./wasm/web/truapi_server.js`,
// which has no signing host in it: a production browser host pairs with a
// wallet and never holds keys. This host holds a development wallet, so its
// worker loads the `testing` core instead, the only bundle built with
// `wasm-signing-host`. The testing server in `@parity/truapi-host` redirects
// the same specifier for the same reason.
const signingCore = {
  find: /^\.\/wasm\/web\/truapi_server\.js$/,
  replacement: "@parity/truapi-host/wasm/testing",
};

export default defineConfig({
  resolve: { alias: [signingCore] },
  worker: { format: "es" },
  server: {
    port: 5180,
    strictPort: true,
    // The in-tree packages are linked from outside this directory.
    fs: { allow: [repoRoot] },
  },
  preview: { port: 5180, strictPort: true },
  build: { target: "es2022" },
});
