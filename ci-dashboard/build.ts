// Builds the dashboard page into a directory, ready to publish.
//
//   bun ci-dashboard/build.ts <out-dir>
//
// The image runs this into ci-dashboard/dist, which the server serves.

import { cp, mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";

const destination = process.argv[2];
if (!destination) {
  throw new Error("usage: build.ts <out-dir>");
}
const out = resolve(destination);
await mkdir(out, { recursive: true });

const result = await Bun.build({
  entrypoints: [join(import.meta.dir, "src/page.ts")],
  target: "browser",
  format: "esm",
  minify: true,
});
if (!result.success || result.outputs.length !== 1) {
  throw new Error(`Cannot bundle the page: ${result.logs.join("\n")}`);
}
await Bun.write(join(out, "dashboard.js"), result.outputs[0]!);
await cp(join(import.meta.dir, "site"), out, { recursive: true });
console.log(`Dashboard written to ${out}`);
