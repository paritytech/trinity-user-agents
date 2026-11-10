import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(
  new URL("../sync-release-versions.mjs", import.meta.url),
);
const trinityStackCrate = "rust/crates/trinity-stack/Cargo.toml";

// A tree with @parity/truapi at 1.4.0 and @parity/trinity-stack at 0.3.0,
// where only the trinity-stack crate version is set by the caller.
function fixture(context, { trinityStackCrateVersion }) {
  const root = mkdtempSync(join(tmpdir(), "sync-release-versions-"));
  context.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (path, contents) => {
    const target = join(root, path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(
      target,
      typeof contents === "string" ? contents : JSON.stringify(contents),
    );
  };
  const crate = (name, version) =>
    `[package]\nname = "${name}"\nversion = "${version}"\nedition = "2024"\n`;
  const truapiRange = { "@parity/truapi": "^1.4.0" };

  mkdirSync(join(root, "scripts"));
  copyFileSync(script, join(root, "scripts/sync-release-versions.mjs"));
  write("js/packages/truapi/package.json", { version: "1.4.0" });
  write("js/packages/truapi-host/package.json", {
    version: "1.4.0",
    dependencies: truapiRange,
  });
  write("js/packages/truapi-debugger/package.json", {
    version: "0.1.0",
    dependencies: truapiRange,
  });
  write("js/packages/trinity-stack/package.json", { version: "0.3.0" });
  write("rust/crates/truapi/Cargo.toml", crate("truapi", "1.4.0"));
  write(
    "rust/crates/truapi-host-cli/Cargo.toml",
    crate("truapi-host-cli", "1.4.0"),
  );
  write(
    trinityStackCrate,
    crate("trinity-stack", trinityStackCrateVersion),
  );
  write("package-lock.json", {
    packages: {
      "js/packages/truapi": { version: "1.4.0" },
      "js/packages/truapi-host": {
        version: "1.4.0",
        dependencies: truapiRange,
      },
      "js/packages/truapi-debugger": {
        version: "0.1.0",
        dependencies: truapiRange,
      },
    },
  });
  write("js/container/package-lock.json", {
    packages: { "../packages/truapi": { version: "1.4.0" } },
  });

  const run = (...args) =>
    spawnSync(
      process.execPath,
      [join(root, "scripts/sync-release-versions.mjs"), ...args],
      { encoding: "utf8" },
    );
  const crateVersion = (path) =>
    readFileSync(join(root, path), "utf8").match(/^version = "(.*)"$/m)[1];
  return { run, crateVersion };
}

test("sync sets the trinity-stack crate version from @parity/trinity-stack", (context) => {
  const tree = fixture(context, { trinityStackCrateVersion: "0.2.0" });
  assert.equal(tree.run().status, 0);
  assert.equal(tree.crateVersion(trinityStackCrate), "0.3.0");
  assert.equal(tree.crateVersion("rust/crates/truapi/Cargo.toml"), "1.4.0");
  assert.equal(tree.run("--check").status, 0);
});

test("a trinity-stack crate version that differs from its npm package fails the release check", (context) => {
  const tree = fixture(context, { trinityStackCrateVersion: "1.4.0" });
  const check = tree.run("--check");
  assert.equal(check.status, 1);
  assert.match(
    check.stderr,
    /rust\/crates\/trinity-stack\/Cargo\.toml is 1\.4\.0; expected 0\.3\.0/,
  );
});
