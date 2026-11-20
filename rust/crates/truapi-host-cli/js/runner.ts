import { pathToFileURL } from "node:url";
import { inspect } from "node:util";
import {
  asProductId,
  type ProductId,
  type ProductAccountId,
  type TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import { getClientSync } from "../../../../js/packages/truapi/src/sandbox.ts";
import { createHostConnection } from "../../../../js/container/src/host-connection.ts";
import { createPermissionAuthorization } from "../../../../js/container/src/network-transport.ts";
import { freezePermissionRuntime } from "../../../../js/container/src/permission-runtime.ts";
import { installFetchGate } from "../../../../js/container/src/network.ts";
import { installWebSocketGate } from "../../../../js/container/src/websocket.ts";
import { installXhrGate } from "../../../../js/container/src/xhr.ts";
import {
  freezeValue,
  reportLockdownFailures,
} from "../../../../js/container/src/freeze.ts";
import { createFrameProviderFactory } from "./ws-provider.ts";

/// The host context injected alongside `truapi`. It only exposes what a script
/// can't get from `truapi` alone: the product id the host serves, so product
/// accounts stay in sync with `--product-id` (hardcoding a mismatched id fails
/// signing with `PermissionDenied`). Use `console.log` / `throw` for the rest.
export interface HostContext {
  /** The product id this host serves (its `--product-id`). */
  productId: ProductId;
  /** A product account id for `derivationIndex` (default 0) under this product. */
  productAccount(index?: number): ProductAccountId;
}

declare global {
  // eslint-disable-next-line no-var
  var truapi: TrUApiClient;
  // eslint-disable-next-line no-var
  var host: HostContext;
  // Playground examples receive this helper from `runExample`; expose the
  // same contract to directly imported CLI scripts.
  // eslint-disable-next-line no-var
  var assert: (condition: unknown, ...message: unknown[]) => asserts condition;
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} must be set`);
  return value;
}

async function main() {
  const frameUrl = requireEnv("TRUAPI_FRAME_URL");
  const productId = asProductId(requireEnv("TRUAPI_PRODUCT_ID"));
  const scriptPath = requireEnv("TRUAPI_SCRIPT");
  const connection = createHostConnection(
    frameUrl,
    createFrameProviderFactory(),
  );
  const context: HostContext = {
    productId,
    productAccount: (index = 0) => ({
      dotNsIdentifier: productId,
      derivationIndex: { tag: "Index", value: index },
    }),
  };
  globalThis.host = context;
  globalThis.assert = (condition: unknown, ...message: unknown[]) => {
    if (condition) return;
    const detail = message
      .map((value) =>
        typeof value === "string"
          ? value
          : inspect(value, { colors: false, depth: 5 }),
      )
      .join(" ");
    throw new Error(detail || "assertion failed");
  };

  freezePermissionRuntime();
  freezeValue(globalThis, "window", globalThis);
  freezeValue(globalThis, "top", globalThis);
  freezeValue(globalThis, "__HOST_WEBVIEW_MARK__", true);
  freezeValue(
    globalThis,
    "__HOST_API_TRANSPORT__",
    Object.freeze(connection.transport),
  );
  Object.defineProperty(globalThis, "__HOST_API_PORT__", {
    get: () => connection.legacyPort,
    set() {},
    configurable: false,
  });

  const authorization = createPermissionAuthorization(
    globalThis as Window & typeof globalThis,
    connection.internal,
  );
  installFetchGate(globalThis, authorization.network);
  installWebSocketGate(globalThis, authorization.network);
  installXhrGate(globalThis, authorization.network);
  reportLockdownFailures();
  const client = getClientSync();
  if (!client) throw new Error("No client over the injected host transport");
  globalThis.truapi = client;

  const timer = setTimeout(() => {
    console.error(`[runner] timed out connecting to ${frameUrl}`);
    process.exit(2);
  }, 15_000);
  try {
    const handshake = await client.system.handshake();
    if (handshake.isErr())
      throw new Error("Host connection failed", { cause: handshake.error });
    clearTimeout(timer);
    if (process.env.TRUAPI_SCRIPT_CWD)
      process.chdir(process.env.TRUAPI_SCRIPT_CWD);
    const module = await import(pathToFileURL(scriptPath).href);
    if (typeof module.default === "function") await module.default(context);
    // A Worker execution runs until the host stops it or drops its
    // connection, as a host's worker runtime keeps one, whether or not its
    // module waits for that itself.
    if (process.env.TRUAPI_SCRIPT_EXECUTION === "worker")
      await new Promise<void>((resolve) =>
        connection.subscribeConnectionStatus((status) => {
          if (status === "disconnected") resolve();
        }),
      );
  } finally {
    clearTimeout(timer);
    connection.dispose();
  }
}

main().then(
  () => process.exit(0),
  (error) => {
    const message = String(error);
    const detail = inspect(error, { colors: false, depth: 5 });
    console.error(
      `[script error] ${detail.includes(message) ? detail : `${message}\n${detail}`}`,
    );
    process.exit(1);
  },
);
