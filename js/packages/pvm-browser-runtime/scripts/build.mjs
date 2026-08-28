import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const packageRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(packageRoot, "../../..");
const dist = resolve(packageRoot, "dist");
const source = resolve(packageRoot, "src");

function embeddedSource(bytes, name) {
  const source = bytes.toString("utf8");
  const marker = '"use strict";\n\n';
  if (!source.includes(marker)) {
    throw new Error(`${name} is missing its strict-mode prologue`);
  }
  return Buffer.from(source.replace(marker, ""), "utf8");
}

const wasm =
  process.env.PVM_RUNTIME_WASM ??
  resolve(
    repositoryRoot,
    "target/wasm32-unknown-unknown/release/pvm_runtime.wasm",
  );

if (process.env.PVM_RUNTIME_WASM === undefined) {
  const build = spawnSync(
    "cargo",
    [
      "build",
      "--locked",
      "--release",
      "--target",
      "wasm32-unknown-unknown",
      "-p",
      "pvm-runtime",
    ],
    { cwd: repositoryRoot, stdio: "inherit" },
  );
  if (build.status !== 0) process.exit(build.status ?? 1);
}

await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });
const translated = await readFile(resolve(source, "pvm-wasm-translated.js"));
const runtimeCore = await readFile(resolve(source, "pvm-runtime-core.js"));
const workerEntry = await readFile(resolve(source, "pvm-wasm-worker-entry.js"));
await copyFile(wasm, resolve(dist, "pvm-browser-runtime.wasm"));
await copyFile(
  resolve(source, "pvm-gpu-worker.js"),
  resolve(dist, "pvm-gpu-worker.js"),
);
await copyFile(
  resolve(source, "pvm-wasm-translated.js"),
  resolve(dist, "pvm-wasm-translated.js"),
);
await copyFile(
  resolve(source, "pvm-runtime-core.js"),
  resolve(dist, "pvm-runtime-core.js"),
);
await copyFile(
  resolve(source, "pvm-wasm-worker-entry.js"),
  resolve(dist, "pvm-wasm-worker-entry.js"),
);
await writeFile(
  resolve(dist, "pvm-worker.js"),
  Buffer.concat([
    translated,
    Buffer.from("\n"),
    embeddedSource(runtimeCore, "pvm-runtime-core.js"),
    Buffer.from("\n"),
    embeddedSource(workerEntry, "pvm-wasm-worker-entry.js"),
  ]),
);

const files = [
  "pvm-browser-runtime.wasm",
  "pvm-worker.js",
  "pvm-gpu-worker.js",
  "pvm-wasm-translated.js",
  "pvm-runtime-core.js",
  "pvm-wasm-worker-entry.js",
];
const sums = [];
for (const file of files) {
  const bytes = await readFile(resolve(dist, file));
  sums.push(`${createHash("sha256").update(bytes).digest("hex")}  ${file}`);
}
await writeFile(resolve(dist, "SHA256SUMS"), `${sums.join("\n")}\n`);
