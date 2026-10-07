// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// The core routes a product's localStorage into this store, so its contents
// are per-user secrets by default. These tests pin the permissions, because
// the failure is silent: a 0644 file works perfectly and is simply readable
// by every other local user.

import { describe, it, expect, afterEach } from "bun:test";
import {
  mkdtemp,
  stat,
  chmod,
  rm,
  readFile,
  writeFile,
} from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { FileKeyValueStore } from "../src/kv.js";

const dirs: string[] = [];

async function storeIn(...segments: string[]) {
  const dir = await mkdtemp(join(tmpdir(), "host-cli-kv-"));
  dirs.push(dir);
  const path = join(dir, ...segments);
  return { path, store: new FileKeyValueStore(path) };
}

const modeOf = async (path: string): Promise<string> =>
  ((await stat(path)).mode & 0o777).toString(8);

afterEach(async () => {
  await Promise.all(
    dirs.splice(0).map((d) => rm(d, { recursive: true, force: true })),
  );
});

describe("FileKeyValueStore", () => {
  it("As a CLI user, my host storage file is owner-only", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");

    // When
    await store.set("dotli:myapp.dot:token", "secret");

    // Then
    expect(await modeOf(path)).toBe("600");
  });

  it("As a CLI user, a missing parent directory is created owner-only", async () => {
    // Given
    const { path, store } = await storeIn("nested", "kv.json");

    // When
    await store.set("k", "v");

    // Then
    expect(await modeOf(join(path, ".."))).toBe("700");
  });

  it("As a CLI user, I upgrade from a build that left the store world-readable and the next write tightens it", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");
    await store.set("k", "v");
    await chmod(path, 0o644);

    // When
    // Any later write must repair it: `mode` on writeFile only applies at
    // creation, so without an explicit chmod the file would stay 0644 forever.
    await store.set("k2", "v2");

    // Then
    expect(await modeOf(path)).toBe("600");
  });

  it("As a product, I write keys and can read, list by prefix, and delete them with the file kept in sync", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");

    // When
    await store.set("dotli:a:one", "1");
    await store.set("dotli:b:two", "2");

    // Then
    expect(await store.get("dotli:a:one")).toBe("1");
    expect(await store.keys("dotli:a:")).toEqual(["dotli:a:one"]);
    await store.delete("dotli:a:one");
    expect(await store.get("dotli:a:one")).toBeNull();
    // Persisted, not just cached.
    expect(JSON.parse(await readFile(path, "utf8"))).toEqual({
      "dotli:b:two": "2",
    });
  });

  // Regression: serializing only the write chain left the LOAD unguarded, so
  // two concurrent set() calls each read the file and the second cache
  // assignment discarded the first caller's key. Both writes resolved, yet
  // one key vanished.
  it("As a product, no key is lost when two writes race", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");
    // Seed the FILE, not the instance: the race only exists while the lazy
    // cache is still unpopulated, so a prior set() through the same store
    // would hide it.
    await writeFile(path, JSON.stringify({ seed: "1" }), "utf8");

    // When
    await Promise.all([store.set("a", "A"), store.set("b", "B")]);

    // Then
    expect(await store.get("a")).toBe("A");
    expect(await store.get("b")).toBe("B");
    expect(JSON.parse(await readFile(path, "utf8"))).toEqual({
      seed: "1",
      a: "A",
      b: "B",
    });
  });

  it("As a product, no key is lost when a write races a delete", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");
    await writeFile(path, JSON.stringify({ keep: "1", gone: "2" }), "utf8");

    // When
    await Promise.all([store.set("added", "3"), store.delete("gone")]);

    // Then
    expect(await store.get("added")).toBe("3");
    expect(await store.get("gone")).toBeNull();
    expect(await store.get("keep")).toBe("1");
  });

  it("As a host embedder, the store exposes its file path so I can clear it on logout", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");

    // Then
    // The core keys product entries by product id, not by account, so a
    // consumer that switches identities must be able to find and delete this
    // file.
    expect(store.filePath).toBe(path);
  });

  it("As a host embedder, I clear the store and both the file and the cached values are gone", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");
    await store.set("a", "A");

    // When
    await store.clear();

    // Then
    expect(existsSync(path)).toBe(false);
    expect(await store.get("a")).toBeNull();
  });

  it("As a host embedder, no pre-clear data survives a clear() racing a write", async () => {
    // Given
    const { path, store } = await storeIn("kv.json");
    await store.set("a", "A");

    // When
    // Which side wins is inherently racy. The invariant is that pre-clear
    // data is gone from memory AND disk, and memory agrees with disk.
    await Promise.all([store.set("b", "B"), store.clear()]);

    // Then
    expect(await store.get("a")).toBeNull();
    const inMemoryB = await store.get("b");
    if (existsSync(path)) {
      const persisted = JSON.parse(await readFile(path, "utf8")) as Record<
        string,
        string | undefined
      >;
      expect(persisted.a).toBeUndefined();
      expect(persisted.b ?? null).toBe(inMemoryB);
    } else {
      expect(inMemoryB).toBeNull();
    }
  });
});
