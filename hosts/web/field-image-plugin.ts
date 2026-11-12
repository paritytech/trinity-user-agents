import { randomBytes } from "node:crypto";
import type { IncomingMessage, ServerResponse } from "node:http";
import type { Plugin } from "vite";
import { LOOPBACK_HOSTNAMES } from "./tools/demos/local-demo-route.js";

/**
 * DEV ONLY, PROTOTYPE. The web host's local image service for protected
 * fields: the host page draws a value as SVG (`src/fields/svg.ts`) and hands
 * it here; a product embeds the URL it gets with `<img>`. The browser keeps a
 * cross-origin image's pixels from the product's scripts. This dev server is
 * an extra trusted component: it holds the SVG in memory until it is served
 * or ends. It is the web stand-in for a native host's own local image scheme.
 *
 * - The host page issues one token per image (`POST issue`), naming the
 *   product origin, and ends unused ones (`POST revoke`). Both need the
 *   `x-field-images` header and a same-origin fetch.
 * - `GET <token>.svg` answers only once, within {@link TOKEN_MS}, to a
 *   request from this machine to a loopback name, with `Sec-Fetch-Dest:
 *   image` and a `Referer` of the product origin the token was issued for.
 *   Every refusal is the same empty 404, and a refusal does not use the
 *   token up.
 * - The answer is `no-store`, `nosniff`, sandboxed by its CSP, and carries
 *   no `Access-Control-*` header (Vite's own CORS middleware adds them first;
 *   they are removed here). `no-store` keeps it out of caches; it does not
 *   stop whoever received it from keeping it.
 *
 * Known limit, accepted for this prototype: the URL is a bearer capability.
 * Product frames are `credentialless`, so an image request carries no cookie
 * of the host's, and nothing ties it to this browser: until it is used,
 * anything on this machine that has the URL can send those two headers and
 * get the SVG, value included. The loopback check refuses other machines,
 * and a tunnel that passes their `Host` on; a tunnel or proxy on this
 * machine that rewrites `Host` to a loopback name gets through.
 *
 * Tokens are one pool of {@link MAX_TOKENS} for every product, so a product
 * can fill it and refuse others' images until its tokens expire.
 *
 * Nothing logs a value or a token. A build does not include it.
 */

/** Where the endpoints live under the dev server's base. */
export const FIELD_IMAGES_PATH = "__field-images/";
/** How long an issued token waits for its image request. */
export const TOKEN_MS = 60_000;
const MAX_TOKENS = 500;
const MAX_SVG_BYTES = 8192;
const MAX_BODY_BYTES = 2 * MAX_SVG_BYTES;

interface Issued {
  /** The product origin whose documents may load it, such as `http://localhost:5183`. */
  origin: string;
  svg: string;
  expires: number;
}

/** Whether the request came from this machine, to this machine's name. */
function loopback(request: IncomingMessage): boolean {
  const host = (request.headers.host ?? "").replace(/:\d+$/, "");
  const remote = request.socket.remoteAddress ?? "";
  return (
    LOOPBACK_HOSTNAMES.includes(host) &&
    (remote === "127.0.0.1" ||
      remote === "::1" ||
      remote === "::ffff:127.0.0.1")
  );
}

function readBody(request: IncomingMessage): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    request.on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_BODY_BYTES) {
        reject(new Error("too large"));
        request.destroy();
        return;
      }
      chunks.push(chunk);
    });
    request.on("end", () => {
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (error) {
        reject(error);
      }
    });
    request.on("error", reject);
  });
}

const isOrigin = (value: unknown): value is string =>
  typeof value === "string" && /^https?:\/\/[^/?#\s]{1,253}$/.test(value);

/** The endpoints' state and handler, apart from Vite, so tests can serve it. */
export function createFieldImages(now: () => number = Date.now) {
  const tokens = new Map<string, Issued>();
  const sweep = () => {
    const at = now();
    for (const [token, issued] of tokens)
      if (issued.expires <= at) tokens.delete(token);
  };
  const refuse = (response: ServerResponse) => {
    response.statusCode = 404;
    response.setHeader("cache-control", "no-store");
    response.end();
  };
  const sameOriginPost = (request: IncomingMessage) =>
    request.method === "POST" &&
    request.headers["x-field-images"] === "1" &&
    request.headers["sec-fetch-site"] === "same-origin";

  async function handle(
    request: IncomingMessage,
    response: ServerResponse,
    path: string,
  ): Promise<void> {
    for (const name of response.getHeaderNames())
      if (name.startsWith("access-control-")) response.removeHeader(name);
    sweep();
    if (!loopback(request)) return refuse(response);
    if (path === "issue" || path === "revoke") {
      if (!sameOriginPost(request)) return refuse(response);
      let body: Record<string, unknown>;
      try {
        body = (await readBody(request)) as Record<string, unknown>;
      } catch {
        return refuse(response);
      }
      if (path === "revoke") {
        const listed = Array.isArray(body?.tokens) ? body.tokens : [];
        for (const token of listed)
          if (typeof token === "string") tokens.delete(token);
        response.statusCode = 204;
        return void response.end();
      }
      const { origin, svg } = body ?? {};
      if (
        !isOrigin(origin) ||
        typeof svg !== "string" ||
        !svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg"') ||
        Buffer.byteLength(svg) > MAX_SVG_BYTES ||
        tokens.size >= MAX_TOKENS
      )
        return refuse(response);
      const token = randomBytes(32).toString("hex");
      tokens.set(token, {
        origin,
        svg,
        expires: now() + TOKEN_MS,
      });
      response.statusCode = 200;
      response.setHeader("cache-control", "no-store");
      response.setHeader("content-type", "application/json");
      return void response.end(JSON.stringify({ token, ttl: TOKEN_MS }));
    }

    const match = /^([0-9a-f]{64})\.svg$/.exec(path);
    const issued = match === null ? undefined : tokens.get(match[1]!);
    let referer: string | null = null;
    try {
      referer = new URL(request.headers.referer ?? "").origin;
    } catch {
      referer = null;
    }
    if (
      request.method !== "GET" ||
      issued === undefined ||
      request.headers["sec-fetch-dest"] !== "image" ||
      referer !== issued.origin
    )
      return refuse(response);
    tokens.delete(match![1]!);
    response.statusCode = 200;
    response.setHeader("content-type", "image/svg+xml");
    response.setHeader("cache-control", "no-store");
    response.setHeader("x-content-type-options", "nosniff");
    response.setHeader(
      "content-security-policy",
      "default-src 'none'; style-src 'unsafe-inline'; sandbox",
    );
    response.setHeader("referrer-policy", "no-referrer");
    response.end(issued.svg);
  }

  return { handle };
}

/** The Vite plugin: {@link createFieldImages} at `<base>__field-images/`. */
export function fieldImages(): Plugin {
  let base = "/";
  const images = createFieldImages();
  return {
    name: "truapi-field-images",
    apply: "serve",
    configResolved(config) {
      base = config.base.startsWith("/") ? config.base : "/";
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? "/", "http://dev.invalid");
        const prefix = `${base}${FIELD_IMAGES_PATH}`;
        if (!url.pathname.startsWith(prefix)) return next();
        images
          .handle(request, response, url.pathname.slice(prefix.length))
          .catch(() => {
            response.statusCode = 404;
            response.end();
          });
      });
    },
  };
}
