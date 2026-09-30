import { parseCidString } from "../archive/cid.js";
import {
  ARCHIVE_LIMITS,
  gatewayBlocks,
  loadArchive,
  loadSite,
} from "../archive/gateway.js";
import { OriginHeldError, claimOrigin } from "./claim.js";
import { ARCHIVE_CACHE, fileKey } from "./files.js";
import { authorizeEmbedding } from "./policy.js";
import { SANDBOX_WORKER, contentTypeOf } from "./serve.js";

/** What the loader tells the host that embeds it, so the host can show why a product is not up. */
export type SandboxStatus =
  | { type: "truapi-sandbox"; state: "loading"; detail: string }
  | {
      type: "truapi-sandbox";
      state: "error";
      detail: string;
      /** `owner`: this origin holds another product's data. */
      code?: "owner";
    };

const params = new URLSearchParams(location.search);

/**
 * The host this page answers to. It is set once the embedding page has been
 * checked, and nothing is sent to a host before that.
 */
let authorizedHost: string | null = null;

function report(status: SandboxStatus, target: string | null): void {
  const line = document.getElementById("status") as HTMLElement;
  line.textContent = status.detail;
  line.dataset.state = status.state;
  if (target !== null && window.parent !== window)
    window.parent.postMessage(status, target);
}

/**
 * Register the worker at `script` and wait until that exact script controls
 * the origin, so a worker left from an earlier open, with other settings, never
 * answers the product's first requests.
 */
async function startWorker(script: string): Promise<void> {
  const registration = await navigator.serviceWorker.register(script, {
    scope: "/",
  });
  const expected = new URL(script, location.origin).href;
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("The sandbox worker did not start.")),
      10_000,
    );
    const check = (): boolean => {
      if (registration.active?.scriptURL !== expected) return false;
      clearTimeout(timer);
      resolve();
      return true;
    };
    if (check()) return;
    const watch = (worker: ServiceWorker | null): void =>
      worker?.addEventListener("statechange", check);
    watch(registration.installing);
    watch(registration.waiting);
    registration.addEventListener("updatefound", () =>
      watch(registration.installing),
    );
  });
}

/**
 * Refuse anything but the configured host embedding this origin, and a link
 * that names a different host or product than the origin itself does. URL
 * values are requests, not credentials: the host and the label come from the
 * browser and the operator's policy.
 */
function authorize(): { host: string; label: string } {
  const { hostOrigin, label } = authorizeEmbedding(__SANDBOX_POLICY__, {
    location,
    topLevel: window.parent === window,
    ancestorOrigins:
      "ancestorOrigins" in location ? [...location.ancestorOrigins] : undefined,
  });
  authorizedHost = hostOrigin;
  if (params.get("host") !== hostOrigin)
    throw new Error(
      "The link names a different host than the one embedding it.",
    );
  const owner = params.get("owner") ?? "";
  if (label !== "" && owner !== label)
    throw new Error(
      "The link names a different product than this origin is for.",
    );
  return { host: hostOrigin, label: owner };
}

async function load(): Promise<void> {
  const { host, label: owner } = authorize();
  const say = (state: SandboxStatus["state"], detail: string) =>
    report({ type: "truapi-sandbox", state, detail }, host);
  const cid = params.get("cid") ?? "";
  parseCidString(cid);
  const gateway = new URL(params.get("gateway") ?? "");
  if (gateway.protocol !== "https:" && gateway.protocol !== "http:")
    throw new Error("The gateway must be http or https.");

  const kind = params.get("kind");
  if (kind !== "car" && kind !== "site")
    throw new Error("The sandbox was not told what the content is.");
  const start = new URL(params.get("start") ?? "/", location.origin);
  if (start.origin !== location.origin)
    throw new Error("The start path must stay on the product's origin.");

  if (!isSecureContext || !("serviceWorker" in navigator))
    throw new Error(
      "Not a secure context: every page above the product must be https or localhost.",
    );

  await claimOrigin(owner, location.origin, {
    caches,
    locks: navigator.locks,
  });

  say("loading", "Starting the sandbox.");
  await startWorker(
    `${SANDBOX_WORKER}?host=${encodeURIComponent(host)}&container=${params.get("container") === "off" ? "off" : "on"}`,
  );

  say("loading", `Fetching ${cid} from ${gateway.origin}.`);
  const blocks = gatewayBlocks(gateway.origin, ARCHIVE_LIMITS);
  const { files } = await (kind === "car" ? loadArchive : loadSite)(
    cid,
    blocks,
  );

  say("loading", `Verified ${files.size} files. Starting the product.`);
  await caches.delete(ARCHIVE_CACHE);
  const cache = await caches.open(ARCHIVE_CACHE);
  for (const [path, bytes] of files)
    await cache.put(
      fileKey(location.origin, path),
      new Response(bytes as BodyInit, {
        headers: { "content-type": contentTypeOf(path) },
      }),
    );
  location.replace(start.href);
}

load().catch((error: unknown) => {
  const detail = error instanceof Error ? error.message : String(error);
  report(
    error instanceof OriginHeldError
      ? { type: "truapi-sandbox", state: "error", detail, code: "owner" }
      : { type: "truapi-sandbox", state: "error", detail },
    authorizedHost,
  );
});
