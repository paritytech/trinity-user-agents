#!/usr/bin/env node
// Rebuild the browser WASM artefacts of the `truapi` runtime generated under
// `dist/wasm/`. Requires wasm-pack, a matching wasm-bindgen CLI, and wasm-opt.
// The core keeps its published `truapi_server` file names.
//
// Two bundles are built, and the difference is deliberate:
//
//   web/      the production browser host. Built with the `runtime` feature
//             alone, so it carries no signing host: a browser host pairs with a wallet that
//             holds the keys, and never holds key material itself.
//   testing/  the mock host used by tests. Adds `wasm-signing-host`, because a
//             test host owns dev accounts and signs locally instead of waiting
//             on a wallet that is not there, and `test-host`, which carries the
//             shortcuts a shipping host must not have. Shipped under the
//             `./testing` subpath so a product bundling `./web` never pulls it
//             in.
//
// Each carries its own copy of `truapi_verifiable`, the ring-VRF module the
// core loads on first use, beside the core's own files. It is built first, and
// both cores are built against its hash.

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import {
  brotliCompress,
  brotliDecompress,
  constants as zlibConstants,
  gzip,
  gunzip,
} from "node:zlib";

const execFileAsync = promisify(execFile);
const brotliCompressAsync = promisify(brotliCompress);
const brotliDecompressAsync = promisify(brotliDecompress);
const gzipAsync = promisify(gzip);
const gunzipAsync = promisify(gunzip);
const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "../../..");
const wasmProfile = process.env.TRUAPI_WASM_PROFILE ?? "release";

function args(crate, outName, target, outDir, features = []) {
  const command = [
    "build",
    "--target",
    target,
    "--out-dir",
    outDir,
    "--out-name",
    outName,
  ];
  if (wasmProfile === "dev") {
    command.push("--dev");
  } else if (wasmProfile === "profiling") {
    command.push("--profiling");
  } else if (wasmProfile !== "release") {
    throw new Error(
      `Unsupported TRUAPI_WASM_PROFILE=${wasmProfile}; expected release, dev, or profiling`,
    );
  }
  command.push(
    resolve(repoRoot, "rust/crates", crate),
    "--no-default-features",
  );
  if (features.length > 0) {
    command.push("--features", features.join(","));
  }
  return command;
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  return `${(kib / 1024).toFixed(2)} MiB`;
}

function readVarUint(bytes, cursor) {
  let result = 0;
  let shift = 0;
  let position = cursor;
  while (position < bytes.length) {
    const byte = bytes[position];
    result += (byte & 0x7f) * 2 ** shift;
    position += 1;
    if ((byte & 0x80) === 0) {
      return [result, position];
    }
    shift += 7;
  }
  throw new Error("unterminated wasm varuint");
}

function readCustomSectionNames(bytes) {
  if (
    bytes.length < 8 ||
    bytes[0] !== 0x00 ||
    bytes[1] !== 0x61 ||
    bytes[2] !== 0x73 ||
    bytes[3] !== 0x6d
  ) {
    throw new Error("generated file is not a wasm module");
  }

  const names = [];
  let offset = 8;
  while (offset < bytes.length) {
    const sectionId = bytes[offset];
    offset += 1;
    const [sectionSize, payloadStart] = readVarUint(bytes, offset);
    const payloadEnd = payloadStart + sectionSize;
    if (payloadEnd > bytes.length) {
      throw new Error("wasm section extends past end of file");
    }
    if (sectionId === 0) {
      const [nameLength, nameStart] = readVarUint(bytes, payloadStart);
      const nameEnd = nameStart + nameLength;
      if (nameEnd > payloadEnd) {
        throw new Error("wasm custom section name extends past section end");
      }
      names.push(
        Buffer.from(bytes.subarray(nameStart, nameEnd)).toString("utf8"),
      );
    }
    offset = payloadEnd;
  }
  return names;
}

async function validateReleaseWasm(wasmPath) {
  if (wasmProfile !== "release") return;

  const wasm = await readFile(wasmPath);
  const customSections = readCustomSectionNames(wasm);
  const forbidden = customSections.filter(
    (name) =>
      name === "name" || name === "producers" || name.startsWith(".debug"),
  );
  if (forbidden.length > 0) {
    throw new Error(
      `release wasm retained debug/metadata custom sections: ${forbidden.join(", ")}`,
    );
  }
}

async function writeCompressedSidecars(wasmPath) {
  if (wasmProfile !== "release") return;

  const wasm = await readFile(wasmPath);
  const gzipBytes = await gzipAsync(wasm, { level: 9 });
  const brotliBytes = await brotliCompressAsync(wasm, {
    params: {
      [zlibConstants.BROTLI_PARAM_QUALITY]: 11,
    },
  });

  await writeFile(`${wasmPath}.gz`, gzipBytes);
  await writeFile(`${wasmPath}.br`, brotliBytes);

  const [gzipRoundTrip, brotliRoundTrip] = await Promise.all([
    gunzipAsync(gzipBytes),
    brotliDecompressAsync(brotliBytes),
  ]);
  if (!gzipRoundTrip.equals(wasm) || !brotliRoundTrip.equals(wasm)) {
    throw new Error("compressed wasm sidecar round-trip validation failed");
  }

  process.stdout.write(
    [
      `wasm size: ${formatBytes(wasm.length)}`,
      `gzip: ${formatBytes(gzipBytes.length)}`,
      `brotli: ${formatBytes(brotliBytes.length)}`,
    ].join(" | ") + "\n",
  );
}

// The core is an rlib for no_std consumers. Request its cdylib only here;
// wasm-pack requires cdylib in Cargo.toml even when rustc can emit it explicitly.
async function buildCore(outName, target, outDir, features, env) {
  const crateDir = resolve(repoRoot, "rust/crates/truapi");
  const cargoArgs = [
    "--manifest-path", resolve(crateDir, "Cargo.toml"),
    "--no-default-features",
    "--features", features.join(","),
  ];
  const options = {
    cwd: repoRoot,
    env: { ...process.env, ...env },
    maxBuffer: 32 * 1024 * 1024,
  };
  const { stdout: metadataJson } = await execFileAsync("cargo", [
    "metadata", "--format-version", "1",
    "--filter-platform", "wasm32-unknown-unknown",
    ...cargoArgs,
  ], options);
  const metadata = JSON.parse(metadataJson);
  const pkg = metadata.packages.find((pkg) => pkg.name === "truapi");
  const coreNode = metadata.resolve.nodes.find((node) => node.id === pkg.id);
  const bindgenId = coreNode.deps.find((dep) => dep.name === "wasm_bindgen").pkg;
  const bindgenVersion = metadata.packages.find((pkg) => pkg.id === bindgenId).version;
  const { stdout: cliVersion } = await execFileAsync("wasm-bindgen", ["--version"], options);
  if (cliVersion.trim() !== `wasm-bindgen ${bindgenVersion}`) {
    throw new Error(
      `wasm-bindgen CLI must match ${bindgenVersion}; run ` +
      `cargo install wasm-bindgen-cli --version ${bindgenVersion} --locked --force`,
    );
  }
  const { stdout, stderr } = await execFileAsync("cargo", [
    "rustc", "--lib", "--crate-type", "cdylib",
    "--target", "wasm32-unknown-unknown",
    ...(wasmProfile === "dev" ? [] : ["--release"]),
    "--message-format", "json-render-diagnostics",
    ...cargoArgs,
  ], options);
  process.stderr.write(stderr);
  const artifacts = stdout.trim().split("\n").map((line) => JSON.parse(line));
  const wasmFiles = artifacts
    .filter((artifact) => artifact.reason === "compiler-artifact" && artifact.package_id === pkg.id)
    .flatMap((artifact) => artifact.filenames)
    .filter((file) => file.endsWith(".wasm"));
  if (wasmFiles.length !== 1) {
    throw new Error(`Expected one truapi Wasm artifact, found ${wasmFiles.length}`);
  }

  const profile = pkg.metadata["wasm-pack"]?.profile?.[wasmProfile] ?? {};
  const bindgen = profile["wasm-bindgen"] ?? {};
  await rm(outDir, { recursive: true, force: true });
  await mkdir(outDir, { recursive: true });
  await execFileAsync("wasm-bindgen", [
    wasmFiles[0], "--out-dir", outDir, "--out-name", outName,
    "--target", target, "--typescript",
    ...((bindgen["debug-js-glue"] ?? (wasmProfile === "dev")) ? ["--debug"] : []),
    ...(bindgen["demangle-name-section"] === false ? ["--no-demangle"] : []),
    ...(bindgen["dwarf-debug-info"] ? ["--keep-debug"] : []),
    ...(bindgen["omit-default-module-path"] ? ["--omit-default-module-path"] : []),
    ...(bindgen["split-linked-modules"] ? ["--split-linked-modules"] : []),
  ], options);
  const optimize = profile["wasm-opt"] ?? (wasmProfile !== "dev");
  if (optimize !== false) {
    const wasmPath = resolve(outDir, `${outName}_bg.wasm`);
    const optimizedPath = resolve(outDir, `${outName}_bg.wasm-opt.wasm`);
    await execFileAsync("wasm-opt", [
      wasmPath, "-o", optimizedPath,
      ...(Array.isArray(optimize) ? optimize : ["-O"]),
    ], options);
    await rename(optimizedPath, wasmPath);
  }

  // Include workspace licenses for this crate's inherited license, and let
  // any crate-local licenses override files with the same name.
  for (const dir of [metadata.workspace_root, crateDir]) {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.startsWith("LICENSE")) {
        await cp(resolve(dir, entry.name), resolve(outDir, entry.name));
      }
    }
  }
  if (pkg.readme) {
    await cp(resolve(crateDir, pkg.readme), resolve(outDir, "README.md"));
  }
  if (pkg.license_file) {
    await cp(resolve(crateDir, pkg.license_file), resolve(outDir, basename(pkg.license_file)));
  }
  const generatedFiles = await readdir(outDir);
  const dependencies = generatedFiles.includes("package.json")
    ? JSON.parse(await readFile(resolve(outDir, "package.json"), "utf8"))
    : undefined;
  await writeFile(resolve(outDir, "package.json"), JSON.stringify({
    name: pkg.name,
    ...(target === "web" ? { type: "module", sideEffects: ["./snippets/*"] } : {}),
    description: pkg.description,
    version: pkg.version,
    license: pkg.license ?? (pkg.license_file ? `SEE LICENSE IN ${basename(pkg.license_file)}` : undefined),
    ...(pkg.repository ? { repository: { type: "git", url: pkg.repository } } : {}),
    ...(pkg.homepage ? { homepage: pkg.homepage } : {}),
    ...(pkg.authors.length ? { collaborators: pkg.authors } : {}),
    ...(pkg.keywords.length ? { keywords: pkg.keywords } : {}),
    files: [
      `${outName}_bg.wasm`, `${outName}.js`, `${outName}.d.ts`,
      ...generatedFiles.filter((file) => file.startsWith("LICENSE") && file !== "LICENSE"),
      ...(generatedFiles.includes("snippets") ? ["snippets"] : []),
    ],
    main: `${outName}.js`,
    types: `${outName}.d.ts`,
    dependencies,
  }, null, 2) + "\n");
}

async function build(crate, outName, target, subdir, features = [], env = {}) {
  const outDir = resolve(pkgRoot, "dist/wasm", subdir);
  process.stdout.write(
    `${crate === "truapi" ? "cargo rustc + wasm-bindgen + wasm-opt" : "wasm-pack build"} ${crate} --target ${target} --${wasmProfile}${
      features.length > 0 ? ` --features ${features.join(",")}` : ""
    } → ${outDir}\n`,
  );
  try {
    const packArgs = args(crate, outName, target, outDir, features);
    if (crate === "truapi") {
      await buildCore(outName, target, outDir, features, env);
    } else {
      await execFileAsync("wasm-pack", packArgs, {
        cwd: repoRoot,
        env: { ...process.env, ...env },
      });
    }
  } catch (err) {
    if (err?.code === "ENOENT") {
      console.error(
        "Wasm builds require cargo, wasm-pack, wasm-bindgen matching Cargo.lock, " +
          "and wasm-opt (Binaryen 117) on PATH. See the package README.",
      );
    }
    throw err;
  }
  // wasm-pack writes a nested `.gitignore: *`; the repo-level ignore already
  // owns generated WASM outputs.
  await rm(resolve(outDir, ".gitignore"), { force: true });
  const wasmPath = resolve(outDir, `${outName}_bg.wasm`);
  await Promise.all([
    rm(`${wasmPath}.br`, { force: true }),
    rm(`${wasmPath}.gz`, { force: true }),
  ]);
  await validateReleaseWasm(wasmPath);
  await writeCompressedSidecars(wasmPath);
  return wasmPath;
}

// The cores are built against this hash and load no other module. It is staged
// apart, since each bundle writes its own `package.json`, and only
// the module's glue and payload are copied into each bundle.
const verifiableStage = resolve(pkgRoot, "dist/wasm/.verifiable");
const verifiableWasm = await build(
  "truapi-verifiable",
  "truapi_verifiable",
  "web",
  ".verifiable",
);
const env = {
  TRUAPI_VERIFIABLE_SHA256: createHash("sha256")
    .update(await readFile(verifiableWasm))
    .digest("hex"),
};
// `--web-only` skips the testing bundle, for a caller that needs only what
// ships to products.
const bundles = process.argv.includes("--web-only")
  ? ["web"]
  : ["web", "testing"];
const webFeatures = process.argv.includes("--signing-host")
  ? ["wasm-signing-host"]
  : ["runtime"];
await build("truapi", "truapi_server", "web", "web", webFeatures, env);
if (bundles.includes("testing")) {
  await build(
    "truapi",
    "truapi_server",
    "web",
    "testing",
    ["wasm-signing-host", "test-host"],
    env,
  );
}
// The compressed sidecars exist only in a release build.
const verifiableFiles = [
  "truapi_verifiable.js",
  "truapi_verifiable_bg.wasm",
  ...(wasmProfile === "release"
    ? ["truapi_verifiable_bg.wasm.br", "truapi_verifiable_bg.wasm.gz"]
    : []),
];
for (const bundle of bundles) {
  for (const file of verifiableFiles) {
    await cp(
      resolve(verifiableStage, file),
      resolve(pkgRoot, "dist/wasm", bundle, file),
    );
  }
}
await rm(verifiableStage, { recursive: true, force: true });
