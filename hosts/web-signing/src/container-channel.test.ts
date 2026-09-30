import { describe, expect, test } from "bun:test";
import { encodeWireMessage } from "@parity/truapi";
import {
  attachContainerChannel,
  CONTAINER_INIT,
  CONTAINER_READY,
  createFrameRouter,
  PRIVATE_PREFIX,
  type Dropped,
} from "./container-channel.js";

/** A wire frame with the given request id and an empty payload. */
function frame(requestId: string): Uint8Array {
  const encoded = encodeWireMessage({
    requestId,
    payload: {
      traitId: 1,
      methodId: 2,
      messageType: 3,
      value: new Uint8Array(),
    },
  });
  if (encoded.isErr()) throw encoded.error;
  return encoded.value;
}

function router() {
  const log = {
    core: [] as Uint8Array[],
    product: [] as Uint8Array[],
    container: [] as Uint8Array[],
    dropped: [] as Dropped[],
    tapped: [] as string[],
  };
  const r = createFrameRouter({
    toCore: (f) => log.core.push(f),
    toProduct: (f) => log.product.push(f),
    tap: (direction) => log.tapped.push(direction),
    onDrop: (reason) => log.dropped.push(reason),
  });
  r.setContainer((f) => log.container.push(f));
  return { r, log };
}

describe("frame router", () => {
  test("keeps the container's replies away from the product", () => {
    const { r, log } = router();
    r.fromCore(frame(`${PRIVATE_PREFIX}1`));
    r.fromCore(frame("p:1"));
    expect(log.container).toEqual([frame(`${PRIVATE_PREFIX}1`)]);
    expect(log.product).toEqual([frame("p:1")]);
  });

  // Otherwise a product could answer or ask as the container: send its own
  // authorization request, or read the container's replies from a shared port.
  test("drops a product frame that claims a private request id", () => {
    const { r, log } = router();
    r.fromProduct(frame(`${PRIVATE_PREFIX}1`));
    r.fromProduct(frame("p:2"));
    expect(log.core).toEqual([frame("p:2")]);
    expect(log.dropped).toEqual(["product-private"]);
  });

  test("drops a container frame that is not private", () => {
    const { r, log } = router();
    r.fromContainer(frame("p:3"));
    r.fromContainer(frame(`${PRIVATE_PREFIX}4`));
    expect(log.core).toEqual([frame(`${PRIVATE_PREFIX}4`)]);
    expect(log.dropped).toEqual(["container-public"]);
  });

  test("drops frames it cannot decode and anything that is not bytes", () => {
    const { r, log } = router();
    r.fromProduct(Uint8Array.of(0xff, 0xff, 0xff));
    r.fromContainer(Uint8Array.of(0xff));
    r.fromProduct("text");
    r.fromContainer({ length: 4 });
    r.fromProduct(new ArrayBuffer(8));
    expect(log.core).toEqual([]);
    expect(log.dropped).toEqual(["undecodable", "undecodable"]);
  });

  // A private reply must not fall through to the product when nothing is
  // listening on the container side yet.
  test("does not hand a private reply to the product when there is no container", () => {
    const log = { product: [] as Uint8Array[] };
    const r = createFrameRouter({
      toCore() {},
      toProduct: (f) => log.product.push(f),
    });
    r.fromCore(frame(`${PRIVATE_PREFIX}9`));
    expect(log.product).toEqual([]);
  });

  test("shows the inspector both directions, private traffic included", () => {
    const { r, log } = router();
    r.fromProduct(frame("p:1"));
    r.fromCore(frame("p:1"));
    r.fromContainer(frame(`${PRIVATE_PREFIX}1`));
    r.fromCore(frame(`${PRIVATE_PREFIX}1`));
    expect(log.tapped).toEqual(["out", "in", "out", "in"]);
  });
});

const ORIGIN = "http://product.localhost:5173";

/** A product frame, the page's message events, and a router that records where frames go. */
function attached() {
  const listeners = new Set<(event: MessageEvent) => void>();
  const granted: { message: unknown; origin: string; ports: MessagePort[] }[] =
    [];
  const contentWindow = {
    postMessage(message: unknown, origin: string, ports: MessagePort[]) {
      granted.push({ message, origin, ports });
    },
  };
  const toCore: Uint8Array[] = [];
  const router = createFrameRouter({
    toCore: (f) => toCore.push(f),
    toProduct() {},
  });
  const seen = { connected: 0, lost: 0 };
  const channel = attachContainerChannel({
    frame: { contentWindow: contentWindow as unknown as Window },
    origin: ORIGIN,
    router,
    onConnected: () => seen.connected++,
    onLost: () => seen.lost++,
    events: {
      addEventListener: (
        _type: string,
        listener: EventListenerOrEventListenerObject,
      ) => listeners.add(listener as (event: MessageEvent) => void),
      removeEventListener: (
        _type: string,
        listener: EventListenerOrEventListenerObject,
      ) => listeners.delete(listener as (event: MessageEvent) => void),
    },
  });
  function announce(
    from: { source?: unknown; origin?: string; data?: unknown } = {},
  ): void {
    const event = {
      source: "source" in from ? from.source : contentWindow,
      origin: from.origin ?? ORIGIN,
      data: "data" in from ? from.data : { type: CONTAINER_READY },
    } as MessageEvent;
    for (const listener of [...listeners]) listener(event);
  }
  /** Lets messages queued on a port arrive. */
  const wait = () => new Promise((resolve) => setTimeout(resolve, 10));
  return {
    channel,
    router,
    granted,
    seen,
    announce,
    toCore,
    listeners,
    wait,
  };
}

describe("container channel", () => {
  test("gives the private port to the frame's own window, at the origin it opened, once", () => {
    const { channel, granted, seen, announce } = attached();
    expect(channel.state).toBe("waiting");
    announce();
    expect(channel.state).toBe("connected");
    expect(seen).toEqual({ connected: 1, lost: 0 });
    expect(
      granted.map(({ message, origin, ports }) => [
        message,
        origin,
        ports.length,
      ]),
    ).toEqual([[{ type: CONTAINER_INIT }, ORIGIN, 1]]);
  });

  // The port is a capability to ask the core for permissions; any other
  // sender would be a page or subframe that must not hold it.
  test("answers nothing that is not the container's announcement from that frame", () => {
    const { channel, granted, seen, announce } = attached();
    announce({ source: {} });
    announce({ origin: "http://other.localhost:5173" });
    announce({ data: { type: "truapi-ready" } });
    announce({ data: null });
    expect(channel.state).toBe("waiting");
    expect(granted).toEqual([]);
    expect(seen).toEqual({ connected: 0, lost: 0 });
  });

  // A frame that navigates loads a new document with a new container. Its
  // announcement cannot be told from a page that imitates one, so it is not
  // answered and the old port stops working.
  test("ends the channel when the frame announces again, and gives the new document no port", async () => {
    const { channel, router, granted, seen, announce, toCore, wait } =
      attached();
    announce();
    const [container] = granted[0].ports;
    const received: Uint8Array[] = [];
    container.onmessage = (event) => received.push(event.data);
    router.fromCore(frame(`${PRIVATE_PREFIX}1`));
    container.postMessage(frame(`${PRIVATE_PREFIX}2`));
    await wait();
    expect(received).toEqual([frame(`${PRIVATE_PREFIX}1`)]);
    expect(toCore).toEqual([frame(`${PRIVATE_PREFIX}2`)]);

    announce();

    expect(channel.state).toBe("lost");
    expect(seen).toEqual({ connected: 1, lost: 1 });
    expect(granted).toHaveLength(1);
    router.fromCore(frame(`${PRIVATE_PREFIX}3`));
    container.postMessage(frame(`${PRIVATE_PREFIX}4`));
    await wait();
    expect(received).toEqual([frame(`${PRIVATE_PREFIX}1`)]);
    expect(toCore).toEqual([frame(`${PRIVATE_PREFIX}2`)]);
  });

  test("stays lost: further announcements are neither answered nor reported again", () => {
    const { channel, granted, seen, announce } = attached();
    announce();
    announce();
    announce();
    announce();
    expect(channel.state).toBe("lost");
    expect(granted).toHaveLength(1);
    expect(seen).toEqual({ connected: 1, lost: 1 });
  });

  // The expected order for a page: one announcement per document. Nothing here
  // may look like a navigation, or a healthy product would be closed.
  test("keeps the channel while the page sends nothing more", () => {
    const { channel, seen, announce } = attached();
    announce();
    announce({ data: { type: "truapi-ready" } });
    announce({ origin: "http://other.localhost:5173" });
    expect(channel.state).toBe("connected");
    expect(seen.lost).toBe(0);
  });

  test("stops listening once disposed", () => {
    const { channel, granted, listeners, announce } = attached();
    channel.dispose();
    announce();
    expect(listeners.size).toBe(0);
    expect(granted).toEqual([]);
  });
});
