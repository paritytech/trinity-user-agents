import { parseCidString } from "../archive/cid.js";
import {
  ARCHIVE_LIMITS,
  gatewayBlocks,
  loadArchive,
  loadSite,
} from "../archive/gateway.js";
import { hostBase, parseMountScope } from "./mount.js";
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
  parseCidString(cid);
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

  report("loading", "Starting the product worker.");
  await startWorker(scope);

  const { reused } = await storeArchive(
    cid,
    location.origin,
    async () => {
      report("loading", `Fetching ${cid} from ${gateway.origin}.`);
      const blocks = gatewayBlocks(gateway.origin, ARCHIVE_LIMITS);
      const { files } = await (kind === "car" ? loadArchive : loadSite)(
        cid,
        blocks,
      );
      report("loading", `Verified ${files.size} files.`);
      return files;
    },
    { caches, locks: navigator.locks },
  );
  report(
    "loading",
    reused
      ? "Using the verified copy. Starting the product."
      : "Starting the product.",
  );
  location.replace(start.href);
}

load().catch((error: unknown) =>
  report("error", error instanceof Error ? error.message : String(error)),
);
