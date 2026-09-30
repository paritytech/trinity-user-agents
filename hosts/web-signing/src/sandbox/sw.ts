import { ARCHIVE_CACHE, fileKey } from "./files.js";
import { identifySandbox, trustedHostOrigins } from "./policy.js";
import {
  SANDBOX_PREFIX,
  SANDBOX_WORKER,
  archivePath,
  containerWanted,
  responseFor,
} from "./serve.js";

/** The parts of a service worker scope this file uses; the host build has no worker types. */
interface FetchEvent extends Event {
  request: Request;
  respondWith(response: Promise<Response> | Response): void;
}
interface WorkerScope {
  location: Location;
  registration: { unregister(): Promise<boolean> };
  skipWaiting(): Promise<void>;
  clients: {
    claim(): Promise<void>;
    matchAll(options: {
      type: "window";
    }): Promise<{ url: string; navigate(url: string): Promise<unknown> }[]>;
  };
  addEventListener(
    type: "install" | "activate",
    listener: (event: ExtendableEvent) => void,
  ): void;
  addEventListener(type: "fetch", listener: (event: FetchEvent) => void): void;
}
interface ExtendableEvent extends Event {
  waitUntil(work: Promise<unknown>): void;
}
declare const self: WorkerScope;

/** The host the container is told to take its private port from, fixed when the worker is registered. */
const registration = new URL(self.location.href).searchParams;
const hostOrigin = registration.get("host") ?? "";
/** Whether HTML gets the container. A request to drop it counts only on a `.localhost` origin. */
const withContainer = containerWanted(
  registration.get("container") !== "off",
  self.location.hostname,
);

/**
 * Whether this worker may serve a product: it must sit on a product origin of
 * the compiled policy, and be told to trust a host that policy names. Anything
 * else gets no product from it, whoever registered it.
 */
const here = new URL(self.location.href);
const serving =
  identifySandbox(__SANDBOX_POLICY__, here) !== null &&
  trustedHostOrigins(__SANDBOX_POLICY__, here).includes(hostOrigin);

self.addEventListener("install", (event) =>
  event.waitUntil(self.skipWaiting()),
);

/**
 * A worker outside its policy removes itself and reloads the pages it held.
 * This is also how a worker an earlier build left on the wallet's own origin
 * goes: the server answers that origin's update check with this script.
 */
async function retire(): Promise<void> {
  await self.registration.unregister();
  const windows = await self.clients.matchAll({ type: "window" });
  await Promise.all(windows.map((client) => client.navigate(client.url)));
}

self.addEventListener("activate", (event) =>
  event.waitUntil(serving ? self.clients.claim() : retire()),
);

const notFound = () =>
  new Response("Not found in this product's archive.", {
    status: 404,
    headers: { "content-type": "text/plain; charset=utf-8" },
  });

async function serve(request: Request): Promise<Response> {
  const url = new URL(request.url);
  if (request.method !== "GET" && request.method !== "HEAD")
    return new Response("Method not allowed.", { status: 405 });
  const cache = await caches.open(ARCHIVE_CACHE);
  const wanted = archivePath(url.pathname);
  let path = wanted;
  let stored =
    wanted === null
      ? undefined
      : await cache.match(fileKey(url.origin, wanted));
  if (stored === undefined && request.mode === "navigate") {
    path = "index.html";
    stored = await cache.match(fileKey(url.origin, path));
  }
  if (stored === undefined || path === null) return notFound();
  return responseFor(
    { path, bytes: new Uint8Array(await stored.arrayBuffer()) },
    withContainer ? hostOrigin : null,
  );
}

if (serving)
  self.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    if (url.origin !== self.location.origin) return;
    if (
      url.pathname.startsWith(SANDBOX_PREFIX) ||
      url.pathname === SANDBOX_WORKER
    )
      return;
    event.respondWith(serve(event.request));
  });
