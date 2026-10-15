// Host callbacks for a pairing host with no screen, such as a CLI. The host
// passes what only it can provide, including how the user answers reviews;
// every other capability gets a fixed answer.

import { ok } from "neverthrow";

import type { GenericError, Result } from "@parity/truapi";

import type {
  AuthPresenter,
  ChainProvider,
  CoreStorage,
  HostChainSet,
  Permissions,
  ProductStorage,
  RequiredHostCallbacks,
  UserConfirmation,
} from "../generated/host-callbacks.js";

/** What a headless pairing host must provide itself. */
export interface HeadlessHostCallbacks {
  /** Persists the core's own state: the paired session, keys, allowances. */
  coreStorage: CoreStorage;
  /** Opens a JSON-RPC connection to the chain with a given genesis hash. */
  chain: ChainProvider;
  /** Receives auth state changes, including the pairing deeplink to show. */
  auth: Required<AuthPresenter>;
  /** Backs the product's `localStorage`. */
  productStorage: ProductStorage;
  /** The chains `chain.connect` can reach. */
  supportedChains: HostChainSet;
  /**
   * The user's answer to reviews: `getAccount` asks to disclose the product
   * subtree, `signRaw` to sign. A CLI asks at its prompt.
   */
  userConfirmation: Required<UserConfirmation>;
  /** The user's answer to permissions, such as `ChainSubmit` before signing. */
  permissions: Permissions;
}

const unsupported = (capability: string) =>
  new Error(`${capability} is not available on a headless host`);

/**
 * A stream that emits `value` once and then stays open until the consumer
 * closes it, for state that never changes on a headless host.
 */
function constant<T>(value: T): AsyncIterable<Result<T, GenericError>> {
  return {
    [Symbol.asyncIterator]() {
      let emitted = false;
      let closed = false;
      let close: (() => void) | undefined;
      const done = { value: undefined, done: true } as const;
      return {
        next() {
          if (!emitted) {
            emitted = true;
            return Promise.resolve({
              value: ok<T, GenericError>(value),
              done: false,
            });
          }
          if (closed) return Promise.resolve(done);
          return new Promise((resolve) => {
            close = () => resolve(done);
          });
        },
        return() {
          closed = true;
          close?.();
          return Promise.resolve(done);
        },
      };
    },
  };
}

/** Fill in every capability a headless pairing host does not own. */
export function headlessHostCallbacks(
  host: HeadlessHostCallbacks,
): RequiredHostCallbacks {
  const supportedGenesis = new Set(
    host.supportedChains.chains.map((chain) => chain.genesisHash.toLowerCase()),
  );
  let nextOperationId = 0;

  return {
    coreStorage: host.coreStorage,
    chain: host.chain,
    auth: host.auth,
    productStorage: host.productStorage,
    userConfirmation: host.userConfirmation,
    permissions: host.permissions,
    features: {
      async featureSupported(request) {
        return {
          supported: supportedGenesis.has(
            request.value.genesisHash.toLowerCase(),
          ),
        };
      },
      async supportedChains() {
        return host.supportedChains;
      },
    },
    navigation: {
      async navigateTo() {
        throw unsupported("Navigation");
      },
    },
    notifications: {
      async pushNotification() {
        throw unsupported("Push notifications");
      },
      async cancelNotification() {},
    },
    productOperations: {
      async beginOperation() {
        return { id: nextOperationId++ };
      },
      async endOperation() {},
    },
    theme: {
      subscribeTheme() {
        return constant({ name: { tag: "Default" }, variant: "Dark" });
      },
    },
    locale: {
      subscribeLocale() {
        return constant({
          languageTag: Intl.DateTimeFormat().resolvedOptions().locale,
        });
      },
    },
    preimage: {
      lookupPreimage() {
        return constant<Uint8Array | undefined>(undefined);
      },
    },
  };
}
