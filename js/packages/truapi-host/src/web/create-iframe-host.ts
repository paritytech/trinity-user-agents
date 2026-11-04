/**
 * Options for `createIframeHost`.
 */
export interface IframeHostOptions {
  /** URL of the product iframe. */
  iframeUrl: string;
  /** Container element the iframe is appended to. */
  container: HTMLElement;
  /**
   * Called with the host end of a MessageChannel for each product document.
   * Hosts typically pipe this into a `WireProvider` (e.g. via
   * `createMessagePortProvider` from `@parity/truapi`).
   *
   * The first call happens immediately, before the iframe loads. A product
   * that navigates its whole document connects again, and the host gets a
   * fresh port once the new document sends its first frame. The function it
   * returned for the previous port is then called, and that port is closed.
   */
  onPort: (port: MessagePort) => (() => void) | void;
  /**
   * Optional explicit allow-list origin. Defaults to the origin of
   * `iframeUrl`. Throws if it disagrees with the iframe URL's origin.
   */
  allowedOrigin?: string;
  /** Optional iframe Permissions Policy allow attribute. */
  allow?: string;
  /** Override the default iframe sandbox attribute. */
  sandbox?: string;
}

/**
 * Handle returned by `createIframeHost`.
 */
export interface IframeHost {
  iframe: HTMLIFrameElement;
  dispose: () => void;
}

/** Ports a product may hold unused before the oldest are closed. */
const MAX_PENDING_OFFERS = 16;

const DEFAULT_IFRAME_SANDBOX = "allow-forms allow-same-origin allow-scripts";
type CredentiallessIframe = HTMLIFrameElement & { credentialless?: boolean };

function sandboxCreatesOpaqueOrigin(sandbox: string): boolean {
  // Sandbox tokens are matched ASCII case-insensitively by the browser.
  return !sandbox.toLowerCase().split(/\s+/).includes("allow-same-origin");
}

function resolveAllowedOrigin(
  iframeUrl: string,
  allowedOrigin?: string,
): string {
  const targetUrl = new URL(iframeUrl, window.location.href);
  if (targetUrl.protocol !== "http:" && targetUrl.protocol !== "https:") {
    throw new Error(
      `Iframe host only allows http(s) product URLs, received ${targetUrl.protocol}`,
    );
  }

  if (!allowedOrigin) {
    return targetUrl.origin;
  }

  const normalizedOrigin = new URL(allowedOrigin).origin;
  if (normalizedOrigin !== targetUrl.origin) {
    throw new Error(
      `Iframe host origin policy mismatch, expected ${normalizedOrigin}, got ${targetUrl.origin}`,
    );
  }

  return normalizedOrigin;
}

/**
 * Embed a product iframe and transfer a `MessagePort` into it. The host
 * keeps the other end and passes it to a `WireProvider` (typically via
 * `createMessagePortProvider`). All product traffic flows over the
 * MessageChannel.
 */
export function createIframeHost(options: IframeHostOptions): IframeHost {
  const {
    iframeUrl,
    container,
    onPort,
    allowedOrigin,
    allow,
    sandbox = DEFAULT_IFRAME_SANDBOX,
  } = options;

  const targetOrigin = resolveAllowedOrigin(iframeUrl, allowedOrigin);

  // The port serving the product's current document, and what to run when a
  // newer document replaces it.
  const first = new MessageChannel();
  let current: Connection = {
    port: first.port1,
  };
  // Hand the host-side port to the caller immediately so it can wire up
  // a provider before the iframe finishes loading. Queued postMessage
  // calls are delivered once the channel is started by the provider.
  current.retire = onPort(first.port1) ?? undefined;
  let firstProductPort: MessagePort | undefined = first.port2;
  // Ports offered to a document after the first, not yet used by it.
  const offers = new Set<Offer>();

  const iframe = document.createElement("iframe");
  iframe.style.width = "100%";
  iframe.style.height = "100%";
  iframe.style.border = "none";
  // COEP hosts need credentialless product iframes when the product origin
  // does not serve matching embedder headers.
  const credentiallessIframe = iframe as CredentiallessIframe;
  credentiallessIframe.credentialless = true;
  if (allow !== undefined) {
    iframe.allow = allow;
  }
  iframe.setAttribute("sandbox", sandbox);
  iframe.referrerPolicy = "no-referrer";
  iframe.src = iframeUrl;
  const acceptsOpaqueOrigin = sandboxCreatesOpaqueOrigin(sandbox);
  // Opaque sandbox origins cannot be addressed by URL origin. Use "*" only
  // for that explicit sandbox mode; otherwise pin the capability port transfer.
  const initTargetOrigin = acceptsOpaqueOrigin ? "*" : targetOrigin;

  const sendInit = (productPort: MessagePort): void => {
    iframe.contentWindow?.postMessage(
      { type: "truapi-init" },
      initTargetOrigin,
      [productPort],
    );
  };

  // A product repeats `truapi-ready` until it holds a port, so a ready that
  // follows an init is either one more from the document that already took
  // its port, or the first from a document that replaced it. The two cannot
  // be told apart, so each such ready gets a port of its own, and that port
  // becomes the product's connection only once a frame arrives on it. A ready
  // the connected document sent before it took its port leaves an idle offer
  // and the connection untouched.
  const offer = (): void => {
    // Only a document that has not taken a port still sends ready, so once
    // readies outnumber the cap the oldest offers went to documents that are
    // gone or will never connect.
    for (const oldest of offers) {
      if (offers.size < MAX_PENDING_OFFERS) break;
      offers.delete(oldest);
      oldest.channel.port1.close();
    }
    const channel = new MessageChannel();
    const pending: Offer = { channel };
    offers.add(pending);
    pending.channel.port1.onmessage = (event: MessageEvent) => {
      promote(pending, event.data);
    };
    sendInit(channel.port2);
  };

  // The relay keeps the first frame, which arrived before the caller had a
  // port to read it from: a message posted to a port waits until that port
  // is started, so the caller reads it whenever it attaches.
  const promote = (pending: Offer, firstFrame: unknown): void => {
    for (const older of offers) {
      offers.delete(older);
      if (older === pending) break;
      older.channel.port1.close();
    }
    retire(current);
    const relay = new MessageChannel();
    const productSide = pending.channel.port1;
    productSide.onmessage = (event: MessageEvent) => {
      relay.port2.postMessage(event.data);
    };
    relay.port2.onmessage = (event: MessageEvent) => {
      productSide.postMessage(event.data);
    };
    relay.port2.postMessage(firstFrame);
    current = {
      port: relay.port1,
      relay: { productSide, hostSide: relay.port2 },
    };
    current.retire = onPort(relay.port1) ?? undefined;
  };

  const onWindowMessage = (event: MessageEvent): void => {
    if (event.source !== iframe.contentWindow) return;
    if (
      event.origin !== targetOrigin &&
      !(acceptsOpaqueOrigin && event.origin === "null")
    ) {
      return;
    }
    if (event.data?.type !== "truapi-ready") return;
    if (firstProductPort) {
      sendInit(firstProductPort);
      firstProductPort = undefined;
    } else {
      offer();
    }
  };
  window.addEventListener("message", onWindowMessage);

  container.appendChild(iframe);

  return {
    iframe,
    dispose() {
      window.removeEventListener("message", onWindowMessage);
      for (const pending of offers) pending.channel.port1.close();
      offers.clear();
      retire(current);
      firstProductPort?.close();
      iframe.remove();
    },
  };
}

/** A port sent to a product document that has not used it yet. */
interface Offer {
  channel: MessageChannel;
}

/** The host's end of one product document's connection. */
interface Connection {
  port: MessagePort;
  retire?: () => void;
  /** Set when the connection was promoted from an offer through a relay. */
  relay?: { productSide: MessagePort; hostSide: MessagePort };
}

/** Run a connection's teardown and close every port it holds. */
function retire(connection: Connection): void {
  connection.retire?.();
  for (const port of [
    connection.port,
    connection.relay?.productSide,
    connection.relay?.hostSide,
  ]) {
    try {
      port?.close();
    } catch {
      // already closed
    }
  }
}

export type { WireProvider } from "@parity/truapi";
