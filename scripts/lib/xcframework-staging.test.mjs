import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { run } from "./truapi-host-release.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const stageScript = join(
  repoRoot,
  "ios/truapi-provider/scripts/stage-xcframework.sh",
);

const slices = ["ios-arm64", "ios-arm64-simulator"];

/** The shape rebuild.sh hands to staging, modulemap per slice included. */
function writeXcframework(root) {
  mkdirSync(root, { recursive: true });
  writeFileSync(join(root, "Info.plist"), "<plist></plist>\n");
  for (const slice of slices) {
    const headers = join(root, slice, "Headers");
    mkdirSync(headers, { recursive: true });
    writeFileSync(join(root, slice, "libtruapi_provider.a"), "");
    writeFileSync(join(headers, "truapi_providerFFI.h"), "// generated\n");
    writeFileSync(
      join(headers, "module.modulemap"),
      'module truapi_providerFFI {\n    header "truapi_providerFFI.h"\n}\n',
    );
  }
}

async function withStaging(
  body,
  {
    writeSource = true,
    sourceName = "truapi_provider.xcframework",
    sourceIsDestination = false,
  } = {},
) {
  const workspace = mkdtempSync(join(tmpdir(), "truapi-provider-staging-"));
  try {
    const packageRoot = join(workspace, "package");
    const source = sourceIsDestination
      ? join(packageRoot, "Binaries", sourceName)
      : join(workspace, "target", sourceName);
    if (writeSource) writeXcframework(source);
    mkdirSync(packageRoot, { recursive: true });

    const result = await run("sh", [stageScript], {
      env: {
        ...process.env,
        PROVIDER_PACKAGE_ROOT: packageRoot,
        PROVIDER_XCFRAMEWORK: source,
      },
    });

    await body({
      result,
      staged: join(packageRoot, "Binaries/truapi_provider.xcframework"),
    });
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
}

test("staging strips every per-slice modulemap", async () => {
  await withStaging(({ result, staged }) => {
    assert.equal(result.status, 0, result.stderr);

    const entries = readdirSync(staged, { recursive: true });
    assert.deepEqual(
      entries.filter((entry) => entry.endsWith("module.modulemap")),
      [],
      "a slice-local modulemap collides with the consumer's own",
    );
  });
});

test("staging keeps the headers and the rest of the tree", async () => {
  await withStaging(({ result, staged }) => {
    assert.equal(result.status, 0, result.stderr);

    const entries = readdirSync(staged, { recursive: true });
    assert.ok(entries.includes("Info.plist"), "Info.plist survives");
    for (const slice of slices) {
      assert.ok(
        entries.includes(join(slice, "Headers/truapi_providerFFI.h")),
        `${slice} keeps its generated header`,
      );
      assert.ok(
        entries.includes(join(slice, "libtruapi_provider.a")),
        `${slice} keeps its library`,
      );
    }
  });
});

test("staging names the destination, not the source directory", async () => {
  await withStaging(
    ({ result, staged }) => {
      assert.equal(result.status, 0, result.stderr);

      const entries = readdirSync(staged, { recursive: true });
      assert.ok(
        entries.includes(join(slices[0], "Headers/truapi_providerFFI.h")),
        "stages under the package's own name whatever the source is called",
      );
    },
    { sourceName: "some-other-name.xcframework" },
  );
});

test("staging refuses when the framework has not been built", async () => {
  await withStaging(
    ({ result }) => {
      assert.equal(result.status, 66, result.stderr);
      assert.match(result.stderr, /rebuild\.sh/);
    },
    { writeSource: false },
  );
});

test("staging refuses to stage a framework onto itself", async () => {
  await withStaging(
    ({ result, staged }) => {
      assert.equal(result.status, 64, result.stderr);
      assert.match(result.stderr, /staging destination/);

      const entries = readdirSync(staged, { recursive: true });
      assert.ok(
        entries.includes(join(slices[0], "Headers/truapi_providerFFI.h")),
        "refusing must not delete the framework it was pointed at",
      );
    },
    { sourceIsDestination: true },
  );
});
