import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  formatBytes,
  measureGroups,
  parseAssets,
  renderReport,
  stableName,
} from "./bundle-size.mjs";

function tree(files) {
  const root = mkdtempSync(join(tmpdir(), "bundle-size-"));
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(root, path, ".."), { recursive: true });
    writeFileSync(join(root, path), content);
  }
  return root;
}

const GROUPS = [
  { name: "wasm", dir: "pkg/dist/wasm" },
  {
    name: "pkg",
    dir: "pkg/dist",
    exclude: ["wasm/", "testing/", "testing.js"],
  },
  { name: "other", dir: "other/dist" },
];

test("measures shipped JS and WASM only, each file in exactly one group", () => {
  const root = tree({
    "pkg/dist/index.js": "export const a = 1;\n",
    "pkg/dist/index.d.ts": "export declare const a: number;\n",
    "pkg/dist/index.js.map": "{}",
    "pkg/dist/testing.js": "export * from './testing/server.js';\n",
    "pkg/dist/testing/server.cjs": "module.exports = {};\n",
    "pkg/dist/web/index.cjs": "module.exports = {};\n",
    "pkg/dist/wasm/core_bg.wasm": "\0asm",
    "pkg/dist/wasm/core_bg.wasm.gz": "sidecar",
    "pkg/dist/wasm/snippets/core-0123456789abcdef/inline0.js": "x",
    "other/dist/main.mjs": "export {};\n",
  });
  try {
    const files = measureGroups(root, GROUPS);
    assert.deepEqual(Object.keys(files).sort(), [
      "other/main.mjs",
      "pkg/index.js",
      "pkg/web/index.cjs",
      "wasm/core_bg.wasm",
      "wasm/snippets/core/inline0.js",
    ]);
    assert.equal(files["wasm/core_bg.wasm"].group, "wasm");
    assert.equal(files["wasm/core_bg.wasm"].raw, 4);
    assert.ok(files["pkg/index.js"].gzip > 0);
    assert.ok(files["pkg/index.js"].brotli > 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fails naming every group without build output", () => {
  const root = tree({ "pkg/dist/index.js": "1" });
  try {
    assert.throws(
      () => measureGroups(root, GROUPS),
      /no build output for: wasm \(pkg\/dist\/wasm\), other \(other\/dist\)/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("parses one group per line, with excludes and comments", () => {
  assert.deepEqual(
    parseAssets(`
      # the core
      wasm  pkg/dist/wasm
      pkg   pkg/dist  !wasm/ !testing.js  # no test host
    `),
    [
      { name: "wasm", dir: "pkg/dist/wasm", exclude: [] },
      { name: "pkg", dir: "pkg/dist", exclude: ["wasm/", "testing.js"] },
    ],
  );
});

test("rejects a group without a directory or with a bare extra word", () => {
  assert.throws(() => parseAssets("wasm"), /expected "<name> <dir>/);
  assert.throws(
    () => parseAssets("pkg pkg/dist wasm/"),
    /got "pkg pkg\/dist wasm\/"/,
  );
});

test("drops the wasm-bindgen snippet hash and nothing else", () => {
  assert.equal(
    stableName("snippets/truapi-c48d42883d2b3735/inline0.js"),
    "snippets/truapi/inline0.js",
  );
  assert.equal(stableName("truapi_server.js"), "truapi_server.js");
  assert.equal(
    stableName("chunk-0123456789abcdef.js"),
    "chunk-0123456789abcdef.js",
  );
});

test("formats sizes in binary units", () => {
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(1536), "1.5 KiB");
  assert.equal(formatBytes(3 * 1024 * 1024), "3.00 MiB");
});

const size = (group, raw) => ({ group, raw, gzip: raw / 2, brotli: raw / 4 });

test("reports deltas, new and removed files against a baseline", () => {
  const baseline = {
    commit: "aaaaaaaaaaaa",
    files: {
      "wasm/core_bg.wasm": size("wasm", 4096),
      "pkg/gone.js": size("pkg", 1024),
      "pkg/same.js": size("pkg", 100),
    },
  };
  const current = {
    commit: "bbbbbbbbbbbb",
    files: {
      "wasm/core_bg.wasm": size("wasm", 5120),
      "pkg/added.js": size("pkg", 2048),
      "pkg/same.js": size("pkg", 100),
    },
  };
  const report = renderReport({
    current,
    baseline,
    repoUrl: "https://example.test",
  });

  assert.match(report, /^## Bundle size report\n/);
  assert.match(
    report,
    /Compared with `main` at \[aaaaaaa\]\(https:\/\/example\.test\/commit\/aaaaaaaaaaaa\)/,
  );
  assert.match(report, /\| `wasm` \| 5\.0 KiB \(\+1\.0 KiB, \+25\.0%\) \|/);
  assert.match(report, /\| `pkg\/added\.js` \(new\) \| 2\.0 KiB \|/);
  assert.match(
    report,
    /\| `pkg\/gone\.js` \| removed \| removed \| removed \|/,
  );
  assert.match(report, /<summary>Changed files \(3\)<\/summary>/);
  assert.match(report, /\*\*WebAssembly modules\*\*/);
  assert.match(report, /<sub>Commit: \[bbbbbbb\]/);
  assert.doesNotMatch(report, /All files/);
  assert.doesNotMatch(report, /`pkg\/same\.js`/);
});

test("says so when no file changed size", () => {
  const snapshot = { files: { "pkg/a.js": size("pkg", 2048) } };
  const report = renderReport({ current: snapshot, baseline: snapshot });
  assert.match(report, /No file changed size\./);
  assert.match(report, /\| \*\*Total\*\* \| 2\.0 KiB \|/);
});

test("reports sizes alone without a baseline", () => {
  const report = renderReport({
    current: { files: { "pkg/a.js": size("pkg", 2048) } },
  });
  assert.match(report, /No baseline from `main` was found/);
  assert.match(report, /\| `pkg` \| 2\.0 KiB \| 1\.0 KiB \| 512 B \|/);
  assert.match(report, /<summary>All files \(1\)<\/summary>/);
  assert.doesNotMatch(report, /Changed files/);
  assert.doesNotMatch(report, /\(new\)/);
  assert.doesNotMatch(report, /\(\+/);
});
