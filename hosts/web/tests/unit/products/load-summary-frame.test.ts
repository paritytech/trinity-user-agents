import { afterAll, describe, expect, mock, test } from "bun:test";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import {
  PRODUCT_LOAD,
  type ProductLoadSummary,
} from "../../../src/products/load-observer.js";

const ORIGIN = "https://host.test";

/** The frame `openProduct` embeds: its window is a port, so a message can name it as its source. */
const frames: { contentWindow: MessagePort }[] = [];
const real = await import("@parity/truapi-host/web");
mock.module("@parity/truapi-host/web", () => ({
  ...real,
  createIframeHost: () => {
    const iframe = {
      contentWindow: new MessageChannel().port1,
      style: {},
      addEventListener() {},
      removeEventListener() {},
    };
    frames.push(iframe);
    return { iframe, dispose() {} };
  },
}));
const page = new EventTarget();
const hadWindow = "window" in globalThis;
Object.assign(globalThis, {
  window: Object.assign(page, { location: { origin: ORIGIN } }),
});
afterAll(() => {
  if (!hadWindow) delete (globalThis as { window?: unknown }).window;
});
const { openProduct } = await import("../../../src/products/product.js");

const runtime = {
  createProvider: async () => ({
    subscribe: () => () => {},
    postMessage() {},
    dispose() {},
  }),
} as unknown as WorkerPairingHostRuntime;

const summary = (totalMs: number): ProductLoadSummary => ({
  outcome: "fetched",
  totalMs,
});

function post(
  source: MessagePort,
  data: unknown,
  origin: string = ORIGIN,
): void {
  page.dispatchEvent(new MessageEvent("message", { data, origin, source }));
}

async function open(heard: ProductLoadSummary[], onClose?: () => void) {
  const product = await openProduct(
    runtime,
    new URL(`${ORIGIN}/truapi-sandbox/index.html`),
    "one.paseo",
    {} as HTMLElement,
    {
      mounted: true,
      onLoadSummary: (got) => heard.push(got),
      ...(onClose === undefined ? {} : { onClose }),
    },
  );
  return { product, frame: frames.at(-1)!.contentWindow };
}

describe("a loader's summary reaches the host", () => {
  // The summary is the frame's own: a name inside it, another frame, or the
  // product the loader hands the frame to cannot speak for the load.
  test("from its own frame, on this origin, once", async () => {
    const heard: ProductLoadSummary[] = [];
    const { product, frame } = await open(heard);
    const other = new MessageChannel().port1;

    post(other, { type: PRODUCT_LOAD, summary: summary(1) });
    post(
      frame,
      { type: PRODUCT_LOAD, summary: summary(2) },
      "https://elsewhere.test",
    );
    post(frame, { type: PRODUCT_LOAD, summary: { product: "two.paseo" } });
    post(frame, { type: PRODUCT_LOAD, summary: summary(3) });
    post(frame, { type: PRODUCT_LOAD, summary: summary(4) });

    expect(heard).toEqual([summary(3)]);
    product.dispose();
  });

  // A load whose frame closes before its loader reports must not wait forever.
  test("is told once when the frame closes", async () => {
    let closes = 0;
    const { product } = await open([], () => (closes += 1));
    expect(closes).toBe(0);
    product.dispose();
    product.dispose();
    expect(closes).toBe(1);
  });

  test("not after the frame is closed", async () => {
    const heard: ProductLoadSummary[] = [];
    const { product, frame } = await open(heard);
    product.dispose();
    post(frame, { type: PRODUCT_LOAD, summary: summary(5) });
    expect(heard).toEqual([]);
  });
});
