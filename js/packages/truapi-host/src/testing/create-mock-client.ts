// One call to get a product client talking to a real core over a mocked host.
//
// The Playwright fixture drives a product that lives in an iframe, which is
// what an end-to-end suite needs. This is the other shape: no iframe, no
// browser automation, both ends of the wire in one process. A product's own
// unit tests get a `TrUApiClient` that behaves like the production one --
// same transport, same SCALE frames, same dispatcher -- with the host seam
// mocked underneath and its recordings available for assertions.
//
//   client  --MessagePort-->  core (WASM)  -->  createMockHost callbacks
//     ^                                              |
//     +---------------- assertions on ---------------+
//
// It uses a real `MessageChannel`, so the wire is the production transport
// rather than a direct function call. What it does NOT exercise is the iframe
// boundary and the browser's origin checks; the fixture covers those.

import {
  createClient,
  createMessagePortProvider,
  createTransport,
} from "@parity/truapi";
import type { TrUApiClient, TrUApiTransport } from "@parity/truapi";

import {
  createMockHost,
  mockRuntimeConfig,
  type MockHost,
  type MockHostConfig,
} from "../web/create-mock-host.js";
import type { ProductRuntimeConfig } from "../runtime.js";
import {
  createChatHostQueries,
  encodeChatEvent,
  startWasmChatQuery,
  type ChatHostRuntime,
  type WasmChatQueryCore,
} from "../chat-queries.js";
import {
  resolveAccount,
  type DevAccount,
  type DevAccountName,
} from "./dev-accounts.js";

/** Options for {@link createMockClient}. */
export interface MockClientOptions {
  /** Behaviour knobs forwarded to {@link createMockHost}. */
  mock?: MockHostConfig;
  /**
   * A mock host to use in place of a new one. A second client on the host of
   * a first one plays a product restart: the rooms, storage and session data
   * of the host stay. `mock` is then ignored.
   */
  host?: MockHost;
  /** Overrides merged into {@link mockRuntimeConfig}. */
  runtimeConfig?: Partial<ProductRuntimeConfig>;
  /**
   * Keep preimage submissions in the core instead of the Bulletin chain, so
   * `preimage.submit` answers without a chain. Defaults to `false`: the mock
   * chain is silent, and a submission waits.
   */
  submitPreimagesLocally?: boolean;
  /**
   * Answer resource allocation, such as the Bulletin allowance of a preimage
   * submission, as granted without an allocation on chain. Defaults to
   * `false`, so an allocation waits on the silent mock chain.
   */
  grantAllowances?: boolean;
  /** Account the host signs as. Defaults to `"alice"`. */
  account?: DevAccountName | DevAccount;
  /**
   * Resolved WASM entry.
   *
   * Defaults to the signing-enabled `testing` bundle resolved against this
   * module, which is what a product's own test run needs: unlike the browser
   * host page, there is no server here mapping `/wasm/` onto disk.
   */
  wasmUrl?: string;
}

/** A product client wired to a mock host, plus the mock for assertions. */
export interface MockClient {
  /** The client a product uses -- the same object it gets in production. */
  client: TrUApiClient;
  /**
   * The transport under `client`, for a suite that sends frames the client
   * does not make, such as a request at an earlier protocol version.
   */
  transport: TrUApiTransport;
  /** The mock host, exposing its recordings and control surface. */
  host: MockHost;
  /**
   * The chat surface of the host runtime: the six chat queries and
   * `publishChatEvent`. The client attaches it to `host.chatUser`, so the
   * mock host can play the user against this product.
   */
  chat: ChatHostRuntime;
  /** Tear down the core, the channel and the client. */
  dispose(): void;
}

/**
 * Build a product client against a mock host and a real core.
 *
 * ```ts
 * const { client, host, dispose } = await createMockClient();
 * await client.navigation.navigateTo({ url: "https://example.invalid" });
 * expect(host.getNavigationLog()).toEqual(["https://example.invalid"]);
 * dispose();
 * ```
 */
export async function createMockClient(
  options: MockClientOptions = {},
): Promise<MockClient> {
  const wasmUrl =
    options.wasmUrl ??
    new URL("../../dist/wasm/testing/truapi_server.js", import.meta.url).href;
  const glue = (await import(/* @vite-ignore */ wasmUrl)) as {
    default: () => Promise<unknown>;
    WasmSigningHostRuntime: new (
      callbacks: unknown,
      config: unknown,
    ) => SigningRuntime;
  };
  await glue.default();

  const { createWasmRawCallbacks } =
    await import("../generated/host-callbacks-adapter.js");

  const host = options.host ?? createMockHost(options.mock);
  const { productId, executionKind, ...hostConfig } = mockRuntimeConfig(
    options.runtimeConfig ?? {},
  );
  const runtime = new glue.WasmSigningHostRuntime(
    {
      // Supplied outside the generated adapter, as the worker runtime does.
      ...createWasmRawCallbacks(host.callbacks),
      workerDemandChanged: () => {},
    },
    hostConfig,
  );

  if (options.submitPreimagesLocally) runtime.setSubmitPreimagesLocally(true);
  if (options.grantAllowances) runtime.setGrantAllowancesUnchecked(true);

  const account = resolveAccount(options.account ?? "alice");
  await runtime.activateLocalSession(account.entropy);

  // Both ends in one process, but still over a real MessagePort: the frames
  // crossing here are the same bytes that cross an iframe boundary.
  const channel = new MessageChannel();
  const core = runtime.productRuntime(
    // A chat product runs as a Worker; the default is an App.
    executionKind === undefined ? { productId } : { productId, executionKind },
    {
      emitFrame(frame: Uint8Array) {
        channel.port1.postMessage(frame);
      },
    },
  );
  channel.port1.onmessage = (event: MessageEvent) => {
    const frame = event.data;
    if (frame instanceof Uint8Array) void core.receiveFrame(frame);
  };
  channel.port1.start();

  const provider = createMessagePortProvider(channel.port2);
  const transport = createTransport(provider);
  const client = createClient(transport);

  // The queries call the core in this thread, as the worker runtime does in
  // its own thread.
  const chat: ChatHostRuntime = {
    ...createChatHostQueries((name, request, sink) =>
      startWasmChatQuery(core, name, request, sink),
    ),
    async publishChatEvent(item) {
      core.publishChatEvent(encodeChatEvent(item));
    },
  };
  const detachChat = host.chatUser.attach(chat);

  return {
    client,
    transport,
    host,
    chat,
    dispose() {
      detachChat();
      provider.dispose();
      core.dispose();
      channel.port1.close();
      channel.port2.close();
    },
  };
}

/** The subset of the generated signing runtime this module drives. */
interface SigningRuntime {
  setSubmitPreimagesLocally(local: boolean): void;
  setGrantAllowancesUnchecked(granted: boolean): void;
  activateLocalSession(secret: Uint8Array): Promise<void>;
  productRuntime(
    product: { productId: string; executionKind?: string },
    sink: { emitFrame(frame: Uint8Array): void },
  ): ProductCore;
}

/** The subset of a per-product core this module drives. */
interface ProductCore extends WasmChatQueryCore {
  receiveFrame(frame: Uint8Array): Promise<void>;
  /** Throws when the connection may not reach Chat. */
  publishChatEvent(event: Uint8Array): void;
  dispose(): void;
}
