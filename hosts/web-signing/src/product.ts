import { createIframeHost } from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import {
  attachContainerChannel,
  createFrameRouter,
  type ContainerChannel,
} from "./container-channel.js";
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
 * Camera and microphone are delegated only to a frame that is expected to load
 * the container, because the container is what asks the core before a page
 * opens a device. A bare feature name delegates to the origin of the frame's
 * `src` and to nothing nested inside it, so a subframe needs its own grant from
 * the page that embeds it. Without the container the frame gets none, and the
 * browser refuses capture outright. The browser's own prompt and the
 * operating system's still apply in either case.
 */
export function productFramePolicy(
  expectContainer: boolean,
): string | undefined {
  return expectContainer ? "camera; microphone" : undefined;
}

/** A product embedded in the page and connected to the core. */
export interface OpenProduct {
  url: URL;
  productId: string;
  /** Present only when the page was opened expecting a container. */
  container: ContainerChannel | null;
  /**
   * Whether the frame loaded a new document after it was connected. Its
   * channels are closed for good, and the page stays as it is until the product
   * is opened again.
   */
  readonly lost: boolean;
  dispose(): void;
}

export interface OpenProductOptions {
  /** Sees every frame between the product and the core, in the product's view. */
  tap?: FrameTap;
  /**
   * Answer a container announcement from the page. Off for a page that does
   * not bring the container, so it cannot ask for a private channel.
   */
  expectContainer?: boolean;
  /** Called when a container announced itself and was given its port. */
  onContainerConnected?(): void;
  /**
   * Called once when a connected page navigated to a new document and its
   * channels were closed. Only a page that brings the container can be seen
   * doing this.
   */
  onLost?(): void;
  /**
   * Called each time the frame finishes loading a document, with how many it
   * has loaded since it was created. A load says the document arrived, not
   * that the product inside it is ready.
   */
  onFrameLoad?(count: number): void;
  /** Called for a frame dropped for being on the wrong channel. */
  onDrop?(reason: string): void;
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
 * Frames between the product and the core pass through a router that keeps the
 * container's private authorization traffic apart from the product's own.
 */
export async function openProduct(
  runtime: WorkerPairingHostRuntime,
  url: URL,
  productId: string,
  container: HTMLElement,
  options: OpenProductOptions = {},
): Promise<OpenProduct> {
  const {
    tap,
    expectContainer = false,
    onContainerConnected,
    onLost,
    onFrameLoad,
    onDrop,
    onSandboxStatus,
    mounted = false,
  } = options;
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
  let channel: ContainerChannel | null = null;
  let lost = false;
  let released = false;
  const router = createFrameRouter({
    toCore: (frame) => provider.postMessage(frame),
    toProduct: (frame) => productPort?.postMessage(frame),
    tap,
    onDrop,
  });
  const frame = createIframeHost({
    iframeUrl: url.href,
    container,
    allow: productFramePolicy(expectContainer),
    onPort(port) {
      productPort = port;
      unsubscribe = provider.subscribe((message) => router.fromCore(message));
      port.onmessage = (event: MessageEvent) => router.fromProduct(event.data);
      port.start();
    },
  });
  let loads = 0;
  const onLoad = (): void => {
    loads += 1;
    onFrameLoad?.(loads);
  };
  frame.iframe.addEventListener("load", onLoad);
  /** Close every channel to the core. The frame itself is left in place. */
  const release = (): void => {
    if (released) return;
    released = true;
    channel?.dispose();
    unsubscribe();
    productPort?.close();
    provider.dispose();
  };
  channel = expectContainer
    ? attachContainerChannel({
        frame: frame.iframe,
        origin: url.origin,
        router,
        onConnected: onContainerConnected,
        onLost() {
          lost = true;
          release();
          onLost?.();
        },
      })
    : null;
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
    container: channel,
    get lost() {
      return lost;
    },
    dispose() {
      window.removeEventListener("message", onWindowMessage);
      frame.iframe.removeEventListener("load", onLoad);
      release();
      frame.dispose();
    },
  };
}
