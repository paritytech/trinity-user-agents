#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Keep release metadata derived from @parity/truapi's version in sync.
 *
 * Update the tracked Cargo.toml versions and the host and truapi-debugger
 * dependency ranges:
 *   npm run sync-release-versions
 *
 * Verify those files and package-lock.json without writing changes:
 *   npm run check-release-versions
 *
 * `npm run version-packages` runs the update after consuming changesets and
 * then refreshes package-lock.json.
 */

const command = "sync-release-versions";
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const truapiPath = resolve(repoRoot, "js/packages/truapi/package.json");
const hostPath = resolve(repoRoot, "js/packages/truapi-host/package.json");
// Crates whose version tracks @parity/truapi: the protocol crate itself, and
// the host CLI, whose published binaries are cut on every protocol release.
const cargoPaths = [
  "rust/crates/truapi/Cargo.toml",
  "rust/crates/truapi-host-cli/Cargo.toml",
].map((path) => resolve(repoRoot, path));
const debuggerPath = resolve(
  repoRoot,
  "js/packages/truapi-debugger/package.json",
);
const lockPath = resolve(repoRoot, "package-lock.json");
const check = process.argv.includes("--check");

const truapi = readJson(truapiPath);
const host = readJson(hostPath);
const wireDebugger = readJson(debuggerPath);
if (typeof truapi.version !== "string" || truapi.version.length === 0) {
  fail(`could not read .version from ${truapiPath}`);
}

const expectedDependency = `^${truapi.version}`;
const actualDependency = host.dependencies?.["@parity/truapi"];
// The debugger decodes with the generated codecs from `@parity/truapi`, so a
// version skew between them is the exact failure the schema-hash gate exists to
// catch at runtime - cheaper to catch here, at release time.
const actualDebuggerDependency = wireDebugger.dependencies?.["@parity/truapi"];
const cargoVersionLine = /^version = "([^"]*)"$/m;
const cargoManifests = cargoPaths.map((path) => {
  const contents = readFile(path);
  const version = contents.match(cargoVersionLine)?.[1];
  if (version === undefined) {
    fail(`could not find a top-level \`version = "..."\` line in ${path}`);
  }
  return { path, contents, version };
});

if (check) {
  const errors = [];
  for (const manifest of cargoManifests) {
    if (manifest.version !== truapi.version) {
      errors.push(
        `${relative(repoRoot, manifest.path)} is ${manifest.version}; expected ${truapi.version}`,
      );
    }
  }
  if (actualDependency !== expectedDependency) {
    errors.push(
      `js/packages/truapi-host/package.json requires ${actualDependency ?? "<missing>"}; expected ${expectedDependency}`,
    );
  }

  if (actualDebuggerDependency !== expectedDependency) {
    errors.push(
      `js/packages/truapi-debugger/package.json requires ${actualDebuggerDependency ?? "<missing>"}; expected ${expectedDependency}`,
    );
  }

  const lock = readJson(lockPath);
  const lockedTruapi = lock.packages?.["js/packages/truapi"]?.version;
  const lockedHost = lock.packages?.["js/packages/truapi-host"]?.version;
  const lockedDependency =
    lock.packages?.["js/packages/truapi-host"]?.dependencies?.[
      "@parity/truapi"
    ];
  if (lockedTruapi !== truapi.version) {
    errors.push(
      `package-lock.json records @parity/truapi ${lockedTruapi ?? "<missing>"}; expected ${truapi.version}`,
    );
  }
  if (lockedHost !== host.version) {
    errors.push(
      `package-lock.json records @parity/truapi-host ${lockedHost ?? "<missing>"}; expected ${host.version}`,
    );
  }
  if (lockedDependency !== expectedDependency) {
    errors.push(
      `package-lock.json records the host dependency as ${lockedDependency ?? "<missing>"}; expected ${expectedDependency}`,
    );
  }
  // The debugger's two lock fields. This script's writer does NOT touch the
  // lock; `npm install --package-lock-only` inside `version-packages` does.
  // Left unchecked they could drift from package.json unnoticed - and
  // release.yml reads package.json, not the lock, so nothing else looks.
  const lockedDebugger =
    lock.packages?.["js/packages/truapi-debugger"]?.version;
  const lockedDebuggerDependency =
    lock.packages?.["js/packages/truapi-debugger"]?.dependencies?.[
      "@parity/truapi"
    ];
  if (lockedDebugger !== wireDebugger.version) {
    errors.push(
      `package-lock.json records @parity/truapi-debugger ${lockedDebugger ?? "<missing>"}; expected ${wireDebugger.version}`,
    );
  }
  if (lockedDebuggerDependency !== expectedDependency) {
    errors.push(
      `package-lock.json records the truapi-debugger dependency as ${lockedDebuggerDependency ?? "<missing>"}; expected ${expectedDependency}`,
    );
  }

  if (errors.length > 0) {
    for (const error of errors) console.error(`${command}: ${error}`);
    console.error(`${command}: run \`npm run version-packages\` to sync`);
    process.exit(1);
  }

  console.log(
    `${command}: crate manifests, host and truapi-debugger dependencies, and package-lock.json use @parity/truapi ${truapi.version}`,
  );
  process.exit(0);
}

for (const manifest of cargoManifests) {
  const name = relative(repoRoot, manifest.path);
  const next = manifest.contents.replace(
    cargoVersionLine,
    `version = "${truapi.version}"`,
  );
  if (next === manifest.contents) {
    console.log(`${command}: ${name} already uses ${truapi.version}`);
  } else {
    writeFileSync(manifest.path, next);
    console.log(`${command}: updated ${name} to ${truapi.version}`);
  }
}

if (actualDependency === expectedDependency) {
  console.log(
    `${command}: host already requires @parity/truapi ${expectedDependency}`,
  );
} else {
  host.dependencies ??= {};
  host.dependencies["@parity/truapi"] = expectedDependency;
  writeFileSync(hostPath, `${JSON.stringify(host, null, 2)}\n`);
  console.log(
    `${command}: updated host dependency to @parity/truapi ${expectedDependency}`,
  );
}

// The debugger needs a writer as well as a check. With only the check, `--check`
// failed with "expected ^X" and told the reader to run this command to fix it -
// which then wrote Cargo.toml and the host dep and left the debugger untouched,
// so the advice could not clear the error it printed.
if (actualDebuggerDependency === expectedDependency) {
  console.log(
    `${command}: truapi-debugger already requires @parity/truapi ${expectedDependency}`,
  );
} else {
  wireDebugger.dependencies ??= {};
  wireDebugger.dependencies["@parity/truapi"] = expectedDependency;
  writeFileSync(debuggerPath, `${JSON.stringify(wireDebugger, null, 2)}\n`);
  console.log(
    `${command}: updated truapi-debugger dependency to @parity/truapi ${expectedDependency}`,
  );
}

function readJson(path) {
  return JSON.parse(readFile(path));
}

function readFile(path) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    console.error(`${command}: could not read ${path}`);
    console.error(error);
    process.exit(1);
  }
}

function fail(message) {
  console.error(`${command}: ${message}`);
  process.exit(1);
}
