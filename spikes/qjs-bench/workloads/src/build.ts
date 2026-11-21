// T3: build dist/workloads.js (one minified IIFE, as a product would ship),
// dist/webview-shim.js for E1, and one wrapper per workload that sets
// __BENCH and then runs the bundle, so an engine only ever evaluates one file.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { WORKLOADS } from "./workload-table";

const dist = new URL("../dist/", import.meta.url);
mkdirSync(dist, { recursive: true });

for (const [entry, name] of [
  ["bundle-entry.ts", "workloads.js"],
  ["webview-shim.ts", "webview-shim.js"],
] as const) {
  const result = await Bun.build({
    entrypoints: [new URL(entry, import.meta.url).pathname],
    format: "iife",
    target: "browser",
    minify: true,
  });
  if (!result.success) {
    for (const message of result.logs) console.error(message);
    process.exit(1);
  }
  writeFileSync(new URL(name, dist), await result.outputs[0].text());
}

const bundle = readFileSync(new URL("workloads.js", dist), "utf8");
for (const [name, config] of Object.entries(WORKLOADS)) {
  writeFileSync(new URL(`w-${name}.js`, dist), `globalThis.__BENCH=${JSON.stringify(config)};\n${bundle}`);
}
console.log(`built workloads.js (${bundle.length} bytes) and ${Object.keys(WORKLOADS).length} workload wrappers`);
