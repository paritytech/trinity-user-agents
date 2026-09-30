import {
  type SandboxPolicy,
  identifySandbox,
  trustedHostOrigins,
} from "./policy.js";

/** The kinds of file a product origin serves for itself. */
export type AssetKind = "loader" | "script" | "worker";

export interface AssetRequest {
  kind: AssetKind;
  method: string;
  /** The `Host` header, which a browser sets and a page cannot. */
  host: string | undefined;
  /** `Sec-Fetch-Dest`: what the browser is loading the file for. */
  fetchDest: string | undefined;
  /** Whether the connection to this server is TLS. */
  secure: boolean;
}

/** How a request path relates to the sandbox's files. */
export type AssetPath = AssetKind | "reserved" | null;

const RESERVED = /^\/__sandbox(?:$|\/|-)/;

/**
 * Which sandbox file `rawPath` names, from the path as the browser sent it.
 *
 * A static file server decodes the path before it looks for a file, so a
 * spelling such as `/__sandbox/%70age.js` reaches the same file as the plain
 * one. Anything that only reaches a file by another spelling is `"reserved"`:
 * the sandbox's prefix is not a product path, so it is refused instead of
 * passed on. `known` maps the exact plain paths to their kind. `null` means
 * the path has nothing to do with the sandbox.
 */
export function classifyAssetPath(
  rawPath: string,
  known: Record<string, AssetKind>,
): AssetPath {
  if (Object.hasOwn(known, rawPath)) return known[rawPath];
  let path = rawPath;
  for (let round = 0; round < 3; round += 1) {
    let decoded: string;
    try {
      decoded = decodeURIComponent(path);
    } catch {
      return /sandbox/i.test(path) ? "reserved" : null;
    }
    if (decoded === path) break;
    path = decoded;
  }
  const flattened = path
    .replaceAll("\\", "/")
    .replace(/\/{2,}/g, "/")
    .split("/")
    .reduce<string[]>((kept, segment) => {
      if (segment === "..") kept.pop();
      else if (segment !== ".") kept.push(segment);
      return kept;
    }, [])
    .join("/");
  return RESERVED.test(flattened.toLowerCase()) ? "reserved" : null;
}

export type AssetDecision =
  | {
      type: "serve";
      /** The origins the browser may let embed a loader page. */
      frameAncestors?: string[];
    }
  | { type: "refuse"; status: number; reason: string };

const HOST_HEADER = /^(\[[0-9a-f:.]+\]|[a-z0-9.-]+)(?::(\d{1,5}))?$/i;

/**
 * Whether this server hands `request` its sandbox file.
 *
 * The files belong to product origins only. On any other origin, which is
 * where the wallets are kept, nothing is served, except the worker: the same
 * script answers there and removes itself, so a worker an earlier build left
 * on that origin cannot keep controlling it. The loader is also refused
 * unless the browser says an embedding page asked for it, and its response
 * tells the browser which pages may embed it.
 */
export function decideAsset(
  policy: SandboxPolicy,
  request: AssetRequest,
): AssetDecision {
  if (request.method !== "GET" && request.method !== "HEAD")
    return { type: "refuse", status: 405, reason: "Method not allowed." };
  const host = HOST_HEADER.exec(request.host ?? "");
  if (host === null)
    return { type: "refuse", status: 400, reason: "No usable Host header." };
  const protocol = request.secure ? "https:" : "http:";
  const parts = { hostname: host[1], port: host[2] ?? "" };
  const sandbox = identifySandbox(policy, parts);
  if (sandbox === null)
    return request.kind === "worker"
      ? { type: "serve" }
      : {
          type: "refuse",
          status: 404,
          reason: "This origin does not serve product files.",
        };
  if (request.kind !== "loader") return { type: "serve" };
  if (request.fetchDest !== "iframe")
    return {
      type: "refuse",
      status: 403,
      reason: "The loader is served only into a frame of the host.",
    };
  const frameAncestors = trustedHostOrigins(policy, {
    protocol,
    port: parts.port,
  });
  return frameAncestors.length === 0
    ? {
        type: "refuse",
        status: 403,
        reason:
          "No host origin is configured. Start the server with WEB_SIGNING_HOST_ORIGINS.",
      }
    : { type: "serve", frameAncestors };
}
