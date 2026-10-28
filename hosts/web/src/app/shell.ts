import { parseAddress, parseTypedAddress } from "./address.js";
import type { NetworkConfig } from "../platform/network-config.js";
import {
  DEFAULT_NETWORK,
  normalizeNetwork,
} from "../platform/network-scope.js";
import { isRecordable } from "./recents.js";

/** The places the host's dock opens, in the order the dock shows them. */
export const DESTINATIONS = ["chat", "pocket", "browse"] as const;
export type Destination = (typeof DESTINATIONS)[number];

/** Destinations that open a product. The others are screens the host draws itself. */
export const PRODUCT_DESTINATIONS = ["browse"] as const;
export type ProductDestination = (typeof PRODUCT_DESTINATIONS)[number];

export function isProductDestination(
  destination: Destination | null,
): destination is ProductDestination {
  return (
    destination !== null &&
    (PRODUCT_DESTINATIONS as readonly string[]).includes(destination)
  );
}

export const DESTINATION_LABELS: Record<Destination, string> = {
  chat: "Chat",
  browse: "Browse",
  pocket: "Pocket",
};

/** The product each product destination opens: a name or URL, or empty for none yet. */
export type DockAddresses = Record<ProductDestination, string>;

/** What a destination opens before the user chooses. Only names known to be published are listed. */
const DEFAULTS: Record<string, Partial<DockAddresses>> = {
  paseo: { browse: "browse.paseo" },
};

export function defaultAddresses(network: string): DockAddresses {
  return {
    browse: "",
    ...DEFAULTS[normalizeNetwork(network)],
  };
}

const KEY = "truapi-web-signing-host:dock:v1";

/** The default network keeps the original key, as the rest of this host's storage does. */
function keyFor(network: string): string {
  const id = normalizeNetwork(network);
  return id === DEFAULT_NETWORK
    ? KEY
    : `truapi-web-signing-host:network:${id}:dock:v1`;
}

/** The addresses saved for a network over its defaults. Anything unreadable gives the defaults. */
export function loadAddresses(
  storage: Pick<Storage, "getItem">,
  network: string,
): DockAddresses {
  const addresses = defaultAddresses(network);
  try {
    const saved: unknown = JSON.parse(
      storage.getItem(keyFor(network)) ?? "null",
    );
    if (typeof saved !== "object" || saved === null) return addresses;
    for (const destination of PRODUCT_DESTINATIONS) {
      const value = (saved as Record<string, unknown>)[destination];
      if (typeof value === "string") addresses[destination] = value;
    }
  } catch {
    // An unreadable value is no choice at all.
  }
  return addresses;
}

/**
 * Remember what `destination` opens. An address that carries credentials or a
 * secret-looking parameter is not kept, as the recents list does not keep it.
 */
export function saveAddress(
  storage: Pick<Storage, "getItem" | "setItem">,
  network: string,
  destination: ProductDestination,
  address: string,
): DockAddresses {
  const addresses = loadAddresses(storage, network);
  if (!isRecordable(address)) return addresses;
  addresses[destination] = address;
  storage.setItem(keyFor(network), JSON.stringify(addresses));
  return addresses;
}

/** What an in-host navigation request turns into. */
export type Navigation =
  | { kind: "product"; address: string }
  | { kind: "external"; url: string }
  | { kind: "refused"; reason: string };

const PRODUCT_SCHEME = /^polkadot(?:app)?:\/\//i;

/**
 * Decide what a product's `navigateTo(url)` does here.
 *
 * A dotNS name of the connected network opens inside the host, whether it is
 * written as `https://name.tld`, `polkadot://name.tld` or
 * `polkadotapp://name.tld`. Any other http or https address goes to the
 * browser. Everything else is refused, so a product cannot make the host open
 * a script or file address.
 */
export function resolveNavigation(
  raw: string,
  network: NetworkConfig,
): Navigation {
  const typed = raw.trim();
  const product = PRODUCT_SCHEME.test(typed);
  const asName = product ? typed.replace(PRODUCT_SCHEME, "polkadot://") : typed;
  try {
    const address = parseAddress(asName, network);
    if (address.kind === "name")
      return {
        kind: "product",
        address: `${address.name}/${address.suffix}`.replace(/\/$/, ""),
      };
    return /^https?:\/\//i.test(typed)
      ? { kind: "external", url: typed }
      : {
          kind: "refused",
          reason: `${typed} is not an address this host opens.`,
        };
  } catch (error) {
    if (!product && /^https?:\/\//i.test(typed))
      return { kind: "external", url: typed };
    return {
      kind: "refused",
      reason: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * The destination whose saved address is the same product as `address`, when
 * `address` names a product with no path of its own. Opening that product a
 * second time would run two copies under one product id, so the dock resumes
 * the one it holds instead.
 */
export function destinationOf(
  address: string,
  addresses: DockAddresses,
  network: NetworkConfig,
): ProductDestination | null {
  let wanted: string | null;
  try {
    const parsed = parseTypedAddress(address, network);
    wanted =
      parsed.kind === "name" && parsed.suffix === "" ? parsed.name : null;
  } catch {
    return null;
  }
  if (wanted === null) return null;
  for (const destination of PRODUCT_DESTINATIONS) {
    try {
      const saved = parseTypedAddress(addresses[destination], network);
      if (saved.kind === "name" && saved.name === wanted) return destination;
    } catch {
      // An address that no longer parses names nothing.
    }
  }
  return null;
}
