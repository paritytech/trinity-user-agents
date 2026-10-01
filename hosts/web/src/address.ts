import { PASEO_DOTNS } from "./dotns.js";
import { parseProductUrl, productIdFor } from "./product.js";

const HAS_SCHEME = /^[a-z][a-z0-9+.-]*:\/\//i;

/**
 * The URL text to parse for what was typed in the address bar.
 *
 * Text without a scheme gets `http://`, so `localhost:3000` and a LAN address
 * work as typed. Anything with another scheme is left alone for
 * `parseProductUrl` to refuse.
 */
export function normalizeAddress(raw: string): string {
  const typed = raw.trim();
  return typed === "" || HAS_SCHEME.test(typed) ? typed : `http://${typed}`;
}

/**
 * Every TLD the core treats as a dotNS name, one per network. It mirrors
 * `DOTNS_TLDS` in `rust/crates/truapi/src/platform.rs`, and a test compares
 * the two. A host recognises all of them so that a name on another network
 * gets an explanation and is not opened as a web address.
 */
export const KNOWN_DOTNS_TLDS = ["dot", "paseo", "testnet"];

/** What the address bar asks to open. */
export type Address =
  | { kind: "url"; url: URL }
  | {
      kind: "name";
      /** The dotNS name, lower case, such as `myapp.paseo`. */
      name: string;
      /** Path, query and hash typed after the name, without a leading `/`. */
      suffix: string;
    };

/** The public gateway suffixes known to serve the browser shell and refuse framing. */
const SHELL_GATEWAY = `.${PASEO_DOTNS.tld}.li`;

/** One DNS-style label: lower case letters, digits and inner hyphens. */
const LABEL = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/;

/**
 * Decide whether the address bar holds a dotNS product name or a web address,
 * and reject what neither can open, saying what to type instead.
 *
 * A dotNS name has no public DNS record, so `myapp.paseo` and
 * `http://myapp.paseo/` both mean the name. Plain URLs, `localhost` and LAN
 * addresses keep their meaning.
 */
export function parseAddress(raw: string): Address {
  const typed = raw.trim();
  const named = typed.replace(/^polkadot:\/\//i, "http://");
  const url = parseProductUrl(normalizeAddress(named));
  const host = url.hostname.toLowerCase();

  if (host.endsWith(SHELL_GATEWAY)) {
    const name = host.slice(0, -".li".length);
    throw new Error(
      `${host} is the public gateway page. It serves the Polkadot browser shell and refuses to be embedded. Type ${name} instead.`,
    );
  }

  const tld = host.split(".").at(-1) ?? "";
  if (!host.includes(".") || !KNOWN_DOTNS_TLDS.includes(tld)) {
    if (/^polkadot:/i.test(typed))
      throw new Error(
        "A polkadot:// address must be a dotNS name such as myapp.paseo.",
      );
    return { kind: "url", url };
  }

  if (url.port !== "" || url.username !== "" || url.password !== "")
    throw new Error(
      `A dotNS name takes no port or credentials. Type ${host} on its own.`,
    );
  if (tld !== PASEO_DOTNS.tld)
    throw new Error(
      `.${tld} names belong to another network. This host is connected to Paseo and opens .${PASEO_DOTNS.tld} names.`,
    );
  const labels = host.split(".");
  if (labels.length !== 2 || !LABEL.test(labels[0]) || labels[0].length > 63)
    throw new Error(
      `${host} is not a product name. A name is one label and the TLD, for example myapp.${PASEO_DOTNS.tld}, using letters, digits and hyphens.`,
    );
  return {
    kind: "name",
    name: host,
    suffix: `${url.pathname.replace(/^\//, "")}${url.search}${url.hash}`,
  };
}

/** The product id a navigation will use, and where it came from. */
export interface EffectiveProductId {
  id: string;
  source: "entered" | "derived";
  /**
   * Whether the core is likely to accept the id. A bare IP address derives an
   * id the core refuses, so it needs one entered. The core decides for real
   * when the product is opened.
   */
  usable: boolean;
}

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;

/**
 * The id `url` opens under when `override` is entered, else the one derived
 * from the address.
 *
 * The override is the user's choice of which product the core takes the page
 * for. It follows the address bar until it is cleared.
 */
export function effectiveProductId(
  url: URL,
  override: string,
): EffectiveProductId {
  const entered = override.trim();
  if (entered !== "") return { id: entered, source: "entered", usable: true };
  const id = productIdFor(url, "");
  const bareAddress = IPV4.test(url.hostname) || url.hostname.includes(":");
  return { id, source: "derived", usable: !bareAddress };
}

/**
 * The id `address` opens under. A name is its own product id unless an id is
 * entered, and is always one the core accepts.
 */
export function effectiveProductIdFor(
  address: Address,
  override: string,
): EffectiveProductId {
  if (address.kind === "url") return effectiveProductId(address.url, override);
  const entered = override.trim();
  return entered === ""
    ? { id: address.name, source: "derived", usable: true }
    : { id: entered, source: "entered", usable: true };
}

const BARE_LABEL = /^([a-z0-9](?:[a-z0-9-]*[a-z0-9])?)(?=$|[/?#])/i;

/**
 * Give a bare product label its network TLD, so `myapp` means `myapp.paseo`.
 *
 * Only text that is a single label, alone or followed by a path, query or
 * hash, is completed. Anything with a dot, a port, a scheme, credentials or
 * brackets keeps its meaning, as do `localhost` and all-digit labels. Typing
 * the dot is how someone says the text is complete as written. This is applied
 * when the address is opened or read, never to what is in the field.
 */
export function completeAddress(raw: string): string {
  const typed = raw.trim();
  const body = typed.replace(/^polkadot:\/\//i, "");
  const label = BARE_LABEL.exec(body)?.[1];
  if (
    label === undefined ||
    label.length > 63 ||
    label.toLowerCase() === "localhost" ||
    /^\d+$/.test(label)
  )
    return typed;
  return `${label.toLowerCase()}.${PASEO_DOTNS.tld}${body.slice(label.length)}`;
}

/** Parse what was typed, completing a bare product label first. */
export function parseTypedAddress(raw: string): Address {
  return parseAddress(completeAddress(raw));
}

/**
 * The TLD shown dimmed after a bare label while it is typed, or an empty
 * string when none applies: once a dot, path or scheme is typed the text means
 * what it says.
 */
export function implicitSuffix(raw: string): string {
  const typed = raw.trim();
  const completed = completeAddress(typed);
  return completed !== typed &&
    completed === `${typed.toLowerCase()}.${PASEO_DOTNS.tld}`
    ? `.${PASEO_DOTNS.tld}`
    : "";
}

/**
 * How an opened address is written in the field: a name on this network
 * without its TLD, which the field shows dimmed. Left whole when dropping the
 * TLD would not read back as the same address.
 */
export function displayAddress(address: string): string {
  const suffix = `.${PASEO_DOTNS.tld}`;
  const label = /^([a-z0-9](?:[a-z0-9-]*[a-z0-9])?)\./.exec(address)?.[1];
  if (label === undefined || !address.slice(label.length).startsWith(suffix))
    return address;
  const after = address[label.length + suffix.length];
  if (after !== undefined && !"/?#".includes(after)) return address;
  const bare = `${label}${address.slice(label.length + suffix.length)}`;
  return completeAddress(bare) === address ? bare : address;
}
