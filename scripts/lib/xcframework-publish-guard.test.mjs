import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { run } from "./truapi-host-release.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const packages = [
  {
    name: "provider",
    script: "ios/truapi-provider/scripts/publish.sh",
    framework: "truapi_provider.xcframework",
    // publish.sh rejects a simulator-only build before it looks at headers.
    slices: ["ios-arm64", "ios-arm64-simulator"],
  },
  {
    name: "host",
    script: "ios/truapi-host/scripts/publish.sh",
    framework: "truapi_server.xcframework",
    slices: ["ios-arm64", "ios-arm64-simulator"],
  },
];

function writeXcframework(root, { slices, framework, stripped }) {
  mkdirSync(root, { recursive: true });
  writeFileSync(join(root, "Info.plist"), "<plist></plist>\n");
  const module = framework.replace(".xcframework", "FFI");
  for (const slice of slices) {
    const headers = join(root, slice, "Headers");
    mkdirSync(headers, { recursive: true });
    writeFileSync(join(headers, `${module}.h`), "// generated\n");
    if (!stripped) {
      writeFileSync(join(headers, "module.modulemap"), `module ${module} {}\n`);
    }
  }
}

// A regressed guard falls through to `gh release create`, so the dirty
// Package.swift and the stub gh are what keep a failing test off GitHub.
function fixture(pkg, { stripped }) {
  const workspace = mkdtempSync(
    join(tmpdir(), `truapi-publish-guard-${pkg.name}-`),
  );
  const script = join(workspace, pkg.script);
  mkdirSync(dirname(script), { recursive: true });
  symlinkSync(join(repoRoot, pkg.script), script);
  writeXcframework(join(dirname(dirname(script)), "Binaries", pkg.framework), {
    ...pkg,
    stripped,
  });

  writeFileSync(join(workspace, "Package.swift"), "let version = 1\n");
  const git = (...arguments_) =>
    execFileSync("git", ["-C", workspace, ...arguments_], { stdio: "ignore" });
  git("init", "-q");
  git("add", "-A");
  git(
    "-c",
    "user.email=test@example.com",
    "-c",
    "user.name=test",
    "-c",
    "commit.gpgsign=false",
    "commit",
    "-qm",
    "fixture",
  );
  writeFileSync(join(workspace, "Package.swift"), "let version = 2\n");

  const stubs = join(workspace, "stubs");
  mkdirSync(stubs);
  const gh = join(stubs, "gh");
  writeFileSync(gh, '#!/bin/sh\necho "stub gh refused: $*" >&2\nexit 70\n');
  chmodSync(gh, 0o755);

  return { workspace, stubs, script };
}

async function withFixture(pkg, options, body) {
  const { workspace, stubs, script } = fixture(pkg, options);
  try {
    const result = await run("sh", [script, "0.0.1"], {
      cwd: workspace,
      env: { ...process.env, PATH: `${stubs}:${process.env.PATH}` },
    });
    assert.doesNotMatch(result.stderr, /stub gh refused/, "the run reached gh");
    await body(result);
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
}

for (const pkg of packages) {
  test(`${pkg.name} publish refuses an unstripped xcframework`, async () => {
    await withFixture(pkg, { stripped: false }, (result) => {
      assert.equal(result.status, 66, result.stderr);
      assert.match(result.stderr, /still carries per-slice modulemaps/);
    });
  });

  // 65 is the dirty-manifest check, so reaching it means the guard passed.
  test(`${pkg.name} publish passes a stripped xcframework`, async () => {
    await withFixture(pkg, { stripped: true }, (result) => {
      assert.equal(result.status, 65, result.stderr);
      assert.doesNotMatch(result.stderr, /modulemap/);
      assert.match(result.stderr, /Package\.swift has uncommitted changes/);
    });
  });
}
