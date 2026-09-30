import { sha256 } from "@noble/hashes/sha2.js";
import type { PortLedger } from "./ports.js";
import {
  PORT_RANGE,
  type SandboxPolicy,
  authorityOf,
  identifySandbox,
  portRangeOf,
} from "./policy.js";

/** The setting that names where per-product origins live, for a host not on loopback. */
export interface SandboxOriginSettings {
  /**
   * Either a URL with `{label}` in its host, such as
   * `https://{label}.sandbox.example`, or one with a port range in place of
   * the port, such as `https://host.example:{9450-9459}`. The range form is for
   * a host reached by one name, where only the port can tell products apart.
   */
  template: string;
  /** Who owns which port, for the range form. */
  ports: PortLedger;
}

const MAX_PORTS = 100;

const LABEL_LIMIT = 63;
const READABLE_PART = 40;

/**
 * The DNS label a product's sandbox origin uses. It reads like the product id
 * and always carries a hash of it, so two ids never share a label and so never
 * share an origin, whatever characters they differ in.
 */
export function productLabel(productId: string): string {
  const readable = productId
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, READABLE_PART)
    .replace(/-+$/, "");
  const hash = [...sha256(new TextEncoder().encode(productId)).slice(0, 6)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  const label = readable === "" ? `p-${hash}` : `${readable}-${hash}`;
  return label.slice(0, LABEL_LIMIT);
}

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]"]);

function isLoopback(hostname: string): boolean {
  return LOOPBACK.has(hostname) || hostname.endsWith(".localhost");
}

/**
 * Whether a product opened by name may run without the container.
 *
 * Without it a product has `document.cookie` and the Cookie Store API, and
 * cookies are keyed by host name, not port. Ports of one name, and labels under
 * one parent domain, share a jar, so a product could set cookies its neighbours
 * and the wallet host's name read. Only a `.localhost` label is a host name of
 * its own, and Chrome refuses a `Domain` that widens it. Anything else needs
 * separate registrable host names, which this host cannot yet verify.
 */
export function containerOffAllowed(
  hostLocation: Pick<Location, "hostname">,
  template: string,
): boolean {
  return template.trim() === "" && isLoopback(hostLocation.hostname);
}

/**
 * The origin a product's archive is served from.
 *
 * Every product gets its own, and it is never the host's. That separation is
 * what keeps a product away from the wallets stored under the host's origin,
 * so no setting turns it off. On a loopback host a `.localhost` label is enough
 * and needs no setup. A host reached any other way needs `settings.template`,
 * because no wildcard name is known to resolve.
 */
export async function sandboxOrigin(
  hostLocation: Pick<Location, "protocol" | "hostname" | "port" | "origin">,
  productId: string,
  settings: SandboxOriginSettings,
): Promise<string> {
  const label = productLabel(productId);
  let origin: string;
  const template = settings.template.trim();
  if (template !== "") {
    origin = portRangeOf(template)
      ? await originFromPortRange(template, label, hostLocation, settings.ports)
      : originFromTemplate(template, label);
  } else if (isLoopback(hostLocation.hostname)) {
    const port = hostLocation.port === "" ? "" : `:${hostLocation.port}`;
    origin = `${hostLocation.protocol}//${label}.localhost${port}`;
  } else {
    throw new Error(
      `No sandbox origin for ${hostLocation.origin}. Use localhost, or start the server with WEB_SIGNING_SANDBOX_ORIGIN set.`,
    );
  }
  if (origin === hostLocation.origin)
    throw new Error(
      "The sandbox origin is this host's own origin, which could read its wallets.",
    );
  return origin;
}

function checkedRange(template: string): { first: number; last: number } {
  const range = portRangeOf(template);
  if (
    range === null ||
    !(range.first >= 1 && range.last <= 65535 && range.first <= range.last)
  )
    throw new Error("The port range must be within 1-65535, low to high.");
  if (range.last - range.first + 1 > MAX_PORTS)
    throw new Error(`The port range may hold at most ${MAX_PORTS} ports.`);
  return range;
}

/**
 * The origin on the port `label` owns in the template's range. The range may
 * not include the host's own port, since that is the wallets' origin.
 */
async function originFromPortRange(
  template: string,
  label: string,
  hostLocation: Pick<Location, "protocol" | "hostname" | "port" | "origin">,
  ports: PortLedger,
): Promise<string> {
  const { first, last } = checkedRange(template);
  const hostPort =
    Number(hostLocation.port) ||
    (hostLocation.protocol === "https:" ? 443 : 80);
  const inRange = originFromPortTemplate(template, first);
  if (
    inRange.protocol === hostLocation.protocol &&
    inRange.hostname === hostLocation.hostname &&
    hostPort >= first &&
    hostPort <= last
  )
    throw new Error(
      "The port range includes this host's own port, which holds the wallets.",
    );
  return originFromPortTemplate(
    template,
    await ports.portFor(label, first, last),
  ).origin;
}

function originFromPortTemplate(template: string, port: number): URL {
  let url: URL;
  try {
    url = new URL(template.replace(PORT_RANGE, `:${port}`));
  } catch {
    throw new Error(`The sandbox origin ${template} is not a URL.`);
  }
  if (url.protocol !== "http:" && url.protocol !== "https:")
    throw new Error("The sandbox origin must be http or https.");
  if (authorityOf(template).includes("@"))
    throw new Error("The sandbox origin takes no credentials.");
  if (url.pathname !== "/" || url.search !== "" || url.hash !== "")
    throw new Error(
      "The sandbox origin is an origin: no path, query or fragment.",
    );
  return url;
}

/** Fill `template` for `label`, refusing one that cannot give each product its own origin. */
export function originFromTemplate(template: string, label: string): string {
  let url: URL;
  try {
    url = new URL(template.replace("{label}", label));
  } catch {
    throw new Error(`The sandbox origin ${template} is not a URL.`);
  }
  if (url.protocol !== "http:" && url.protocol !== "https:")
    throw new Error("The sandbox origin must be http or https.");
  const authority = /^https?:\/\/([^/?#]*)/i.exec(template)?.[1] ?? "";
  if (!authority.includes("{label}") || authority.includes("@"))
    throw new Error(
      "The sandbox origin must put {label} in its host name, so each product gets its own origin.",
    );
  if (url.pathname !== "/" || url.search !== "" || url.hash !== "")
    throw new Error(
      "The sandbox origin is an origin: no path, query or fragment.",
    );
  return url.origin;
}

/**
 * The policy an operator configured, checked whole. A mistake stops the server
 * here rather than leaving a boundary that looks set and is not.
 */
export function parseSandboxPolicy(
  template: string,
  hostOrigins: readonly string[],
): SandboxPolicy {
  const trimmed = template.trim();
  if (trimmed !== "") {
    if (portRangeOf(trimmed))
      originFromPortTemplate(trimmed, checkedRange(trimmed).first);
    else originFromTemplate(trimmed, "probe-000000000000");
  }
  const policy: SandboxPolicy = { template: trimmed, hostOrigins: [] };
  for (const raw of hostOrigins) {
    let url: URL;
    try {
      url = new URL(raw);
    } catch {
      throw new Error(`The host origin ${raw} is not a URL.`);
    }
    if (
      (url.protocol !== "http:" && url.protocol !== "https:") ||
      url.pathname !== "/" ||
      url.search !== "" ||
      url.hash !== "" ||
      url.username !== "" ||
      url.password !== ""
    )
      throw new Error(`The host origin ${raw} is not an http(s) origin.`);
    if (identifySandbox(policy, url) !== null)
      throw new Error(
        `The host origin ${raw} is also a product origin, which would let a product embed the loader as the host.`,
      );
    if (!policy.hostOrigins.includes(url.origin))
      policy.hostOrigins.push(url.origin);
  }
  return policy;
}

/**
 * Refuse to open a product archive from a page that is not a secure context.
 *
 * The sandbox needs a service worker, and a frame is a secure context only when
 * every page above it is too. So it is this page that must be secure, whatever
 * origin the product gets. Plain http over a LAN address is not, and neither is
 * a `.localhost` product inside such a page.
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

/** The URL the product frame opens for `cid`, which loads and serves the archive. */
export function sandboxUrl(options: {
  origin: string;
  cid: string;
  gateway: string;
  hostOrigin: string;
  kind: "car" | "site";
  /** Path, query and hash the product opens at, starting with `/`. */
  start: string;
  /** False only for a developer relaxation. */
  container: boolean;
  /**
   * The product's label. The loader records it on the origin at first use and
   * refuses the origin to any other product, so a reassigned port cannot hand
   * one product another's retained data.
   */
  owner: string;
}): URL {
  const url = new URL("/__sandbox/index.html", options.origin);
  url.searchParams.set("cid", options.cid);
  url.searchParams.set("gateway", options.gateway);
  url.searchParams.set("host", options.hostOrigin);
  url.searchParams.set("kind", options.kind);
  url.searchParams.set("start", options.start);
  url.searchParams.set("owner", options.owner);
  if (!options.container) url.searchParams.set("container", "off");
  return url;
}
