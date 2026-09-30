/**
 * Which origins are product origins, and which pages may embed the loader.
 *
 * The policy is chosen by whoever runs the server and compiled into the loader
 * and the worker. Nothing in a URL takes part in it: a link can name any host,
 * owner or gateway, so none of those says who is asking.
 */
export interface SandboxPolicy {
  /**
   * Where product origins live: a URL with `{label}` in its host, one with a
   * port range in place of the port, or empty for `<label>.localhost` beside a
   * loopback host.
   */
  template: string;
  /**
   * The origins allowed to embed the loader. Empty derives them from the
   * template, which the label form cannot do.
   */
  hostOrigins: string[];
}

/** The parts of an origin the policy reads. A server that cannot see the scheme omits it. */
export interface OriginParts {
  protocol?: string;
  hostname: string;
  /** Empty for the scheme's default port. */
  port: string;
}

export const PORT_RANGE = /:\{(\d{1,5})-(\d{1,5})\}$/;

/** `productLabel` always ends in a hash, so a wallet host name never has this shape. */
const PRODUCT_LABEL = /^[a-z0-9-]+-[0-9a-f]{12}$/;
const LOCALHOST_LABEL = /^([a-z0-9-]+-[0-9a-f]{12})\.localhost$/;
/** `[::1]` is left out: a CSP source list cannot name an IPv6 address, so a frame policy could not admit it. */
const LOOPBACK_HOSTS = ["localhost", "127.0.0.1"];

/** The authority of `template`: everything between `//` and the first `/`, `?` or `#`. */
export function authorityOf(template: string): string {
  return /^https?:\/\/([^/?#]*)/i.exec(template)?.[1] ?? "";
}

/** The port range a template names, or null for a label template. */
export function portRangeOf(
  template: string,
): { first: number; last: number } | null {
  const range = PORT_RANGE.exec(authorityOf(template));
  return range ? { first: Number(range[1]), last: Number(range[2]) } : null;
}

function defaultPort(protocol: string): number {
  return protocol === "https:" ? 443 : 80;
}

function portOf(parts: OriginParts, fallbackProtocol: string): number {
  return parts.port === ""
    ? defaultPort(parts.protocol ?? fallbackProtocol)
    : Number(parts.port);
}

/**
 * The label a product origin carries, or null for an origin that is not one.
 * A range origin has no label to read, so it gives an empty string.
 */
export function identifySandbox(
  policy: SandboxPolicy,
  parts: OriginParts,
): { label: string } | null {
  const hostname = parts.hostname.toLowerCase();
  const template = policy.template.trim();
  if (template === "") {
    if (parts.protocol !== undefined && !/^https?:$/.test(parts.protocol))
      return null;
    const match = LOCALHOST_LABEL.exec(hostname);
    return match ? { label: match[1] } : null;
  }
  const range = portRangeOf(template);
  let url: URL;
  try {
    url = new URL(
      range
        ? template.replace(PORT_RANGE, `:${range.first}`)
        : template.replace("{label}", "x"),
    );
  } catch {
    return null;
  }
  if (parts.protocol !== undefined && parts.protocol !== url.protocol)
    return null;
  const port = portOf(parts, url.protocol);
  if (range) {
    const inRange = port >= range.first && port <= range.last;
    return hostname === url.hostname.toLowerCase() && inRange
      ? { label: "" }
      : null;
  }
  const templatePort = url.port === "" ? defaultPort(url.protocol) : +url.port;
  if (port !== templatePort) return null;
  const templateHost = authorityOf(template).replace(/:\d+$/, "").toLowerCase();
  const pieces = templateHost.split("{label}");
  if (pieces.length !== 2) return null;
  const [prefix, suffix] = pieces;
  if (!hostname.startsWith(prefix) || !hostname.endsWith(suffix)) return null;
  const label = hostname.slice(prefix.length, hostname.length - suffix.length);
  return PRODUCT_LABEL.test(label) ? { label } : null;
}

/**
 * The origins that may embed the loader served at `sandbox`.
 *
 * Configured origins win. Beside a loopback host the loopback names on the
 * product's own scheme and port qualify. A port range on one name puts the
 * wallets on that name's default port. A label template has nothing to derive
 * from, so it gives none until the operator names them.
 */
export function trustedHostOrigins(
  policy: SandboxPolicy,
  sandbox: { protocol: string; port: string },
): string[] {
  if (policy.hostOrigins.length > 0) return policy.hostOrigins;
  const template = policy.template.trim();
  if (template === "") {
    const port = sandbox.port === "" ? "" : `:${sandbox.port}`;
    return LOOPBACK_HOSTS.map((host) => `${sandbox.protocol}//${host}${port}`);
  }
  const range = portRangeOf(template);
  if (range === null) return [];
  const url = new URL(template.replace(PORT_RANGE, `:${range.first}`));
  return [`${url.protocol}//${url.hostname}`];
}

/** What the loader can see of the page that embeds it. */
export interface EmbeddingContext {
  location: Pick<Location, "protocol" | "hostname" | "port" | "origin">;
  topLevel: boolean;
  /** The browser's own list of ancestor origins, nearest first; undefined where it keeps none. */
  ancestorOrigins: readonly string[] | undefined;
}

/**
 * Check that this page is a product origin, embedded directly by a host the
 * operator trusts, and return that host's origin and the label the origin
 * carries. Every check runs before the loader touches storage or a worker.
 *
 * The embedding page is read from the browser's ancestor list, not from the
 * URL. The host's frames send no referrer, so a browser without the list has
 * nothing else to go on and is refused.
 */
export function authorizeEmbedding(
  policy: SandboxPolicy,
  context: EmbeddingContext,
): { hostOrigin: string; label: string } {
  const { location } = context;
  const sandbox = identifySandbox(policy, location);
  if (sandbox === null)
    throw new Error(
      `${location.origin} is not a product origin of this host, so it will not load a product.`,
    );
  if (context.topLevel)
    throw new Error("The loader runs only inside the host, never on its own.");
  const trusted = trustedHostOrigins(policy, location);
  if (trusted.length === 0)
    throw new Error(
      "No host origin is configured for this product origin. Start the server with WEB_SIGNING_HOST_ORIGINS.",
    );
  const { ancestorOrigins } = context;
  if (ancestorOrigins === undefined)
    throw new Error(
      "This browser does not say which page embeds the loader (location.ancestorOrigins), so it will not load a product.",
    );
  const parent = ancestorOrigins.length === 1 ? ancestorOrigins[0] : null;
  if (parent === null || !trusted.includes(parent))
    throw new Error(
      "The page that embeds the loader is not this host, so it will not load a product.",
    );
  return { hostOrigin: parent, label: sandbox.label };
}
