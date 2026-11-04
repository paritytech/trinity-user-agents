/**
 * The network a tab runs on.
 *
 * The core, the wallet session and every open product are built for one
 * network, so the choice is made before the core starts and applied by
 * reloading the tab. It is kept per tab, like the tab's wallet, so two tabs can
 * run on different networks without one reload changing the other.
 */

import {
  CHAIN_ROLES,
  type ChainBackend,
  type ChainRole,
} from "./network-config.js";
import { DEFAULT_NETWORK, normalizeNetwork } from "./network-scope.js";

/** One network the compact menu offers, with the id the choice is saved under. */
export interface NetworkChoice {
  /** Stable id, saved as the tab's choice and used to pick the network config. */
  id: string;
  /** What the menu shows. */
  label: string;
}

/** Where a tab's network choice is kept. Per tab, like the tab's wallet. */
export const TAB_NETWORK_KEY = "truapi-web-signing-host:tab-network";

/**
 * The network this tab runs on.
 *
 * The tab's own last choice, or `fallback` when it has none or the saved id is
 * not one of `offered` (a tab from before this choice existed, or one naming a
 * network this build no longer serves). `fallback` is Paseo, so an older tab
 * keeps running on Paseo.
 */
export function chosenNetwork(
  storage: Storage,
  offered: readonly NetworkChoice[],
  fallback: string,
): string {
  const saved = storage.getItem(TAB_NETWORK_KEY);
  return saved !== null && offered.some((choice) => choice.id === saved)
    ? saved
    : fallback;
}

/** Remember the tab's network choice. A reload applies it. */
export function rememberNetwork(storage: Storage, id: string): void {
  storage.setItem(TAB_NETWORK_KEY, id);
}

/** The legacy key of the tab's wallet, kept for the default network. */
const TAB_WALLET_KEY = "truapi-web-signing-host:tab-wallet";
/** The legacy key of the tab's product, kept for the default network. */
const TAB_PRODUCT_KEY = "truapi-web-signing-host:tab-product";

/**
 * The key the tab's wallet is kept under for `network`.
 *
 * The default network keeps the original key, so a tab from before networks
 * were separated restores its wallet unchanged. Another network gets a
 * `network:<id>:` segment, so the wallet a tab had signed in on one network is
 * never restored as the session on another.
 */
export function tabWalletKey(network: string = DEFAULT_NETWORK): string {
  const id = normalizeNetwork(network);
  return id === DEFAULT_NETWORK
    ? TAB_WALLET_KEY
    : `truapi-web-signing-host:network:${id}:tab-wallet`;
}

/**
 * The key the tab's open product is kept under for `network`.
 *
 * Scoped per network for the same reason as {@link tabWalletKey}: a product
 * left open on one network is never reopened on another. The default network
 * keeps the original key.
 */
export function tabProductKey(network: string = DEFAULT_NETWORK): string {
  const id = normalizeNetwork(network);
  return id === DEFAULT_NETWORK
    ? TAB_PRODUCT_KEY
    : `truapi-web-signing-host:network:${id}:tab-product`;
}
/** The chain backends a tab chose for `network`, per tab like its network. */
export function chainBackendsKey(network: string): string {
  return `truapi-web-signing-host:network:${normalizeNetwork(network)}:tab-chain-backends`;
}

const BACKENDS: readonly ChainBackend[] = ["rpc", "light-client"];

/**
 * The backends this tab chose for `network`'s chains, by chain. A chain it
 * did not choose for, or saved under a name this build does not know, is left
 * out, so it keeps the network's default.
 */
export function savedChainBackends(
  storage: Storage,
  network: string,
): Partial<Record<ChainRole, ChainBackend>> {
  let saved: unknown;
  try {
    saved = JSON.parse(storage.getItem(chainBackendsKey(network)) ?? "{}");
  } catch {
    return {};
  }
  if (typeof saved !== "object" || saved === null) return {};
  const record = saved as Record<string, unknown>;
  return Object.fromEntries(
    CHAIN_ROLES.flatMap((role) => {
      const backend = record[role];
      return BACKENDS.includes(backend as ChainBackend)
        ? [[role, backend]]
        : [];
    }),
  );
}

/**
 * Save this tab's chain backends for `network`. Applied when the tab next
 * loads, because the core and every connection are built at boot. An empty
 * choice removes the saved one, so every chain keeps its default.
 */
export function saveChainBackends(
  storage: Storage,
  network: string,
  backends: Partial<Record<ChainRole, ChainBackend>>,
): void {
  const key = chainBackendsKey(network);
  if (Object.keys(backends).length === 0) storage.removeItem(key);
  else storage.setItem(key, JSON.stringify(backends));
}
