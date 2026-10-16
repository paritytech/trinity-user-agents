// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

// The typed host callbacks, implemented for a terminal host: every required
// group plus the optional permission-status probe. The optional `chat`,
// `contacts` and `pocket` groups are deliberately absent, since a terminal
// host holds no chat lists, contacts or cards. Without `contacts` the core
// answers a contact pick `Unsupported`, and a transaction naming contact
// handles fails `UnknownContact` rather than being signed unresolved. The
// generated adapter (`createWasmRawCallbacks`) turns these into the raw SCALE
// surface the wasm core calls. Hand-written SCALE callbacks are exactly the
// drift this package exists to avoid.

import { ok } from "neverthrow";
import type {
  AuthState,
  CoreStorageKey,
  PermissionDecision,
  RequiredHostCallbacks,
} from "@parity/truapi-host";
import type { HexString } from "@parity/truapi";
import type { ChainEndpoints, ChainPool } from "./chain-pool.js";
import { fromHex, toHex } from "./hex.js";
import type { KeyValueStore } from "./kv.js";
import type { HostPresenter } from "./presenter.js";
import { describeReview, type ConfirmRequest } from "./reviews.js";

export interface HostCallbackDeps {
  coreStore: KeyValueStore;
  productStore: KeyValueStore;
  pool: ChainPool;
  presenter: HostPresenter;
  endpoints: ChainEndpoints;
  /** Environment id advertised through the core's supported-chains set. */
  network: string;
  /**
   * Awaited before every product-storage operation. The host uses it to hold
   * reads back while an identity switch is still clearing the previous
   * identity's data, so a fast product can never observe stale entries.
   */
  productStorageGate?: () => Promise<void>;
  theme: "Dark" | "Light";
  /** BCP 47 language tag served through the `locale` subscription. */
  locale: string;
  /** Optional preimage retrieval backend (P2P/IPFS). Default: always a miss. */
  lookupPreimage?: (key: Uint8Array) => Promise<Uint8Array | undefined>;
  onAuthState: (state: AuthState) => void;
  log?: (line: string) => void;
}

/**
 * Flatten a typed core-storage slot to a stable, legible backing key. Slot
 * tags are unique. The parameterized slots carry their parameters.
 */
export function coreSlot(key: CoreStorageKey): string {
  switch (key.tag) {
    case "AllowanceKeys":
      return `AllowanceKeys:${key.value.sessionId}`;
    case "PermissionAuthorization":
      return `PermissionAuthorization:${key.value.productId}:${key.value.request.tag}`;
    case "AutoSigningKey":
      return `AutoSigningKey:${key.value.productId}`;
    case "RingVrfRegistry":
      return `RingVrfRegistry:${toHex(key.value.rootPublicKey)}`;
    case "ProductSubtree":
      return `ProductSubtree:${key.value.sessionId}:${key.value.productId}`;
    case "ProductManifest":
      return `ProductManifest:${key.value.productId}`;
    case "SsoResponderRequestLedger":
      return `SsoResponderRequestLedger:${toHex(key.value.rootPublicKey)}:${toHex(key.value.peerStatementAccountId)}:${toHex(key.value.peerEncryptionPublicKey)}`;
    default:
      // Parameterless slots flatten to their tag. A NEW parameterized slot
      // landing here would collide across its parameters, so new engine
      // versions must be checked against this switch.
      return key.tag;
  }
}

const park = (): Promise<never> => new Promise<never>(() => {});

export function createHostCallbacks(
  deps: HostCallbackDeps,
): RequiredHostCallbacks {
  const {
    coreStore,
    productStore,
    pool,
    presenter,
    endpoints,
    network,
    theme,
    locale,
    lookupPreimage,
    onAuthState,
    productStorageGate,
    log,
  } = deps;
  let nextNotificationId = 1;
  // Operations a product has begun and not yet ended, per product id. In a
  // browser these references keep the product's worker alive. This host runs
  // the core in-process, so nothing needs keeping alive and the bookkeeping
  // exists to honour the contract: ids unique among ONE product's open
  // operations, reusable afterwards, and ends idempotent.
  const openOperations = new Map<string, Set<number>>();
  const nextOperationId = new Map<string, number>();
  // The wire type is a plain number, so wrap well inside u32 rather than
  // growing without bound in a long-lived CLI.
  const OPERATION_ID_LIMIT = 0xffff_ffff;
  // Live `subscribeStorage` streams, per product-storage key. The core is
  // the only writer through this host, so the write and clear callbacks are
  // a complete change feed: no filesystem watching is needed.
  const storageWatchers = new Map<
    string,
    Set<(value: string | undefined) => void>
  >();
  const notifyStorage = (key: string, value: string | undefined): void => {
    for (const listener of [...(storageWatchers.get(key) ?? [])]) {
      listener(value);
    }
  };

  /**
   * Ask for consent that carries a lifetime. Presenters may answer with a
   * duration; one that only answers yes/no gets its yes read as the lasting
   * grant, which is what the boolean permission callbacks recorded before
   * the core could express "once".
   */
  const askPermission = async (
    request: ConfirmRequest,
  ): Promise<PermissionDecision> => {
    if (presenter.confirmPermission !== undefined) {
      return presenter.confirmPermission(request);
    }
    return (await presenter.confirm(request)) ? "AllowAlways" : "Deny";
  };

  return {
    navigation: {
      // NOT the pairing affordance. Pairing arrives as `AuthState.Pairing`.
      // This is "open a URL in the system browser", which a terminal host
      // hands to the user instead of guessing at an opener.
      async navigateTo(url) {
        presenter.openUrl(url);
      },
    },

    notifications: {
      async pushNotification(notification) {
        presenter.notify(
          notification.deeplink === undefined
            ? notification.text
            : `${notification.text} (${notification.deeplink})`,
        );
        return { id: nextNotificationId++ };
      },
      async cancelNotification(id) {
        // Notifications are printed, not retained. Cancelling is idempotently
        // a no-op by contract.
        log?.(`cancelNotification(${String(id)})`);
      },
    },

    permissions: {
      devicePermission(product, request) {
        return askPermission({
          title: `Allow access to: ${request}`,
          details: [`product: ${product.productId}`],
          phoneVerifies: false,
        });
      },
      remotePermission(product, request) {
        return askPermission({
          title: "Grant a product permission",
          details: [
            `product: ${product.productId}`,
            `permission: ${JSON.stringify(request.permission)}`,
          ],
          phoneVerifies: false,
        });
      },
    },

    features: {
      async featureSupported(request) {
        // The only feature probe today is per-chain support. This host serves
        // exactly the chains it has endpoints for.
        const supported =
          request.tag === "Chain" &&
          endpoints[request.value.genesisHash] !== undefined;
        return { supported };
      },
      async supportedChains() {
        // Advertise the role-mapped subset of the endpoint map. Both
        // advertisements answer from the same map, so they can never
        // disagree with `featureSupported` or with `chain.connect`.
        return {
          network,
          chains: Object.entries(endpoints).flatMap(
            ([genesisHash, endpoint]) =>
              endpoint.role === undefined
                ? []
                : [
                    {
                      identifier: endpoint.role,
                      // Endpoint keys are documented `0x`-prefixed hex.
                      genesisHash: genesisHash as `0x${string}`,
                    },
                  ],
          ),
        };
      },
    },

    productStorage: {
      // The core namespaces these keys itself
      // (`truapi:product-storage:v1:<len>:<productId>:<key>`). The key carries
      // NO account component, which is why the host clears this store on
      // logout (see CliHost).
      async read(key) {
        await productStorageGate?.();
        const hit = await productStore.get(key);
        return hit === null ? undefined : fromHex(hit);
      },
      async write(key, value) {
        await productStorageGate?.();
        const hex = toHex(value);
        await productStore.set(key, hex);
        notifyStorage(key, hex);
      },
      async clear(key) {
        await productStorageGate?.();
        await productStore.delete(key);
        notifyStorage(key, undefined);
      },
      async *subscribeStorage(key) {
        const queue: (string | undefined)[] = [];
        let wake: (() => void) | null = null;
        const listener = (value: string | undefined): void => {
          queue.push(value);
          wake?.();
          wake = null;
        };
        const listeners = storageWatchers.get(key) ?? new Set();
        storageWatchers.set(key, listeners);
        listeners.add(listener);
        try {
          await productStorageGate?.();
          const current = await productStore.get(key);
          // The store holds exactly the hex the write callback put there.
          yield ok({ value: (current ?? undefined) as HexString | undefined });
          for (;;) {
            while (queue.length > 0) {
              const next = queue.shift();
              yield ok({ value: next as HexString | undefined });
            }
            await new Promise<void>((resolve) => {
              wake = resolve;
            });
          }
        } finally {
          listeners.delete(listener);
          if (listeners.size === 0) {
            storageWatchers.delete(key);
          }
        }
      },
    },

    coreStorage: {
      async readCoreStorage(key) {
        const hit = await coreStore.get(coreSlot(key));
        return hit === null ? undefined : fromHex(hit);
      },
      async writeCoreStorage(key, value) {
        await coreStore.set(coreSlot(key), toHex(value));
      },
      async clearCoreStorage(key) {
        await coreStore.delete(coreSlot(key));
      },
    },

    chain: {
      connect(genesisHash) {
        return pool.connect(genesisHash);
      },
    },

    auth: {
      authStateChanged(state) {
        onAuthState(state);
      },
    },

    userConfirmation: {
      confirmUserAction(review) {
        return presenter.confirm(describeReview(review, { endpoints }));
      },
      // Identity and account disclosures whose consent has a lifetime. Same
      // prompt content as the one-shot path, different answer shape.
      confirmPermission(review) {
        return askPermission(describeReview(review, { endpoints }));
      },
    },

    theme: {
      async *subscribeTheme() {
        yield ok({ name: { tag: "Default" as const }, variant: theme });
        // A terminal theme never changes mid-run. Park forever so the core's
        // subscription stays open instead of seeing an immediate
        // end-of-stream.
        await park();
      },
    },

    locale: {
      async *subscribeLocale() {
        yield ok({ languageTag: locale });
        // Same contract as `theme`: emit once, then keep the stream open.
        await park();
      },
    },

    permissionStatus: {
      async devicePermissionStatus() {
        // Status probe only, must not prompt. A terminal process has no OS
        // device-permission model to report on, so every capability is
        // NotApplicable here. Actual grants still go through the prompting
        // `permissions` group above.
        return "NotApplicable";
      },
    },

    productOperations: {
      async beginOperation(product, label) {
        const open = openOperations.get(product.productId) ?? new Set<number>();
        openOperations.set(product.productId, open);
        let id = nextOperationId.get(product.productId) ?? 1;
        // Skip ids this product still has open, so a wrap cannot hand out a
        // live id. The loop terminates because a product cannot hold
        // OPERATION_ID_LIMIT operations at once.
        while (open.has(id)) {
          id = id >= OPERATION_ID_LIMIT ? 1 : id + 1;
        }
        open.add(id);
        nextOperationId.set(
          product.productId,
          id >= OPERATION_ID_LIMIT ? 1 : id + 1,
        );
        log?.(
          `beginOperation(${product.productId}, ${label === "" ? "unlabelled" : label}) -> ${String(id)}`,
        );
        return { id };
      },
      async endOperation(product, id) {
        // Idempotent by contract: an unknown or already-ended id is a no-op,
        // so a retry after an ambiguous failure is safe.
        openOperations.get(product.productId)?.delete(id);
      },
    },

    preimage: {
      async *lookupPreimage(key) {
        let value: Uint8Array | undefined;
        try {
          value = await lookupPreimage?.(key);
        } catch (error) {
          log?.(
            `lookupPreimage(${toHex(key).slice(0, 18)}…) failed: ${String(error)}`,
          );
          value = undefined;
        }
        yield ok(value);
        // Same contract as `theme`: emit once, then keep the stream open.
        await park();
      },
    },
  };
}
