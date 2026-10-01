import { archiveCacheName, completeKey, fileKey } from "./files.js";
import { SANDBOX_DIR, parseMountScope } from "./mount.js";
import { archiveRequestPath, responseFor } from "./serve.js";

/** The parts of a service worker scope this file uses; the host build has no worker types. */
interface FetchEvent extends Event {
  request: Request;
  respondWith(response: Promise<Response> | Response): void;
}
interface WorkerScope {
  location: Location;
  registration: { scope: string; unregister(): Promise<boolean> };
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

/** The directory this script is served from, which is the host's base path. */
const base = new URL("./", self.location.href).pathname;
const scope = new URL(self.registration.scope);
/** What this registration serves, read from its own scope: a worker registered anywhere else serves nothing. */
const mount = parseMountScope(base, scope.pathname);

self.addEventListener("install", (event) =>
  event.waitUntil(self.skipWaiting()),
);

/** A worker outside any mount removes itself and reloads the pages it held. */
async function retire(): Promise<void> {
  await self.registration.unregister();
  const windows = await self.clients.matchAll({ type: "window" });
  await Promise.all(windows.map((client) => client.navigate(client.url)));
}

self.addEventListener("activate", (event) =>
  event.waitUntil(mount !== null ? self.clients.claim() : retire()),
);

const notFound = (reason: string) =>
  new Response(reason, {
    status: 404,
    headers: { "content-type": "text/plain; charset=utf-8" },
  });

async function serve(
  request: Request,
  content: { cid: string },
): Promise<Response> {
  const url = new URL(request.url);
  if (request.method !== "GET" && request.method !== "HEAD")
    return new Response("Method not allowed.", { status: 405 });
  const cache = await caches.open(archiveCacheName(content.cid));
  if ((await cache.match(completeKey(url.origin))) === undefined)
    return notFound("This product's archive is not loaded. Open it again.");
  let path = archiveRequestPath(url.pathname, scope.pathname);
  let stored =
    path === null ? undefined : await cache.match(fileKey(url.origin, path));
  if (stored === undefined && request.mode === "navigate") {
    path = "index.html";
    stored = await cache.match(fileKey(url.origin, path));
  }
  if (stored === undefined || path === null)
    return notFound("Not found in this product's archive.");
  return responseFor({
    path,
    bytes: new Uint8Array(await stored.arrayBuffer()),
  });
}

// Every request a controlled page makes arrives here, whatever its path, so a
// root-relative `/assets/app.js` is answered from the archive although it is
// outside the scope. A request to another origin, and the host's own loader
// files, go to the network.
if (mount !== null)
  self.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    if (url.origin !== self.location.origin) return;
    if (url.pathname.startsWith(`${base}${SANDBOX_DIR}`)) return;
    event.respondWith(serve(event.request, mount));
  });
