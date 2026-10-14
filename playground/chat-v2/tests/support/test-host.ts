/**
 * The sample product on the test host: the client of `createMockClient` from
 * `@parity/truapi-host/testing/client`, which runs the real core (WASM) over a
 * mock host, given to the same `startSampleProduct` the Worker calls. The mock
 * host plays the user through `host.chatUser`.
 *
 * The suites skip when the WASM bundles of `@parity/truapi-host` are not
 * built, and `REQUIRE_WASM=1` makes that a failure, as in the package.
 */
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { ChatMessage, ChatMessageBody } from "@parity/truapi";
import type { MockClient } from "@parity/truapi-host/testing/client";
import type { MockHost } from "@parity/truapi-host/testing";
import { bytesToHex } from "../../worker/bytes";
import type { Assets } from "../../worker/fixtures";
import { startSampleProduct, type SampleProduct } from "../../worker/product";

/** The WASM entry the test client loads. */
function testingWasm(): string | undefined {
  try {
    return fileURLToPath(import.meta.resolve("@parity/truapi-host/wasm/testing"));
  } catch {
    return undefined;
  }
}

/** Whether the test host can run. Throws under `REQUIRE_WASM=1` when it cannot. */
export function testHostIsBuilt(): boolean {
  const wasm = testingWasm();
  const client = (() => {
    try {
      return fileURLToPath(import.meta.resolve("@parity/truapi-host/testing/client"));
    } catch {
      return undefined;
    }
  })();
  const missing = [wasm, client].filter((path) => path === undefined || !existsSync(path));
  if (missing.length === 0) return true;
  if (process.env.REQUIRE_WASM === "1") {
    throw new Error(
      "REQUIRE_WASM=1 but @parity/truapi-host is not built with its WASM bundles. " +
        "Run `make wasm` and `npm run build` in js/packages/truapi-host first.",
    );
  }
  return false;
}

const ASSETS = new URL("../../assets/", import.meta.url);

function asset(name: string) {
  return bytesToHex(new Uint8Array(readFileSync(new URL(name, ASSETS))));
}

/** The real assets, so the core screens the same bytes a host receives. */
export function loadTestAssets(): Assets {
  return {
    avatar: asset("avatar.png"),
    avatarAlt: asset("avatar-alt.png"),
    badge: asset("badge.png"),
    icons: [1, 2, 3, 4, 5, 6].map((index) => asset(`icon-${index}.png`)),
    samplePng: asset("sample.png"),
    samplePdf: asset("sample.pdf"),
    sampleMp4: asset("sample.mp4"),
  };
}

/** The DotNS name the links of the product use in the suites. */
export const PRODUCT = "sample.dot";

export interface Running extends MockClient {
  product: SampleProduct;
  /** The event log lines of the product, parsed. */
  log(): Record<string, unknown>[];
}

/**
 * Start the sample product on a new test host, or on `host` to play a worker
 * restart. Resolves once the rooms exist and the product is idle.
 */
export async function startOnTestHost(options: { host?: MockHost } = {}): Promise<Running> {
  const { createMockClient } = await import("@parity/truapi-host/testing/client");
  const mock = await createMockClient({
    ...(options.host ? { host: options.host } : {}),
    // Chat is a Worker service.
    runtimeConfig: { executionKind: "Worker" },
    // The product submits a preimage at start, for `/post rich`. The mock
    // chain never answers, so the core keeps the preimage and grants the
    // Bulletin allowance itself.
    submitPreimagesLocally: true,
    grantAllowances: true,
  });
  // Short waits, so that the time-out and retry paths run fast.
  mock.host.chatUser.setTimings({ retryDelay: 100 });
  const lines: string[] = [];
  const product = startSampleProduct({
    client: mock.client,
    assets: loadTestAssets(),
    product: PRODUCT,
    logSink: (line) => lines.push(line),
    crash: () => {},
  });
  const log = () => lines.map((line) => JSON.parse(line) as Record<string, unknown>);
  await product.rooms;
  await product.idle();
  // The start ends with the preimage line. A suite that stops the product
  // before it would leave the submission open.
  await until(() => log().some((line) => line.via === "preimage"), "the preimage of the start");
  return { ...mock, product, log };
}

/** Wait until `check` gives a value that is not `undefined` or `false`. */
export async function until<T>(
  check: () => T | undefined | false,
  what = "the condition",
  timeoutMs = 5_000,
): Promise<T> {
  const end = Date.now() + timeoutMs;
  for (;;) {
    const value = check();
    if (value !== undefined && value !== false) return value;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

/** The text of a body, or `undefined` for a body with no text. */
export function bodyText(body: ChatMessageBody): string | undefined {
  switch (body.tag) {
    case "Text":
      return body.value.text;
    case "RichText":
    case "Actions":
      return body.value.text;
    case "Custom":
      return body.value.previewText;
  }
}

/** The texts of the messages of a room, oldest first. */
export function texts(host: MockHost, roomId: string): (string | undefined)[] {
  return host.feed(roomId).map((message) => bodyText(message.body));
}

/**
 * Wait for a message of a room whose text is `text`, or matches it, and give
 * the newest such message.
 */
export function waitForText(host: MockHost, roomId: string, text: string | RegExp): Promise<ChatMessage> {
  const matches = (value: string | undefined) =>
    value !== undefined && (typeof text === "string" ? value === text : text.test(value));
  return until(
    () =>
      host
        .feed(roomId)
        .filter((message) => matches(bodyText(message.body)))
        .at(-1),
    `${String(text)} in ${roomId}`,
  );
}

/** The newest product message of a room. */
export function newestProductMessage(host: MockHost, roomId: string): ChatMessage {
  const message = host
    .feed(roomId)
    .filter((each) => each.origin.participantId === "product")
    .at(-1);
  if (!message) throw new Error(`no product message in ${roomId}`);
  return message;
}
