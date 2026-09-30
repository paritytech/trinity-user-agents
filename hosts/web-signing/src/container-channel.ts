import { decodeWireMessage } from "@parity/truapi";
import type { FrameTap } from "./frame-tap.js";

/**
 * Requests the container makes on its own behalf carry this request-id prefix,
 * and the core echoes it on the reply. It is what tells private authorization
 * traffic from the product's own calls on the one core connection.
 */
export const PRIVATE_PREFIX = "host:";

/** The container announces itself with this, and is answered with a private port. */
export const CONTAINER_READY = "truapi-container-ready";
export const CONTAINER_INIT = "truapi-container-init";

/** What the router does with a frame nobody may see. */
export type Dropped = "product-private" | "container-public" | "undecodable";

export interface FrameRouter {
  /** A frame from the core: private replies go to the container, the rest to the product. */
  fromCore(frame: Uint8Array): void;
  /** A frame from the product's port. Frames claiming to be private are dropped. */
  fromProduct(data: unknown): void;
  /** A frame from the container's port. Only private frames are accepted. */
  fromContainer(data: unknown): void;
  /** Give the router the container's end of the private channel, or take it away with `undefined`. */
  setContainer(post: ((frame: Uint8Array) => void) | undefined): void;
}

export interface FrameRouterOptions {
  toCore(frame: Uint8Array): void;
  toProduct(frame: Uint8Array): void;
  tap?: FrameTap;
  onDrop?(reason: Dropped): void;
}

function requestIdOf(frame: Uint8Array): string | undefined {
  const decoded = decodeWireMessage(frame);
  return decoded.isOk() ? decoded.value.requestId : undefined;
}

/**
 * Split the one core connection into the product's channel and the container's.
 *
 * The container's authorization calls and their replies must never reach the
 * product, and the product must not be able to speak as the container. So the
 * split is by request id, and a frame on the wrong channel is dropped, not
 * forwarded. Nothing here reads or changes a payload.
 */
export function createFrameRouter(options: FrameRouterOptions): FrameRouter {
  const { toCore, toProduct, tap, onDrop } = options;
  let toContainer: ((frame: Uint8Array) => void) | undefined;
  return {
    setContainer(post) {
      toContainer = post;
    },
    fromCore(frame) {
      const id = requestIdOf(frame);
      tap?.("in", frame);
      if (id?.startsWith(PRIVATE_PREFIX)) toContainer?.(frame);
      else toProduct(frame);
    },
    fromProduct(data) {
      if (!(data instanceof Uint8Array)) return;
      const id = requestIdOf(data);
      if (id === undefined) return onDrop?.("undecodable");
      if (id.startsWith(PRIVATE_PREFIX)) return onDrop?.("product-private");
      tap?.("out", data);
      toCore(data);
    },
    fromContainer(data) {
      if (!(data instanceof Uint8Array)) return;
      const id = requestIdOf(data);
      if (id === undefined) return onDrop?.("undecodable");
      if (!id.startsWith(PRIVATE_PREFIX)) return onDrop?.("container-public");
      tap?.("out", data);
      toCore(data);
    },
  };
}

/**
 * Where the container's private channel stands: not announced yet, given its
 * port, or ended because the page announced again as a new document.
 */
export type ContainerState = "waiting" | "connected" | "lost";

export interface ContainerChannel {
  readonly state: ContainerState;
  dispose(): void;
}

/**
 * Answer the container's announcement with a private port.
 *
 * Only the product frame's own window is heard, and only from the origin the
 * product was opened at. The port goes back to exactly that origin, once. A page
 * that was not opened as a container page never gets one.
 *
 * The container announces once per document. A second announcement therefore
 * means the frame loaded a new document: a link, a form post or a script
 * navigated it. That document is not answered, because its container cannot be
 * told apart from a page that only imitates one. The private port is closed
 * instead, and the channel stays lost until the product is opened again. A
 * single-page navigation loads no document and changes nothing.
 */
export function attachContainerChannel(options: {
  frame: Pick<HTMLIFrameElement, "contentWindow">;
  origin: string;
  router: FrameRouter;
  onConnected?(): void;
  onLost?(): void;
  /** Where announcements arrive. The page's window, unless a test supplies one. */
  events?: Pick<Window, "addEventListener" | "removeEventListener">;
}): ContainerChannel {
  const { frame, origin, router, onConnected, onLost } = options;
  const events = options.events ?? window;
  let state: ContainerState = "waiting";
  let container: MessagePort | undefined;

  const release = (): void => {
    router.setContainer(undefined);
    container?.close();
    container = undefined;
  };

  const onMessage = (event: MessageEvent): void => {
    if (state === "lost") return;
    if (event.source !== frame.contentWindow || event.origin !== origin) return;
    if ((event.data as { type?: unknown } | null)?.type !== CONTAINER_READY)
      return;
    if (state === "connected") {
      state = "lost";
      release();
      onLost?.();
      return;
    }
    const channel = new MessageChannel();
    container = channel.port1;
    container.onmessage = (message: MessageEvent) =>
      router.fromContainer(message.data);
    router.setContainer((frame) => container?.postMessage(frame));
    state = "connected";
    frame.contentWindow?.postMessage({ type: CONTAINER_INIT }, origin, [
      channel.port2,
    ]);
    onConnected?.();
  };
  events.addEventListener("message", onMessage as EventListener);

  return {
    get state() {
      return state;
    },
    dispose() {
      events.removeEventListener("message", onMessage as EventListener);
      release();
    },
  };
}
