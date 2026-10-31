import type { HexString } from "@parity/truapi/scale";
import { err, ok } from "neverthrow";
import { ProductStorage } from "../../src/chat/worker/truapi-ports.js";

/**
 * What a host keeps of one product's TrUAPI storage: it outlives the
 * product's worker, as the host's does. Each write answers a turn later, as
 * a message to the host does, and fails while `failing` is set.
 */
export class KeptStorage {
  readonly kept = new Map<string, HexString>();
  /** Why every write fails now, or null when writes are kept. */
  failing: string | null = null;

  /** A product worker's storage over what is kept, as a worker that starts reads it. */
  open(keys: string[]): Promise<ProductStorage> {
    const later = () => new Promise((resolve) => setTimeout(resolve, 0));
    const client = {
      localStorage: {
        read: async ({ key }: { key: string }) => {
          const value = this.kept.get(key);
          return ok(value === undefined ? {} : { value });
        },
        write: async ({ key, value }: { key: string; value: HexString }) => {
          await later();
          if (this.failing !== null)
            return err({ tag: "Unknown", value: { reason: this.failing } });
          this.kept.set(key, value);
          return ok(undefined);
        },
        clear: async ({ key }: { key: string }) => {
          await later();
          if (this.failing !== null)
            return err({ tag: "Unknown", value: { reason: this.failing } });
          this.kept.delete(key);
          return ok(undefined);
        },
      },
    };
    return ProductStorage.load(client as never, keys, () => {});
  }
}
