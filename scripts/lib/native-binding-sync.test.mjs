import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

test("host binding sync removes obsolete namespaces without deleting hand-written sources", () => {
  const workspace = mkdtempSync(join(tmpdir(), "truapi-binding-sync-"));
  const scriptPath = "ios/truapi-host/scripts/sync-bindings.sh";
  const script = join(workspace, scriptPath);
  const sources = join(workspace, "ios/truapi-host/Sources");
  const hostSources = join(sources, "TrUAPIHost");
  const generated = join(workspace, "target/uniffi-swift-out");
  try {
    mkdirSync(dirname(script), { recursive: true });
    symlinkSync(join(repoRoot, scriptPath), script);
    mkdirSync(hostSources, { recursive: true });
    mkdirSync(generated, { recursive: true });
    writeFileSync(join(hostSources, "TrUAPIHost.swift"), "// hand-written host\n");
    mkdirSync(join(hostSources, "Resources"));
    writeFileSync(
      join(hostSources, "Resources/truapi-container.js"),
      "// container\n",
    );
    for (const namespace of ["truapi", "truapi_server", "truapi_platform"]) {
      writeFileSync(join(generated, `${namespace}.swift`), "// generated\n");
      writeFileSync(join(generated, `${namespace}FFI.h`), "// header\n");
      writeFileSync(
        join(generated, `${namespace}FFI.modulemap`),
        `module ${namespace}FFI {}\n`,
      );
      const headers = join(sources, `${namespace}FFI/include`);
      mkdirSync(headers, { recursive: true });
      writeFileSync(join(headers, `${namespace}FFI.h`), "// stale header\n");
      writeFileSync(join(headers, "module.modulemap"), "// stale modulemap\n");
      writeFileSync(join(hostSources, `${namespace}.swift`), "// stale binding\n");
    }

    execFileSync("sh", [script], { cwd: workspace });

    assert.deepEqual(readdirSync(sources).sort(), ["TrUAPIHost", "truapiFFI"]);
    assert.deepEqual(readdirSync(hostSources).sort(), [
      "Resources",
      "TrUAPIHost.swift",
      "truapi.swift",
    ]);
    assert.equal(
      readFileSync(join(hostSources, "TrUAPIHost.swift"), "utf8"),
      "// hand-written host\n",
    );
    assert.equal(
      readFileSync(join(hostSources, "Resources/truapi-container.js"), "utf8"),
      "// container\n",
    );
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
});
