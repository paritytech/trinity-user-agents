import { parseCidString } from "../archive/cid.js";
import {
  ARCHIVE_LIMITS,
  carBlocks,
  carCounts,
  carSkip,
  gatewayBlocks,
  loadArchive,
  loadSite,
  rawCounts,
} from "../archive/gateway.js";
import {
  type ProductLoadSummary,
  type ReadTiming,
  postLoadSummary,
  timeReads,
} from "../load-observer.js";
import { hostBase, parseMountScope } from "./mount.js";
import { archivePath } from "./serve.js";
import { storeArchive } from "./store.js";

/** What the loader tells the host that embeds it, so the host can show why a product is not up. */
export type SandboxStatus = {
  type: "truapi-sandbox";
  state: "loading" | "error";
  detail: string;
};

const params = new URLSearchParams(location.search);
/** The host's directory: this page is `<base>truapi-sandbox/index.html`. */
const base = hostBase("../", location.href);

function report(state: SandboxStatus["state"], detail: string): void {
  const line = document.getElementById("status") as HTMLElement;
  line.textContent = detail;
  line.dataset.state = state;
  if (window.parent !== window)
    window.parent.postMessage(
      { type: "truapi-sandbox", state, detail } satisfies SandboxStatus,
      location.origin,
    );
}

/**
 * Register the worker for `scope` and wait until it is active, so the
 * navigation into the product reaches it and not the static host.
 */
async function startWorker(scope: string): Promise<void> {
  const script = new URL(`${base}sw.js`, location.origin).href;
  const registration = await navigator.serviceWorker.register(script, {
    scope,
  });
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("The product worker did not start.")),
      10_000,
    );
    const check = (): boolean => {
      if (registration.active?.scriptURL !== script) return false;
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

/** What this load has counted so far, posted once when it ends. */
const counted: Omit<ProductLoadSummary, "outcome" | "totalMs"> = {};
const loaderStart = performance.now();

/** Tell the host what this load did, once, before the frame moves on or stops. */
function summarize(
  outcome: ProductLoadSummary["outcome"],
  reason?: string,
): void {
  if (window.parent === window) return;
  postLoadSummary({
    ...counted,
    outcome,
    totalMs: performance.now() - loaderStart,
    ...(reason === undefined ? {} : { reason }),
  });
}

async function load(): Promise<void> {
  if (window.parent === window)
    throw new Error("The loader runs only inside the host, never on its own.");
  if (!isSecureContext)
    throw new Error(
      "Not a secure context: every page above the product must be https or localhost.",
    );
  if (!("serviceWorker" in navigator))
    throw new Error(
      "This browser does not offer service workers here, and the product loader needs them. An embedded web view often lacks them; open the host in the system browser.",
    );

  const scope = params.get("scope") ?? "";
  const mount = parseMountScope(base, scope);
  if (mount === null)
    throw new Error("The link names no product mount of this host.");
  const { cid } = mount;
  const root = parseCidString(cid);
  const gateway = new URL(params.get("gateway") ?? "");
  if (gateway.protocol !== "https:" && gateway.protocol !== "http:")
    throw new Error("The gateway must be http or https.");
  const kind = params.get("kind");
  if (kind !== "car" && kind !== "site")
    throw new Error("The loader was not told what the content is.");
  const start = new URL(
    (params.get("start") ?? "/").replace(/^\/+/, ""),
    new URL(scope, location.origin),
  );
  if (!start.href.startsWith(new URL(scope, location.origin).href))
    throw new Error("The start path must stay inside the product's mount.");
  const entry = params.get("entry") ?? "index.html";
  if (archivePath(`/${entry}`) !== entry)
    throw new Error("The entry is not a path inside the content.");

  report("loading", "Starting the product worker.");
  const workerStart = performance.now();
  await startWorker(scope);
  counted.workerMs = performance.now() - workerStart;

  const { reused, files, storeMs } = await storeArchive(
    cid,
    location.origin,
    async () => {
      report("loading", `Fetching ${cid} from ${gateway.origin}.`);
      const loadStart = performance.now();
      const timing: ReadTiming = { reads: 0, waitMs: 0 };
      const rawBlocks = gatewayBlocks(
        gateway.origin,
        ARCHIVE_LIMITS,
        fetch,
        (counted.raw = rawCounts()),
      );
      const skip = carSkip(gateway.origin, location.origin);
      if (skip !== null) counted.carSkipped = skip;
      const blocks = timeReads(
        skip === null
          ? carBlocks(
              root,
              gateway.origin,
              ARCHIVE_LIMITS,
              rawBlocks,
              fetch,
              (counted.car = carCounts()),
            )
          : rawBlocks,
        loadStart,
        timing,
      );
      try {
        const { files } = await (kind === "car" ? loadArchive : loadSite)(
          cid,
          blocks,
          ARCHIVE_LIMITS,
          undefined,
          entry,
        );
        report("loading", `Verified ${files.size} files.`);
        return files;
      } finally {
        counted.loadMs = performance.now() - loadStart;
        counted.reads = timing.reads;
        if (kind === "site") counted.waitMs = timing.waitMs;
        else if (timing.lastBlockMs !== undefined)
          counted.fetchMs = timing.lastBlockMs;
      }
    },
    { caches, locks: navigator.locks },
  );
  if (files !== null) counted.files = files;
  if (storeMs !== undefined) counted.storeMs = storeMs;
  report(
    "loading",
    reused
      ? "Using the verified copy. Starting the product."
      : "Starting the product.",
  );
  summarize(reused ? "cached" : "fetched");
  location.replace(start.href);
}

load().catch((error: unknown) => {
  const detail = error instanceof Error ? error.message : String(error);
  report("error", detail);
  summarize("failed", detail);
});
