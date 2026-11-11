import assert from "node:assert/strict";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  detectTarget,
  installEnvironment,
  run,
  startReleaseServer,
  versionReportingArchive,
} from "./truapi-host-release.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const installerSource = readFileSync(
  join(repoRoot, "scripts/truapi-host-installer.sh"),
);

/**
 * The triple the installer must derive on this machine. A wrong derivation
 * asks the fake release server for an asset it does not serve, so these tests
 * pin the uname mapping as well as the install layout.
 */
const target = detectTarget();
const skip = target === undefined && "no prebuilt target for this platform";

/** Run the installer the way the documented one-liner does: piped into bash. */
function runInstaller(environment) {
  return run("bash", ["-s"], {
    input: installerSource,
    env: { ...process.env, ...environment },
  });
}

async function withHome(body) {
  const home = mkdtempSync(join(tmpdir(), "truapi-host-home-"));
  try {
    await body(home);
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
}

/** Serve one published version, run `body`, and always shut the server down. */
async function withRelease(version, options, body) {
  const release = await startReleaseServer();
  release.publish(version, target, versionReportingArchive(version), options);
  try {
    await body(release);
  } finally {
    await release.close();
  }
}

test(
  "installs the advertised version and wires the symlink chain",
  { skip },
  async () => {
    await withRelease("0.10.0", {}, async (release) => {
      await withHome(async (home) => {
        const result = await runInstaller(
          installEnvironment(home, release.baseUrl),
        );
        assert.equal(result.status, 0, result.stderr);

        const root = join(home, "share");
        const installed = join(root, "versions/0.10.0/truapi-host");
        assert.ok(existsSync(installed), "versioned binary is installed");
        assert.equal(readlinkSync(join(root, "current")), "versions/0.10.0");
        assert.equal(
          realpathSync(join(home, "bin/truapi-host")),
          realpathSync(installed),
        );

        // The PATH entry must reach the binary through `current`, so that an
        // update can swap versions without rewriting the user's symlink.
        assert.equal(
          readlinkSync(join(home, "bin/truapi-host")),
          join(root, "current/truapi-host"),
        );

        const executed = await run(join(home, "bin/truapi-host"), []);
        assert.equal(executed.stdout.trim(), "truapi-host 0.10.0");
      });
    });
  },
);

test("refuses an archive whose digest does not match", { skip }, async () => {
  await withRelease("0.10.0", { corruptChecksum: true }, async (release) => {
    await withHome(async (home) => {
      const result = await runInstaller(
        installEnvironment(home, release.baseUrl),
      );
      assert.notEqual(result.status, 0, "installer must fail closed");
      assert.match(result.stderr, /checksum/i);
      assert.ok(
        !existsSync(join(home, "bin/truapi-host")),
        "nothing is installed when verification fails",
      );
    });
  });
});

test(
  "an upgrade repoints current and leaves the PATH symlink alone",
  { skip },
  async () => {
    await withRelease("0.10.0", {}, async (release) => {
      await withHome(async (home) => {
        const environment = installEnvironment(home, release.baseUrl);
        assert.equal((await runInstaller(environment)).status, 0);
        const link = join(home, "bin/truapi-host");
        const linkTarget = readlinkSync(link);

        release.publish("0.10.1", target, versionReportingArchive("0.10.1"));
        const upgrade = await runInstaller(environment);
        assert.equal(upgrade.status, 0, upgrade.stderr);

        const root = join(home, "share");
        assert.equal(readlinkSync(join(root, "current")), "versions/0.10.1");
        assert.equal(readlinkSync(link), linkTarget, "PATH symlink is stable");
        assert.equal((await run(link, [])).stdout.trim(), "truapi-host 0.10.1");
      });
    });
  },
);

test(
  "TRUAPI_HOST_VERSION pins the install and skips the stable pointer",
  { skip },
  async () => {
    await withRelease("0.10.1", {}, async (release) => {
      // The pointer now names 0.10.0 while we ask for 0.10.1: a pinned install
      // must not consult it at all.
      release.publish("0.10.0", target, versionReportingArchive("0.10.0"));
      await withHome(async (home) => {
        const result = await runInstaller({
          ...installEnvironment(home, release.baseUrl),
          TRUAPI_HOST_VERSION: "0.10.1",
        });
        assert.equal(result.status, 0, result.stderr);
        assert.equal(
          readlinkSync(join(home, "share/current")),
          "versions/0.10.1",
        );
      });
    });
  },
);

test(
  "installing clears a cargo-installed copy that would shadow it",
  { skip },
  async () => {
    await withRelease("0.10.0", {}, async (release) => {
      await withHome(async (home) => {
        // No cargo on PATH here, so this exercises the plain-removal fallback.
        const cargoBin = join(home, "cargo/bin");
        mkdirSync(cargoBin, { recursive: true });
        const stale = join(cargoBin, "truapi-host");
        writeFileSync(stale, "#!/bin/sh\necho stale\n");

        const result = await runInstaller(
          installEnvironment(home, release.baseUrl),
        );
        assert.equal(result.status, 0, result.stderr);
        assert.ok(
          !existsSync(stale),
          "a cargo copy must not be left shadowing the install",
        );
      });
    });
  },
);

test(
  "--uninstall removes the install and leaves foreign binaries alone",
  { skip },
  async () => {
    await withRelease("0.10.0", {}, async (release) => {
      await withHome(async (home) => {
        const environment = installEnvironment(home, release.baseUrl);
        assert.equal((await runInstaller(environment)).status, 0);
        const root = join(home, "share");
        assert.ok(existsSync(join(root, "versions/0.10.0/truapi-host")));

        const removal = await run("bash", ["-s", "--", "--uninstall"], {
          input: installerSource,
          env: { ...process.env, ...environment },
        });
        assert.equal(removal.status, 0, removal.stderr);
        assert.ok(
          !existsSync(join(home, "bin/truapi-host")),
          "PATH symlink is gone",
        );
        assert.ok(!existsSync(join(root, "versions")), "version store is gone");
        assert.ok(!existsSync(join(root, "current")), "current link is gone");
      });
    });
  },
);

// An unrelated truapi-host on the PATH is not ours to delete.
test("--uninstall keeps a PATH entry that is not ours", { skip }, async () => {
  await withHome(async (home) => {
    const binDir = join(home, "bin");
    mkdirSync(binDir, { recursive: true });
    const foreign = join(binDir, "truapi-host");
    writeFileSync(foreign, "#!/bin/sh\necho someone elses\n");

    const removal = await run("bash", ["-s", "--", "--uninstall"], {
      input: installerSource,
      env: {
        ...process.env,
        ...installEnvironment(home, "http://unused.invalid"),
      },
    });
    assert.equal(removal.status, 0, removal.stderr);
    assert.ok(existsSync(foreign), "a foreign binary must survive --uninstall");
  });
});

// Neither route can assume the other ever ran.
test("installing succeeds when no cargo copy exists", { skip }, async () => {
  await withRelease("0.10.0", {}, async (release) => {
    await withHome(async (home) => {
      const environment = installEnvironment(home, release.baseUrl);
      assert.ok(!existsSync(join(home, "cargo/bin/truapi-host")));

      const result = await runInstaller(environment);
      assert.equal(result.status, 0, result.stderr);
      assert.ok(existsSync(join(home, "bin/truapi-host")));
    });
  });
});

test(
  "TRUAPI_HOST_PRESERVE_CARGO_INSTALL leaves a cargo copy in place",
  { skip },
  async () => {
    await withRelease("0.10.0", {}, async (release) => {
      await withHome(async (home) => {
        const cargoBin = join(home, "cargo/bin");
        mkdirSync(cargoBin, { recursive: true });
        const existing = join(cargoBin, "truapi-host");
        writeFileSync(existing, "#!/bin/sh\necho stale\n");

        const result = await runInstaller({
          ...installEnvironment(home, release.baseUrl),
          TRUAPI_HOST_PRESERVE_CARGO_INSTALL: "1",
        });
        assert.equal(result.status, 0, result.stderr);
        assert.ok(
          existsSync(existing),
          "a caller that owns only its own PATH must not delete another install",
        );
        assert.ok(existsSync(join(home, "bin/truapi-host")));
      });
    });
  },
);

test("--uninstall succeeds when nothing is installed", { skip }, async () => {
  await withHome(async (home) => {
    const removal = await run("bash", ["-s", "--", "--uninstall"], {
      input: installerSource,
      env: {
        ...process.env,
        ...installEnvironment(home, "http://unused.invalid"),
      },
    });
    assert.equal(removal.status, 0, removal.stderr);
  });
});

test("reports an unsupported platform instead of installing something wrong", async () => {
  await withHome(async (home) => {
    const fakeBin = join(home, "fake");
    mkdirSync(fakeBin, { recursive: true });
    const uname = join(fakeBin, "uname");
    writeFileSync(
      uname,
      '#!/bin/sh\ncase "$1" in -s) echo Plan9;; *) echo pdp11;; esac\n',
    );
    chmodSync(uname, 0o755);

    const result = await runInstaller({
      HOME: home,
      TRUAPI_HOST_INSTALL_DIR: join(home, "share"),
      TRUAPI_HOST_BIN_DIR: join(home, "bin"),
      PATH: `${fakeBin}:${process.env.PATH}`,
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Plan9 pdp11/);
  });
});
