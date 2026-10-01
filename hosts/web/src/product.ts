import { createIframeHost } from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { FrameTap } from "./frame-tap.js";

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

/**
 * The Permissions Policy the product frame is given.
 *
 * Camera and microphone are delegated to the frame, so a product can ask the
 * browser for a device. A bare feature name delegates to the origin of the
 * frame's `src` and to nothing nested inside it. Delegating grants nothing by
 * itself: the browser's own prompt and the operating system's still apply.
 */
export const PRODUCT_FRAME_POLICY = "camera; microphone";

/** A product embedded in the page and connected to the core. */
export interface OpenProduct {
  url: URL;
  productId: string;
  dispose(): void;
}

export interface OpenProductOptions {
  /** Sees every frame between the product and the core, in the product's view. */
  tap?: FrameTap;
  /**
   * Called each time the frame finishes loading a document, with how many it
   * has loaded since it was created. A load says the document arrived, not
   * that the product inside it is ready.
   */
  onFrameLoad?(count: number): void;
  /**
   * Called with what the sandbox loader reports while it fetches and checks an
   * archive. Only the product frame's own window is heard.
   */
  onSandboxStatus?(state: "loading" | "error", detail: string): void;
  /**
   * Allow `url` on this page's own origin. Only a product mounted by this host
   * from a verified archive is opened that way; any other address on this
   * origin is refused.
   */
  mounted?: boolean;
}

/**
 * Embed `url` in `container` and connect it to a product runtime for
 * `productId`.
 *
 * An address is embedded where it is served, and an address on this page's own
 * origin is refused: the product would sit beside the saved wallets. The one
 * exception is a product this host mounted itself from a verified archive
 * (`mounted`), which is served from this origin on purpose. It is a
 * development tool, and it trusts the products its user opens.
 *
 * The product reaches the core over the public channel of `createIframeHost`.
 * Only calls made through TrUAPI are seen by the core; the page's own requests
 * follow the browser's rules.
 */
export async function openProduct(
  runtime: WorkerPairingHostRuntime,
  url: URL,
  productId: string,
  container: HTMLElement,
  options: OpenProductOptions = {},
): Promise<OpenProduct> {
  const { tap, onFrameLoad, onSandboxStatus, mounted = false } = options;
  if (url.origin === window.location.origin && !mounted) {
    throw new Error(
      "A product on this host's own origin could read the saved wallets.",
    );
  }
  const provider = await runtime.createProvider({
    productId,
    executionKind: "App",
  });
  let unsubscribe = () => {};
  let productPort: MessagePort | undefined;
  const frame = createIframeHost({
    iframeUrl: url.href,
    container,
    allow: PRODUCT_FRAME_POLICY,
    onPort(port) {
      productPort = port;
      unsubscribe = provider.subscribe((message) => {
        tap?.("in", message);
        port.postMessage(message);
      });
      port.onmessage = (event: MessageEvent) => {
        if (!(event.data instanceof Uint8Array)) return;
        tap?.("out", event.data);
        provider.postMessage(event.data);
      };
      port.start();
    },
  });
  let loads = 0;
  const onLoad = (): void => {
    loads += 1;
    onFrameLoad?.(loads);
  };
  frame.iframe.addEventListener("load", onLoad);
  const onWindowMessage = (event: MessageEvent): void => {
    if (event.source !== frame.iframe.contentWindow) return;
    if (event.origin !== url.origin) return;
    const data = event.data as {
      type?: unknown;
      state?: unknown;
      detail?: unknown;
    } | null;
    if (data?.type !== "truapi-sandbox" || typeof data.detail !== "string")
      return;
    if (data.state === "loading" || data.state === "error")
      onSandboxStatus?.(data.state, data.detail);
  };
  if (onSandboxStatus) window.addEventListener("message", onWindowMessage);
  return {
    url,
    productId,
    dispose() {
      window.removeEventListener("message", onWindowMessage);
      frame.iframe.removeEventListener("load", onLoad);
      unsubscribe();
      productPort?.close();
      provider.dispose();
      frame.dispose();
    },
  };
}
