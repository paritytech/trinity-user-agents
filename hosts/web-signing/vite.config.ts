import { defineConfig } from "vite";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { sandboxAssets } from "./sandbox-plugin.js";
import type { BuildInfo } from "./src/versions.js";

const hostRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

/** A file's bytes, or null when it is not there. */
function readOrNull(path: string): Buffer | null {
  try {
    return readFileSync(path);
  } catch {
    return null;
  }
}

function sha256(path: string): string | null {
  const bytes = readOrNull(path);
  return bytes === null
    ? null
    : createHash("sha256").update(bytes).digest("hex");
}

function manifestVersion(dir: string): string | null {
  const bytes = readOrNull(join(dir, "package.json"));
  if (bytes === null) return null;
  const { version } = JSON.parse(bytes.toString("utf8")) as {
    version?: unknown;
  };
  return typeof version === "string" ? version : null;
}

/** Where a linked `@parity/*` package really lives, as this host resolves it. */
function linkedPackage(name: string): string {
  return realpathSync(join(hostRoot, "node_modules/@parity", name));
}

function git(...args: string[]): string | null {
  try {
    return execFileSync("git", args, {
      cwd: repoRoot,
      encoding: "utf8",
    }).trim();
  } catch {
    return null;
  }
}

/**
 * What this build is made of, read from the files it will serve. Measured at
 * server start or build, so a WASM bundle rebuilt while the dev server runs
 * shows here only after a restart.
 */
function readBuildInfo(): BuildInfo {
  const truapi = linkedPackage("truapi");
  const truapiHost = linkedPackage("truapi-host");
  const truapiProvider = linkedPackage("truapi-provider");
  const truapiDebugger = linkedPackage("truapi-debugger");
  const core = join(truapiHost, "dist/wasm/testing");
  const status = git("status", "--porcelain");
  return {
    packages: {
      truapi: manifestVersion(truapi),
      truapiHost: manifestVersion(truapiHost),
      truapiProvider: manifestVersion(truapiProvider),
      truapiDebugger: manifestVersion(truapiDebugger),
    },
    core: {
      version: manifestVersion(core),
      sha256: sha256(join(core, "truapi_server_bg.wasm")),
    },
    verifiableSha256: sha256(join(core, "truapi_verifiable_bg.wasm")),
    providerSha256: sha256(
      join(truapiProvider, "dist/truapi_provider_bg.wasm"),
    ),
    source: {
      commit: git("rev-parse", "--short", "HEAD"),
      dirty: status !== null && status !== "",
    },
  };
}

// The worker runtime imports the production core, `./wasm/web/truapi_server.js`,
// which has no signing host in it: a production browser host pairs with a
// wallet and never holds keys. This host holds a development wallet, so its
// worker loads the `testing` core instead, the only bundle built with
// `wasm-signing-host`. The testing server in `@parity/truapi-host` redirects
// the same specifier for the same reason.
const signingCore = {
  find: /^\.\/wasm\/web\/truapi_server\.js$/,
  replacement: "@parity/truapi-host/wasm/testing",
};

/**
 * Host names, besides localhost, the server answers to. Vite refuses any other
 * `Host` header to stop DNS rebinding, so a host reached through a private name
 * lists that name here.
 */
const allowedHosts = (process.env.WEB_SIGNING_ALLOWED_HOSTS ?? "")
  .split(",")
  .map((name) => name.trim())
  .filter((name) => name !== "");

export default defineConfig({
  define: {
    __BUILD_INFO__: JSON.stringify(readBuildInfo()),
  },
  plugins: [sandboxAssets()],
  resolve: { alias: [signingCore] },
  worker: { format: "es" },
  server: {
    port: 5180,
    strictPort: true,
    // The in-tree packages are linked from outside this directory.
    fs: { allow: [repoRoot] },
    ...(allowedHosts.length > 0 ? { allowedHosts } : {}),
  },
  preview: {
    port: 5180,
    strictPort: true,
    ...(allowedHosts.length > 0 ? { allowedHosts } : {}),
  },
  build: { target: "es2022" },
});
