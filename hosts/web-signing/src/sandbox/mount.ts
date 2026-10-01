import { sha256 } from "@noble/hashes/sha2.js";

/** Where a mounted product's files are served, under the host's base path. */
export const PRODUCT_DIR = "product/";
/** The host's own static files for mounted products: the loader page and script. Never the product's. */
export const SANDBOX_DIR = "truapi-sandbox/";

/** The product a frame is opened for, and the wallet session it belongs to. */
export interface Mount {
  walletId: string;
  productId: string;
  cid: string;
}

const LABEL_LIMIT = 63;
const READABLE_PART = 40;

function hashHex(text: string): string {
  return [...sha256(new TextEncoder().encode(text)).slice(0, 6)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * A path segment for a product id. It reads like the id and always carries a
 * hash of it, so two ids never share a segment whatever characters they differ in.
 */
export function productLabel(productId: string): string {
  const readable = productId
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, READABLE_PART)
    .replace(/-+$/, "");
  const hash = hashHex(productId);
  const label = readable === "" ? `p-${hash}` : `${readable}-${hash}`;
  return label.slice(0, LABEL_LIMIT);
}

/** A path segment for a wallet id: a hash, so the id itself is not spelled out in URLs. */
export function walletKey(walletId: string): string {
  return hashHex(walletId);
}

/**
 * The path of the host page's directory, from the build's base. It works for
 * an absolute base such as `/repo/` and for `./`, and always ends in `/`.
 */
export function hostBase(baseUrl: string, pageUrl: string): string {
  const { pathname } = new URL(baseUrl, pageUrl);
  return pathname.endsWith("/") ? pathname : `${pathname}/`;
}

/**
 * The path a mounted product is served under, and the scope of the service
 * worker that serves it.
 *
 * The wallet, the product and the content all take part, so the same product
 * opened by two wallets, or at two versions, in two tabs is two mounts. One
 * tab's worker never answers another tab's page. The same wallet opening the
 * same content again lands on the same mount.
 */
export function mountScope(base: string, mount: Mount): string {
  return `${base}${PRODUCT_DIR}${walletKey(mount.walletId)}/${productLabel(mount.productId)}/${mount.cid}/`;
}

const SCOPE = /^[0-9a-f]{12}\/[a-z0-9-]+\/([A-Za-z0-9]+)\/$/;

/** The content a scope path names, or null for a path that is no mount. */
export function parseMountScope(
  base: string,
  scopePath: string,
): { cid: string } | null {
  const prefix = `${base}${PRODUCT_DIR}`;
  if (!scopePath.startsWith(prefix)) return null;
  const match = SCOPE.exec(scopePath.slice(prefix.length));
  return match ? { cid: match[1] } : null;
}

/**
 * The address the product frame opens for a name: the loader, a static page of
 * this host, which fetches and checks the content, starts the worker for
 * `scope` and then moves the frame into the product.
 */
export function loaderUrl(options: {
  base: string;
  origin: string;
  scope: string;
  gateway: string;
  kind: "car" | "site";
  /** Path, query and hash the product opens at, starting with `/`. */
  start: string;
}): URL {
  const url = new URL(
    `${options.base}${SANDBOX_DIR}index.html`,
    options.origin,
  );
  url.searchParams.set("scope", options.scope);
  url.searchParams.set("gateway", options.gateway);
  url.searchParams.set("kind", options.kind);
  url.searchParams.set("start", options.start);
  return url;
}

/**
 * Refuse to open a product archive from a page that is not a secure context.
 *
 * Mounting needs a service worker, and a frame is a secure context only when
 * every page above it is too.
 */
export function requireSecureHost(
  isSecureContext: boolean,
  hostOrigin: string,
): void {
  if (isSecureContext) return;
  throw new Error(
    `Names need a secure page, and this one is at ${hostOrigin}. A frame is secure only if every page above it is. Use https, or http://localhost:<port>.`,
  );
}
