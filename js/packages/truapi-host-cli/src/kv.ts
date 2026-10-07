// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Node KeyValueStore implementations backing the core's storage callbacks.
// Lifted from the earlier @dotli/host-node package (feat/host-core lineage),
// which hardened them against racing writes and world-readable files.

import { readFile, writeFile, mkdir, chmod, rm } from "node:fs/promises";
import { dirname } from "node:path";

/** Plain async string KV. Values here are hex-encoded core payloads. */
export interface KeyValueStore {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
  keys(prefix: string): Promise<readonly string[]>;
}

/** Ephemeral, process-lifetime store. Good for CLI one-shots and tests. */
export class InMemoryKeyValueStore implements KeyValueStore {
  private readonly map = new Map<string, string>();

  async get(key: string): Promise<string | null> {
    return this.map.get(key) ?? null;
  }

  async set(key: string, value: string): Promise<void> {
    this.map.set(key, value);
  }

  async delete(key: string): Promise<void> {
    this.map.delete(key);
  }

  async keys(prefix: string): Promise<readonly string[]> {
    return [...this.map.keys()].filter((k) => k.startsWith(prefix));
  }
}

/**
 * Permissions for the store and its directory: owner-only.
 *
 * The core routes a product's `localStorage` straight into this store, and the
 * core-storage file holds the SSO session blob and allowance keys. Default
 * permissions would make that 0644, i.e. readable by every other local user on
 * a shared build machine or CI box.
 */
const STORE_MODE = 0o600;
const STORE_DIR_MODE = 0o700;

/**
 * Persistent store backed by a single JSON file. Loaded once on first access
 * and written back on every mutation. Suitable for a long-lived CLI. For heavy
 * write loads prefer a real embedded database.
 *
 * The file is owner-only (see {@link STORE_MODE}). Note the store is keyed by
 * whatever prefixes the caller uses. The core scopes product storage by
 * product id, NOT by account or session, so a consumer that supports
 * switching identities must clear the file itself on logout, or the next
 * identity inherits the previous one's host storage. The path is available as
 * {@link FileKeyValueStore.filePath} for exactly that.
 */
export class FileKeyValueStore implements KeyValueStore {
  private cache: Record<string, string> | null = null;
  // The IN-FLIGHT load, memoised. Serializing only the write chain was not enough:
  // two concurrent set() calls both found `cache === null`, both issued their own
  // readFile, and the second assignment discarded the first caller's mutation. One
  // key vanished from the file and from memory while BOTH writes resolved
  // successfully. Sharing the read fixes it, since after it settles every caller
  // mutates the same object.
  private loading: Promise<Record<string, string>> | null = null;
  private readonly path: string;
  // Serialize writes so two overlapping flushes can never tear the file.
  private writeChain: Promise<void> = Promise.resolve();

  constructor(path: string) {
    this.path = path;
  }

  /**
   * Where this store persists, so a consumer can clear it on logout. The
   * core scopes entries by product id, not by account, so leaving the file
   * behind hands the next identity the previous one's host storage.
   */
  get filePath(): string {
    return this.path;
  }

  private load(): Promise<Record<string, string>> {
    if (this.cache !== null) {
      return Promise.resolve(this.cache);
    }
    if (this.loading !== null) {
      return this.loading;
    }
    this.loading = (async () => {
      try {
        this.cache = JSON.parse(await readFile(this.path, "utf8")) as Record<
          string,
          string
        >;
      } catch {
        // Missing or unreadable file starts an empty store. A malformed file is
        // treated the same so a corrupt cache never wedges the host.
        this.cache = {};
      } finally {
        this.loading = null;
      }
      return this.cache;
    })();
    return this.loading;
  }

  private flush(): Promise<void> {
    // Chain each write after the previous one. The `.catch` keeps a failed
    // write from poisoning the chain for later writes.
    this.writeChain = this.writeChain
      .catch(() => {})
      .then(async () => {
        await mkdir(dirname(this.path), {
          recursive: true,
          mode: STORE_DIR_MODE,
        });
        await writeFile(this.path, JSON.stringify(this.cache ?? {}), {
          encoding: "utf8",
          mode: STORE_MODE,
        });
        // `mode` on writeFile only applies when the file is CREATED, so a store
        // written before this was tightened would stay 0644 forever. chmod every
        // flush to self-heal those.
        await chmod(this.path, STORE_MODE).catch(() => {
          // Not our file, or a filesystem without POSIX modes. The write itself
          // succeeded, so don't fail the store over its permissions.
        });
      });
    return this.writeChain;
  }

  // Mutations go through `this.cache` AFTER the load settles, never through
  // the object `load()` resolved with. `clear()` swaps the cache object out,
  // so a mutation on the resolved reference could land on a detached object
  // and a successfully-resolved write would silently vanish.
  async get(key: string): Promise<string | null> {
    await this.load();
    return (this.cache ??= {})[key] ?? null;
  }

  async set(key: string, value: string): Promise<void> {
    await this.load();
    (this.cache ??= {})[key] = value;
    await this.flush();
  }

  async delete(key: string): Promise<void> {
    await this.load();
    delete (this.cache ??= {})[key];
    await this.flush();
  }

  async keys(prefix: string): Promise<readonly string[]> {
    await this.load();
    return Object.keys((this.cache ??= {})).filter((k) => k.startsWith(prefix));
  }

  /**
   * Drop every entry and delete the backing file. The next identity to log in
   * must not inherit this one's storage (core product-storage keys carry no
   * account component), so logout calls this.
   */
  async clear(): Promise<void> {
    // Let an in-flight load settle first. Its `this.cache = ...` assignment
    // would otherwise resurrect stale data over the cleared cache.
    if (this.loading !== null) {
      await this.loading;
    }
    this.cache = {};
    // Ride the write chain so an in-flight flush cannot resurrect the file
    // after the removal.
    this.writeChain = this.writeChain
      .catch(() => {})
      .then(() => rm(this.path, { force: true }));
    await this.writeChain;
  }
}
