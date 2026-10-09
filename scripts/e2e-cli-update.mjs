#!/usr/bin/env node
/**
 * End-to-end check of the truapi-host install and self-update chain.
 *
 * Serves a fake release over HTTP, installs the real packaged binary with the
 * real installer, then drives that binary through `truapi-host update`.
 * Nothing here contacts GitHub.
 *
 *   make cli-dist          # produce the archive this consumes
 *   make e2e-cli-update
 */
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  detectTarget,
  installEnvironment,
  run,
  startReleaseServer,
  stubArchive,
  versionReportingArchive,
} from "./lib/truapi-host-release.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const installer = join(repoRoot, "scripts/truapi-host-installer.sh");
const target = process.env.CLI_TARGET || detectTarget();
const version = readFileSync(
  join(repoRoot, "rust/crates/truapi-host-cli/Cargo.toml"),
  "utf8",
).match(/^version = "(.*)"$/m)[1];
const nextVersion = `${version}-next`;

const failures = [];

function check(label, actual, expected) {
  if (Object.is(actual, expected)) {
    console.log(`  ok    ${label}`);
    return;
  }
  failures.push(label);
  console.log(`  FAIL  ${label}`);
  console.log(`          expected: ${JSON.stringify(expected)}`);
  console.log(`          actual:   ${JSON.stringify(actual)}`);
}

async function main() {
  if (target === undefined) {
    console.error("no prebuilt target for this platform");
    return 2;
  }
  const packaged = join(
    repoRoot,
    `target/dist/truapi-host-${version}-${target}.tar.gz`,
  );
  if (!existsSync(packaged)) {
    console.error(`missing ${packaged}`);
    console.error(`run: make cli-dist CLI_TARGET=${target}`);
    return 2;
  }

  const release = await startReleaseServer();
  release.publish(version, target, readFileSync(packaged));
  let secondHome;

  const home = mkdtempSync(join(tmpdir(), "truapi-host-e2e-"));
  const root = join(home, "share");
  const entrypoint = join(home, "bin/truapi-host");
  const environment = {
    ...process.env,
    ...installEnvironment(home, release.baseUrl),
  };

  try {
    console.log("1. install the packaged binary through the real installer");
    const installed = await run("bash", [installer], { env: environment });
    check("installer exits cleanly", installed.status, 0);
    if (installed.status !== 0) {
      console.error(installed.stderr);
      return 1;
    }
    check(
      "current points at the installed version",
      readlinkSync(join(root, "current")),
      `versions/${version}`,
    );
    check(
      "the PATH entry resolves through current",
      readlinkSync(entrypoint),
      join(root, "current/truapi-host"),
    );
    check(
      "the installed binary reports its version",
      (
        await run(entrypoint, ["--version"], { env: environment })
      ).stdout.trim(),
      `truapi-host ${version}`,
    );
    check(
      "the product-script runner ships beside the binary",
      existsSync(join(root, `versions/${version}/runner.js`)),
      true,
    );
    check(
      "the matching product-script types ship beside the runner",
      readFileSync(join(root, `versions/${version}/script-types.d.ts`), "utf8"),
      readFileSync(
        join(repoRoot, "rust/crates/truapi-host-cli/js/script-types.d.ts"),
        "utf8",
      ),
    );
    check(
      "the dev container ships beside the runner",
      existsSync(join(root, `versions/${version}/sandbox-assets/container.js`)),
      true,
    );
    // The checkout's runner imports @parity/truapi by relative path. Running the
    // packaged one from an unrelated directory proves the client is bundled in,
    // so a downloaded install needs no source tree: it reaches its own env check
    // instead of failing to resolve a module.
    const runner = await run(
      "bun",
      ["run", join(root, `versions/${version}/runner.js`)],
      { env: environment, cwd: tmpdir() },
    );
    check(
      "the packaged runner resolves without a checkout",
      /TRUAPI_FRAME_URL must be set/.test(runner.stdout + runner.stderr),
      true,
    );
    const script = join(home, "script.ts");
    writeFileSync(
      script,
      'assert(typeof Bun !== "undefined"); assert(typeof process !== "undefined"); console.log("installed runner reached");\n',
    );
    const scriptRun = await run(
      entrypoint,
      [
        "pairing-host",
        "--script",
        script,
        "--base-path",
        join(home, "state"),
        "--auto-accept",
      ],
      {
        env: { ...environment, TRUAPI_HOST_NO_UPDATE: "1" },
        cwd: tmpdir(),
      },
    );
    check(
      "the installed entrypoint runs a product script",
      scriptRun.status,
      0,
    );
    check(
      "the product script was reached through the bundled runner",
      (scriptRun.stdout + scriptRun.stderr).includes(
        "installed runner reached",
      ),
      true,
    );

    console.log("2. a managed install with nothing newer published");
    check(
      "update reports up to date",
      (await run(entrypoint, ["update"], { env: environment })).stdout.trim(),
      `truapi-host ${version} is up to date.`,
    );

    console.log("3. a tampered archive is refused");
    release.publish("9.9.9", target, stubArchive("#!/bin/sh\necho pwned\n"), {
      corruptChecksum: true,
    });
    const tampered = await run(entrypoint, ["update"], { env: environment });
    check("update fails closed", tampered.status !== 0, true);
    check(
      "the failure names the checksum",
      /checksum/.test(tampered.stderr),
      true,
    );
    check(
      "the active version is unchanged",
      readlinkSync(join(root, "current")),
      `versions/${version}`,
    );

    console.log("4. updates can be turned off");
    check(
      "TRUAPI_HOST_NO_UPDATE is honoured",
      (
        await run(entrypoint, ["update"], {
          env: { ...environment, TRUAPI_HOST_NO_UPDATE: "1" },
        })
      ).stdout.trim(),
      "Updates are disabled by TRUAPI_HOST_NO_UPDATE.",
    );

    console.log("5. publish a newer version and self-update into it");
    // A stand-in script rather than a second release build, so the content
    // swap is observable. It replaces the real CLI, so nothing after this
    // step can exercise the updater again.
    release.publish(nextVersion, target, versionReportingArchive(nextVersion));
    const linkBefore = readlinkSync(entrypoint);
    const updated = await run(entrypoint, ["update"], { env: environment });
    check(
      "update reports the install",
      updated.stdout.trim(),
      `Installed truapi-host ${nextVersion}; restart to use it.`,
    );
    check(
      "current moved to the new version",
      readlinkSync(join(root, "current")),
      `versions/${nextVersion}`,
    );
    check("the PATH entry is untouched", readlinkSync(entrypoint), linkBefore);
    check(
      "the next run executes the new version",
      (await run(entrypoint, [], { env: environment })).stdout.trim(),
      `truapi-host ${nextVersion}`,
    );
    check(
      "the version it was updated from is still on disk",
      existsSync(join(root, `versions/${version}/truapi-host`)),
      true,
    );

    console.log("6. an unmanaged binary refuses to update itself");
    const sourceBuild = join(repoRoot, `target/${target}/release/truapi-host`);
    const unmanaged = await run(sourceBuild, ["update"], { env: environment });
    check("unmanaged update fails", unmanaged.status !== 0, true);
    check(
      "the failure explains why",
      unmanaged.stderr.includes("was not installed by the installer"),
      true,
    );
    check(
      "the managed install was left alone",
      readlinkSync(join(root, "current")),
      `versions/${nextVersion}`,
    );
    // A local build never updates, so it has to say so rather than look
    // identical to a managed install that is silently up to date.
    const local = await run(
      sourceBuild,
      ["identity-check", "--mnemonic", "bogus"],
      { env: environment },
    );
    check(
      "a local build identifies itself",
      /\(local build\), not auto-updating/.test(local.stderr),
      true,
    );
    check(
      "a local build shows how to get the prebuilt release",
      local.stderr.includes(
        "curl -fsSL https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh | bash",
      ),
      true,
    );

    console.log("7. a short command finishes the update it started");
    // A fresh install, because the checks above have already recorded a check
    // timestamp and would throttle this one.
    release.setPointer(version);
    secondHome = mkdtempSync(join(tmpdir(), "truapi-host-e2e-"));
    const secondEnvironment = {
      ...process.env,
      ...installEnvironment(secondHome, release.baseUrl),
    };
    await run("bash", [installer], { env: secondEnvironment });
    const secondRoot = join(secondHome, "share");

    const shortVersion = `${version}-short`;
    release.publish(
      shortVersion,
      target,
      versionReportingArchive(shortVersion),
    );
    // Fails almost immediately, so the process only stays alive long enough to
    // finish the download if it deliberately waits for it.
    await run(
      join(secondRoot, "current/truapi-host"),
      ["identity-check", "--mnemonic", "bogus"],
      { env: secondEnvironment },
    );
    check(
      "the download completed before the command exited",
      readlinkSync(join(secondRoot, "current")),
      `versions/${shortVersion}`,
    );
  } finally {
    await release.close();
    rmSync(home, { recursive: true, force: true });
    if (secondHome) rmSync(secondHome, { recursive: true, force: true });
  }

  console.log();
  if (failures.length > 0) {
    console.log(`${failures.length} check(s) failed: ${failures.join(", ")}`);
    return 1;
  }
  console.log("all checks passed");
  return 0;
}

process.exit(await main());
