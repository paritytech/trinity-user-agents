import { createIframeHost } from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";

/** A product URL the host can embed: http or https only. */
export function parseProductUrl(raw: string): URL {
  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    throw new Error(`Not a URL: ${raw}`);
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error(
      `Only http and https products can be embedded, not ${url.protocol}`,
    );
  }
  return url;
}

/**
 * The product id the core scopes accounts, grants and storage to.
 *
 * A URL has no dotNS name to take one from. A localhost URL keeps its port, so
 * two local products are two products, and the core accepts it as a
 * development id. Any other host needs `override` to name the dotNS product it
 * stands in for; the core refuses ids that are neither.
 */
export function productIdFor(url: URL, override: string): string {
  const named = override.trim();
  if (named) return named;
  return url.hostname === "localhost" ? url.host : url.hostname;
}

/** A product embedded in the page and connected to the core. */
export interface OpenProduct {
  url: URL;
  productId: string;
  dispose(): void;
}

/**
 * Embed `url` in `container` and connect it to a product runtime for
 * `productId`.
 *
 * The product runs on its own origin. That is what keeps it away from this
 * page's storage, where the wallets are, so a product must never be served
 * from this origin.
 */
export async function openProduct(
  runtime: WorkerPairingHostRuntime,
  url: URL,
  productId: string,
  container: HTMLElement,
): Promise<OpenProduct> {
  if (url.origin === window.location.origin) {
    throw new Error(
      "A product on this host's own origin could read the saved wallets.",
    );
  }
  const provider = await runtime.createProvider({
    productId,
    executionKind: "App",
  });
  let unsubscribe = () => {};
  const frame = createIframeHost({
    iframeUrl: url.href,
    container,
    onPort(port) {
      unsubscribe = provider.subscribe((message) => port.postMessage(message));
      port.onmessage = (event: MessageEvent) => {
        if (event.data instanceof Uint8Array) provider.postMessage(event.data);
      };
      port.start();
    },
  });
  return {
    url,
    productId,
    dispose() {
      unsubscribe();
      provider.dispose();
      frame.dispose();
    },
  };
}
