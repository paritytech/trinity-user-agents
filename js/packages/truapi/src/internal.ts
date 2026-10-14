import { Err, Ok, ResultAsync } from "neverthrow";

export {
  createHostConnection,
  type HostConnection,
} from "./host-connection.js";

export {
  createInternalClient,
  type InternalTrUApiClient,
} from "./generated/internal-client.js";

/** Protects result handling when host authorization shares the product's realm. */
export function freezeInternalResults(): void {
  for (const constructor of [Err, Ok, ResultAsync]) {
    Object.freeze(constructor.prototype);
    Object.freeze(constructor);
  }
}
