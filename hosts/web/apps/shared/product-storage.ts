import type { TrUApiClient } from "@parity/truapi";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { utf8 } from "../../bridges/primitives/codec.js";

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : JSON.stringify(error);

type StorageClient = Pick<TrUApiClient, "localStorage">;

/**
 * What a chat engine needs of the product's TrUAPI storage. `flush` resolves
 * once every write so far is kept, and rejects when one was not.
 */
export type RecordStorage = Pick<Storage, "getItem" | "setItem"> & {
  flush(): Promise<void>;
};

/**
 * A `Storage` the engine reads and writes at once, kept in the product's
 * TrUAPI storage. {@link ProductStorage.load} reads the keys first; each
 * write goes to the host in order, and {@link ProductStorage.flush} waits for
 * them and rejects while the last write of any key failed. The host keeps the
 * value per product, so it outlives the worker.
 */
export class ProductStorage implements Storage {
  private readonly items = new Map<string, string>();
  private writes: Promise<void> = Promise.resolve();
  /** Keys whose last write failed, with why: the host does not hold their value. */
  private readonly failed = new Map<string, string>();

  private constructor(
    private readonly client: StorageClient,
    private readonly onError: (reason: string) => void,
  ) {}

  static async load(
    client: StorageClient,
    keys: string[],
    onError: (reason: string) => void,
  ): Promise<ProductStorage> {
    const storage = new ProductStorage(client, onError);
    for (const key of keys) {
      const read = await client.localStorage.read({ key });
      if (read.isErr())
        throw new Error(
          `The worker's storage could not be read: ${errorText(read.error)}`,
        );
      const value = read.value.value;
      if (value !== undefined)
        storage.items.set(key, new TextDecoder().decode(hexToBytes(value)));
    }
    return storage;
  }

  get length(): number {
    return this.items.size;
  }

  key(index: number): string | null {
    return [...this.items.keys()][index] ?? null;
  }

  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    // The same value again needs no write, unless the last write of it failed.
    if (this.items.get(key) === value && !this.failed.has(key)) return;
    this.items.set(key, value);
    this.queue(key, async () => {
      const written = await this.client.localStorage.write({
        key,
        value: bytesToHex(utf8(value)),
      });
      return written.isErr() ? errorText(written.error) : null;
    });
  }

  removeItem(key: string): void {
    if (!this.items.delete(key)) return;
    this.queue(key, async () => {
      const cleared = await this.client.localStorage.clear({ key });
      return cleared.isErr() ? errorText(cleared.error) : null;
    });
  }

  clear(): void {
    for (const key of [...this.items.keys()]) this.removeItem(key);
  }

  /**
   * Resolves once every write so far reached the host and the host holds the
   * last value of every key; rejects while the last write of a key failed.
   */
  async flush(): Promise<void> {
    await this.writes;
    const [failure] = this.failed.values();
    if (failure !== undefined)
      throw new Error(`The worker's storage did not keep a write: ${failure}`);
  }

  /** Run `write` after every earlier one; it answers why it failed, or null. */
  private queue(key: string, write: () => Promise<string | null>): void {
    this.writes = this.writes.then(async () => {
      let failure: string | null;
      try {
        failure = await write();
      } catch (error) {
        failure = errorText(error);
      }
      if (failure === null) this.failed.delete(key);
      else {
        this.failed.set(key, failure);
        this.onError(failure);
      }
    });
  }
}
