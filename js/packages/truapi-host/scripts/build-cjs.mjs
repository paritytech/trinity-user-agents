// Emit CommonJS alongside the ESM build for the `./testing` entries.
//
// Playwright resolves a consumer's test files by the nearest package.json
// `type`, so a CJS consumer (no `"type": "module"`) can only load these through
// a `require` condition. `@parity/host-api-test-sdk` ships one; without it a
// CJS repo cannot adopt this fixture at all.
//
// Output sits beside the ESM file rather than under a `cjs/` root on purpose:
// `server.js` finds its bundles with `resolve(__dirname, "../..", "dist")`, so
// a different nesting depth would point that at the wrong directory.
import { build } from "esbuild";

// Only the `dist/testing/*` entries: they share one directory depth, which is
// what keeps `server.js`'s `resolve(__dirname, "../..", "dist")` correct in the
// bundled output. The root `./testing` entry sits one level up and is not
// emitted, so a CJS consumer imports the specific subpath.
const ENTRIES = [
  "testing/playwright",
  "testing/server",
  "testing/dev-accounts",
  "testing/create-mock-client",
  "testing/host-page",
];

await Promise.all(
  ENTRIES.map((entry) =>
    build({
      entryPoints: [`dist/${entry}.js`],
      outfile: `dist/${entry}.cjs`,
      bundle: true,
      format: "cjs",
      platform: "node",
      target: "node18",
      // Peer and optional deps stay external: bundling Playwright would give a
      // test a second copy of its own runner, and esbuild is loaded lazily.
      external: ["@playwright/test", "esbuild", "@parity/truapi"],
      // `server.js` derives its own directory from `import.meta.url`, which is
      // empty under `cjs`. Point it at the real file instead, so it still finds
      // `dist/` to bundle the browser entries out of.
      banner: {
        js: "const __esm_import_meta_url = require('url').pathToFileURL(__filename).href;",
      },
      define: { "import.meta.url": "__esm_import_meta_url" },
      logLevel: "warning",
    }),
  ),
);
console.log(`built ${ENTRIES.length} cjs entries`);
