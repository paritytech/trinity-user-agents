// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Integration: the REAL wasm core, booted in-process, driven by the real
// product client over the loopback wire. No network, no phone:
//
//   - a product localStorage round-trip exercises the full frame path
//     (client -> transport -> loopback -> Rust -> storage callbacks -> back).
//   - `requestLogin` exercises the pairing presentation headless. The core
//     emits `AuthState.Pairing{deeplink}` before opening any socket, so a
//     host with NO endpoints still gets a QR to render (Gate A's shape).
//   - logout clearing pins the product-storage-is-not-account-scoped rule.

import { describe, it, expect, beforeAll, afterAll } from "bun:test";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createClient, createTransport } from "@parity/truapi";
import type { AuthState } from "@parity/truapi-host";

import { createCliHost, type CliHost } from "../src/host.js";
import type { ConfirmRequest } from "../src/reviews.js";
import type { HostPresenter } from "../src/presenter.js";

/** A presenter with scripted decisions and a full recording. */
function createScriptedPresenter(decide: (request: ConfirmRequest) => boolean) {
  const states: AuthState[] = [];
  const confirms: ConfirmRequest[] = [];
  const notifications: string[] = [];
  const presenter: HostPresenter = {
    authStateChanged(state) {
      states.push(state);
    },
    async confirm(request) {
      confirms.push(request);
      return decide(request);
    },
    notify(text) {
      notifications.push(text);
    },
    openUrl() {},
    dispose() {},
  };
  return { presenter, states, confirms, notifications };
}

describe("createCliHost against the real wasm core", () => {
  let dir: string;
  let host: CliHost;
  let scripted: ReturnType<typeof createScriptedPresenter>;

  beforeAll(async () => {
    dir = await mkdtemp(join(tmpdir(), "host-cli-int-"));
    scripted = createScriptedPresenter(() => false);
    host = await createCliHost({
      host: { name: "host-cli tests", version: "0.0.0-test" },
      pairing: { deeplinkScheme: "polkadotapp" },
      // Unreachable genesis hashes: the test must stay offline. featureSupported
      // and chain.connect both answer from this map.
      people: { genesisHash: `0x${"11".repeat(32)}` },
      bulletin: { genesisHash: `0x${"22".repeat(32)}` },
      assetHub: { genesisHash: `0x${"33".repeat(32)}` },
      chains: {},
      network: "host-cli-test",
      storageDir: dir,
      presenter: scripted.presenter,
      logLevel: "off",
    });
  });

  afterAll(async () => {
    host.dispose();
    await rm(dir, { recursive: true, force: true });
  });

  it("As a product, I write and read localStorage and the value round-trips through the Rust core", async () => {
    // Given
    const product = host.createProduct({ productId: "host-cli-test.dot" });
    const client = createClient(createTransport(product.provider));

    // When
    const written = await client.localStorage.write({
      key: "cache",
      value: "0xdeadbeef",
    });
    expect(written.isOk()).toBe(true);

    const read = await client.localStorage.read({ key: "cache" });

    // Then
    expect(read.isOk()).toBe(true);
    expect(JSON.stringify(read.isOk() ? read.value : null)).toContain(
      "deadbeef",
    );

    // The backing file exists, is owner-only, and carries the core's own
    // product-storage namespace (product id, NO account component). That
    // missing account component is why the host clears it on logout.
    const mode = (await stat(host.storagePaths.product)).mode & 0o777;
    expect(mode.toString(8)).toBe("600");
    const persisted = JSON.parse(
      await readFile(host.storagePaths.product, "utf8"),
    ) as Record<string, string>;
    const keys = Object.keys(persisted);
    expect(keys.length).toBeGreaterThan(0);
    for (const key of keys) {
      expect(key).toMatch(/^truapi:product-storage:v1:.*host-cli-test\.dot/);
    }

    product.dispose();
  });

  it("As a host embedder, I request a login offline and receive a Pairing deeplink to render headless", async () => {
    // Given
    const product = host.createProduct({ productId: "host-cli-test.dot" });
    const client = createClient(createTransport(product.provider));

    // When
    // Do NOT await: requestLogin resolves only at a terminal state. The
    // pairing presentation is the host's job and arrives via AuthState.
    const login = client.account
      .requestLogin({ reason: "host-cli integration test" })
      .then(
        () => undefined,
        () => undefined,
      );

    // Then
    const pairing = await host.waitForAuthState(
      (state) => state.tag === "Pairing",
      15_000,
    );
    if (pairing.tag !== "Pairing") {
      throw new Error("unreachable");
    }
    expect(pairing.value.deeplink).toMatch(/^polkadotapp:\/\//);
    expect(host.authState()?.tag).toBe("Pairing");

    // The scripted presenter saw the same state. That is the QR hook.
    expect(scripted.states.some((state) => state.tag === "Pairing")).toBe(true);

    host.cancelPairing();
    await host.waitForAuthState(
      (state) => state.tag === "Disconnected" || state.tag === "LoginFailed",
      15_000,
    );
    await login;
    product.dispose();
  });

  it("As a product, I subscribe to a storage key and see its current value and then each change", async () => {
    // Given
    const product = host.createProduct({ productId: "host-cli-test.dot" });
    const client = createClient(createTransport(product.provider));
    const seen: (string | undefined)[] = [];
    const waitFor = async (count: number): Promise<void> => {
      for (let i = 0; i < 200 && seen.length < count; i++) {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    };

    // When
    const subscription = client.localStorage
      .subscribe({ request: { key: "watched" } })
      .subscribe({
        next(item) {
          seen.push(item.value);
        },
      });
    await waitFor(1);
    await client.localStorage.write({ key: "watched", value: "0xabcd" });
    await waitFor(2);

    // Then
    // The key is unset when the stream opens, so the first item carries no
    // value; the write is reported as a change.
    expect(seen[0]).toBeUndefined();
    expect(seen[1]).toContain("abcd");

    subscription.unsubscribe();
    product.dispose();
  });

  it("As a CLI user, I log out and the host clears product storage", async () => {
    // Given
    const product = host.createProduct({ productId: "host-cli-test.dot" });
    const client = createClient(createTransport(product.provider));
    await client.localStorage.write({ key: "leftover", value: "0x01" });
    expect(existsSync(host.storagePaths.product)).toBe(true);

    // When
    await host.disconnectSession();

    // Then
    expect(existsSync(host.storagePaths.product)).toBe(false);
    // Core storage (the host's own slots) survives. Only the product side is
    // identity-tainted.
    product.dispose();
  });
});
