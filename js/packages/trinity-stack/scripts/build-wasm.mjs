#!/usr/bin/env node
// Builds the trinity-stack crate with its `js` feature into wasm/, which the package ships next
// to dist/. wasm-pack is required.

import { execFile } from "node:child_process";
import { readFile, rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const pkgRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(pkgRoot, "../../..");
const outDir = resolve(pkgRoot, "wasm");

try {
  await execFileAsync(
    "wasm-pack",
    [
      "build",
      "--target",
      "web",
      "--release",
      "--out-dir",
      outDir,
      "--out-name",
      "trinity_stack",
      resolve(repoRoot, "rust/crates/trinity-stack"),
      "--features",
      "js",
    ],
    { cwd: repoRoot },
  );
} catch (err) {
  if (err?.code === "ENOENT") {
    console.error(
      "wasm-pack is required. Install it with `cargo install wasm-pack`.",
    );
    process.exit(1);
  }
  throw err;
}

// This package's own package.json, ignore rules and README are authoritative.
await Promise.all([
  rm(resolve(outDir, ".gitignore"), { force: true }),
  rm(resolve(outDir, "package.json"), { force: true }),
  rm(resolve(outDir, "README.md"), { force: true }),
]);

const wasm = await readFile(resolve(outDir, "trinity_stack_bg.wasm"));
process.stdout.write(`wasm size: ${(wasm.length / 1024).toFixed(1)} KiB\n`);
