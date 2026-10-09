// Browser half of the test host: the page a Playwright fixture drives.
//
// It embeds the product in an iframe, runs a real truapi core against
// `createMockHost`'s callbacks, and publishes the mock's control surface on
// `window.__TRUAPI_TEST_HOST__` so the fixture can reach it through
// `page.evaluate`.
//
//   Playwright (node)                    this page (browser)
//   +------------------+                 +----------------------------------+
//   | fixture          |  page.evaluate  | window.__TRUAPI_TEST_HOST__      |
//   | testHost.*       |---------------->| createMockHost() control surface |
//   +------------------+                 +----------------------------------+
//                                             ^ callbacks
//                                        +----------------------------------+
//                                        | truapi WASM core                 |
//                                        +----------------------------------+
//                                             ^ SCALE frames over MessagePort
//                                        +----------------------------------+
//                                        | product iframe (#product-frame)  |
//                                        +----------------------------------+
//
// The core runs on the page's main thread rather than in a Worker: a test host
// has no UI to keep responsive, and one less moving part is one less thing to
// debug when a suite fails.

import { DerivationIndex } from "@parity/truapi";

import { createIframeHost } from "../web/create-iframe-host.js";
import { createWebWorkerPairingHostRuntime } from "../web/create-worker-host-runtime.js";
import {
  createMockHost,
  mockRuntimeConfig,
  type MockHost,
  type MockHostConfig,
} from "../web/create-mock-host.js";
import type { ProductRuntimeConfig } from "../runtime.js";
import {
  checkDerivationIndex,
  resolveAccount,
  type DevAccount,
  type DevAccountName,
} from "./dev-accounts.js";

/**
 * Id the test host gives the product iframe.
 *
 * The Playwright fixture's `productFrame()` resolves against this, so the two
 * must agree; it is exported rather than duplicated as a string literal.
 */
export const PRODUCT_FRAME_ID = "product-frame";

/** Options for {@link startTestHost}. */
export interface TestHostPageOptions {
  /** URL of the product under test. */
  productUrl: string;
  /** Element the product iframe is appended to. */
  container: HTMLElement;
  /** Behaviour knobs forwarded to {@link createMockHost}. */
  mock?: MockHostConfig;
  /** Overrides merged into {@link mockRuntimeConfig}. */
  runtimeConfig?: Partial<ProductRuntimeConfig>;
  /**
   * Resolved WASM entry. Defaults to the signing-enabled `testing` bundle,
   * which is what lets the host own dev accounts instead of waiting on a
   * wallet that is not there.
   */
  wasmUrl?: string;
  /** Accounts the host can sign as. Defaults to `["alice"]`. */
  accounts?: (DevAccountName | DevAccount)[];
  /**
   * Whether the host activates a session at boot.
   *
   * `"auto"` (the default) starts signed in, which is what most suites want.
   * `"manual"` boots with no session so a test can drive the signed-out path;
   * call `switchAccount` to sign in.
   */
  loginBehavior?: LoginBehavior;
  /**
   * Where the core runs.
   *
   * `"worker"` (the default) matches production: web hosts run the core in a
   * Web Worker. `"main-thread"` keeps it on the page, which is simpler to
   * debug but is not a topology any real host uses.
   */
  topology?: "worker" | "main-thread";
  /** URL of the worker script. Defaults to what the test host server serves. */
  workerUrl?: string;
  /**
   * How resource allocation is answered.
   *
   * `"granted"` (the default) answers every request as allocated without
   * performing it, so a suite can exercise a product's allowance-dependent
   * paths with no on-chain personhood identity. Nothing is allocated: a green
   * run says the product handles a grant, not that a host would have given one.
   *
   * `"chain"` runs the real allocation -- ring membership, slot, proof,
   * extrinsic -- against the chains the host serves, and fails where a real
   * host would.
   */
  allowances?: "granted" | "chain";
  /**
   * Resource tags answered as refused, whatever `allowances` would otherwise
   * say. Applied before the product loads, because a product asks for its
   * resources on connect and a later call would land after that.
   */
  withheldResources?: string[];
  /**
   * Core log level (`off`/`error`/`warn`/`info`/`debug`/`trace`).
   *
   * The core logs why a call failed before mapping it to a protocol answer,
   * so raising this is what turns an opaque outcome into its reason. Under
   * `"worker"` those lines go to the worker console, which Playwright's
   * `page.on("console")` does not observe; pair this with
   * `topology: "main-thread"` to read them from a test.
   */
  logLevel?: string;
}

/** How the test host answers login at boot. */
export type LoginBehavior = "auto" | "manual";

/**
 * Account control, which lives on the runtime rather than the platform.
 *
 * A TrUAPI host derives accounts from session entropy, so switching account
 * means re-activating the session -- it is not a host callback the mock can
 * answer, which is why these sit alongside the mock's surface rather than in
 * it.
 */
export interface AccountControl {
  /** Names the host can currently sign as, in order. */
  getAccounts(): string[];
  /** The account the current session is activated from, if any. */
  getActiveAccount(): string | undefined;
  /**
   * Deliver a host-authored Chat action to the product, the way a posted
   * message or a tapped `Actions` button reaches it.
   *
   * Rejects when no product is connected: there is no stream to publish into.
   */
  injectChatAction(action: unknown): Promise<void>;
  /** Re-activate the session as `name`. */
  switchAccount(name: string): Promise<void>;
  /** Replace the roster, activating the first entry. */
  setAccounts(names: (DevAccountName | DevAccount)[]): Promise<void>;
  /** Drop the session, leaving the host signed out. */
  signOut(): Promise<void>;
  /**
   * The SS58 address of a product account, at the prefix the core mandates.
   *
   * Derived from the active session's root, so it answers `undefined` while
   * the host is signed out and a different address after `switchAccount`. This
   * is what a suite funds, or asserts on, without reading it out of the
   * product's own UI.
   *
   * Encoded at the prefix the core mandates, which is not necessarily the one
   * the product displays: a product rendering at another prefix shows a
   * different string for the same account. Compare against a product's own
   * rendering by decoding both, and pass this to a faucet or a transfer as it
   * stands.
   */
  getProductAccountAddress(
    productId?: string,
    index?: number,
  ): Promise<string | undefined>;
}

/** What the fixture reaches on `window.__TRUAPI_TEST_HOST__`. */
export type TestHostControl = MockHost & AccountControl;

/** The running test host. */
export interface TestHostPage {
  /** The control surface the fixture reaches. */
  host: TestHostControl;
  /** The embedded product iframe. */
  iframe: HTMLIFrameElement;
  /** Tear down the iframe, the core and the channel. */
  dispose(): void;
}

declare global {
  interface Window {
    /** Published for the Playwright fixture; see the module comment. */
    __TRUAPI_TEST_HOST__?: TestHostControl;
    /**
     * The same object under the name `@parity/host-api-test-sdk` publishes.
     * A suite that drives the host page directly, rather than through the
     * fixture, reaches it here under whichever of the two names its
     * `page.evaluate` calls already use.
     */
    __TEST_HOST__?: TestHostControl;
  }
}

/**
 * Publish `control` under both global names, and return the undo.
 *
 * One object under two names, never a copy: a suite reaching the page through
 * either name drives the same mock, and two objects would let them drift.
 */
export function publishTestHostGlobals(control: TestHostControl): () => void {
  window.__TRUAPI_TEST_HOST__ = control;
  window.__TEST_HOST__ = control;
  return () => {
    delete window.__TRUAPI_TEST_HOST__;
    delete window.__TEST_HOST__;
  };
}

/**
 * Boot the test host into `container` and publish its control surface.
 *
 * Resolves once the core is running and the product iframe has its port, so a
 * fixture that awaits this can assume the wire is live.
 */
export async function startTestHost(
  options: TestHostPageOptions,
): Promise<TestHostPage> {
  const host = createMockHost(options.mock);
  const { productId: hostProduct, ...hostConfig } = mockRuntimeConfig(
    options.runtimeConfig ?? {},
  );

  // A signing host, not a pairing host: a test host owns its keys. Note the
  // behavioural consequence -- a signing host answers `request_login` with
  // AlreadyConnected instead of starting a pairing flow, so a suite asserting
  // on pairing UI is asserting on a host role this is not.
  let worker: Worker | undefined;
  // Exactly one of these is set; which one is the topology.
  let workerRuntime: WorkerSigningRuntime | undefined;
  let directRuntime: DirectSigningRuntime | undefined;
  let runtime: {
    activateLocalSession(
      secret: Uint8Array,
      liteUsername?: string,
    ): Promise<void>;
    disconnectSession(): Promise<void>;
  };
  // The two derivation helpers are pure -- they need `default()` and no
  // session -- so the worker topology, which keeps the core off this thread,
  // loads the glue here just to call them.
  const wasmUrl = options.wasmUrl ?? "./wasm/testing/truapi_server.js";
  let derivation: Promise<ProductAccountDerivation> | undefined;
  const deriveHelpers = (): Promise<ProductAccountDerivation> => {
    derivation ??= (async () => {
      const glue = (await import(/* @vite-ignore */ wasmUrl)) as {
        default: () => Promise<unknown>;
      } & ProductAccountDerivation;
      await glue.default();
      return glue;
    })();
    return derivation;
  };

  if ((options.topology ?? "worker") === "worker") {
    // Production topology: the core runs in a Web Worker, reached over the
    // same protocol a real web host uses.
    worker = new Worker(options.workerUrl ?? "/test-host-worker.js", {
      type: "module",
    });
    workerRuntime = (await createWebWorkerPairingHostRuntime(
      worker,
      host.callbacks,
      { hostConfig: hostConfig as never, role: "signing" },
    )) as unknown as WorkerSigningRuntime;
    if (options.logLevel) workerRuntime.setLogLevel?.(options.logLevel);
    if ((options.allowances ?? "granted") === "granted") {
      await workerRuntime.setGrantAllowancesUnchecked?.(true);
    }
    if (options.withheldResources?.length) {
      await workerRuntime.setWithheldResources?.(options.withheldResources);
    }
    runtime = workerRuntime;
  } else {
    const glue = (await import(/* @vite-ignore */ wasmUrl)) as {
      default: () => Promise<unknown>;
      setLogLevel?: (level: string) => void;
      WasmSigningHostRuntime: new (
        callbacks: unknown,
        config: unknown,
      ) => DirectSigningRuntime;
    };
    await glue.default();
    if (options.logLevel) glue.setLogLevel?.(options.logLevel);
    const { createWasmRawCallbacks } = await import(
      "../generated/host-callbacks-adapter.js"
    );
    directRuntime = new glue.WasmSigningHostRuntime(
      {
        // A raw bridge callback rather than a generated host callback, so it
        // is supplied here the way the worker runtime does. The test host runs
        // one product and starts no workers.
        ...createWasmRawCallbacks(host.callbacks),
        workerDemandChanged: () => {},
      },
      hostConfig,
    );
    // The direct core takes a name through a separate entry point, so the
    // shared `activate` above cannot call it directly. Adapt here rather than
    // branching there, so both topologies activate identically.
    if ((options.allowances ?? "granted") === "granted") {
      directRuntime.setGrantAllowancesUnchecked?.(true);
    }
    if (options.withheldResources?.length) {
      directRuntime.setWithheldResources?.(options.withheldResources);
    }
    const direct = directRuntime;
    runtime = {
      activateLocalSession(secret, liteUsername) {
        if (
          liteUsername !== undefined &&
          typeof direct.activateLocalSessionWithIdentity === "function"
        ) {
          return direct.activateLocalSessionWithIdentity(secret, liteUsername);
        }
        return direct.activateLocalSession(secret);
      },
      disconnectSession: () => direct.disconnectSession(),
    };
  }

  let roster: DevAccount[] = (options.accounts ?? ["alice"]).map(resolveAccount);
  let active: DevAccount | undefined;

  const activate = async (account: DevAccount) => {
    if (active) await runtime.disconnectSession();
    // Named, not anonymous: a session with no username makes
    // `account.get_user_id` answer `Unknown`, where a real host names the
    // signed-in identity. The name is the account's, which is what
    // `@parity/host-api-test-sdk` answers with too.
    await runtime.activateLocalSession(account.entropy, account.name);
    active = account;
  };

  if ((options.loginBehavior ?? "auto") === "auto") {
    const first = roster[0];
    if (!first) throw new Error("test host needs at least one account");
    await activate(first);
  }

  // The core and the product each hold one end of a MessageChannel. Frames are
  // raw SCALE bytes in both directions; nothing interprets them here.
  //
  // The two topologies expose the core differently -- a worker hands back a
  // wire provider, the main thread hands back a product core -- so each is
  // normalised to the same "pipe this port" step.
  let detach: (() => void) | undefined;
  // Set when the product connects, by whichever topology is running.
  let publishChatAction: ((action: unknown) => Promise<void>) | undefined;
  const iframeHost = createIframeHost({
    iframeUrl: options.productUrl,
    container: options.container,
    onPort(port) {
      void (async () => {
        if (workerRuntime) {
          const provider = await workerRuntime.createProvider({ productId: hostProduct });
          const unsubscribe = provider.subscribe((frame) => {
            port.postMessage(frame);
          });
          port.onmessage = (event: MessageEvent) => {
            const frame = event.data;
            if (frame instanceof Uint8Array) provider.postMessage(frame);
          };
          publishChatAction = provider.publishChatAction
            ? (action) => provider.publishChatAction!(action)
            : undefined;
          detach = () => {
            unsubscribe();
            publishChatAction = undefined;
            provider.dispose();
          };
        } else {
          const core = directRuntime!.productRuntime(
            { productId: hostProduct },
            {
              emitFrame(frame: Uint8Array) {
                port.postMessage(frame);
              },
            },
          );
          port.onmessage = (event: MessageEvent) => {
            const frame = event.data;
            if (frame instanceof Uint8Array) void core.receiveFrame(frame);
          };
          // The direct core takes bytes, so the value is encoded here rather
          // than inside the provider.
          publishChatAction = core.publishChatAction
            ? async (action) => {
                const { HostChatActionSubscribeItem } = await import(
                  "@parity/truapi"
                );
                core.publishChatAction!(
                  HostChatActionSubscribeItem.enc(action as never),
                );
              }
            : undefined;
          detach = () => {
            publishChatAction = undefined;
            core.dispose();
          };
        }
        port.start();
      })();
    },
  });

  const control: TestHostControl = Object.assign(host, {
    getAccounts: () => roster.map((account) => account.name),
    getActiveAccount: () => active?.name,
    async getProductAccountAddress(productId?: string, index = 0) {
      const target = productId ?? hostProduct;
      const subtree = await (directRuntime
        ? directRuntime.productSubtreePublicKey(target)
        : workerRuntime?.getProductSubtreePublicKey(target));
      // No session: there is no root to derive from, and answering an address
      // would name an account the host cannot sign for.
      if (!subtree) return undefined;
      const helpers = await deriveHelpers();
      // The index crosses SCALE-encoded, so the chain code stays core-owned.
      const encoded = DerivationIndex.enc({
        tag: "Index",
        value: checkDerivationIndex(index),
      });
      return helpers.productAccountAddress(
        helpers.deriveProductAccountPublicKey(subtree, encoded),
      );
    },
    injectChatAction: async (action: unknown) => {
      if (!publishChatAction) {
        throw new Error(
          "no product is connected, so there is no Chat action stream to " +
            "publish into; wait for the product frame before injecting",
        );
      }
      await publishChatAction(action);
    },
    async switchAccount(name: string) {
      const account = roster.find((entry) => entry.name === name);
      if (!account) {
        throw new Error(
          `no account "${name}" on this host; have ${roster
            .map((entry) => entry.name)
            .join(", ")}`,
        );
      }
      await activate(account);
    },
    async setAccounts(names: (DevAccountName | DevAccount)[]) {
      roster = names.map(resolveAccount);
      const first = roster[0];
      if (!first) throw new Error("test host needs at least one account");
      await activate(first);
    },
    async signOut() {
      if (!active) return;
      await runtime.disconnectSession();
      active = undefined;
    },
  });

  // The fixture locates the product by id. `createIframeHost` does not set
  // one -- a production host has no reason to -- so the test host does.
  iframeHost.iframe.id = PRODUCT_FRAME_ID;

  const unpublish = publishTestHostGlobals(control);

  return {
    host: control,
    iframe: iframeHost.iframe,
    dispose() {
      unpublish();
      detach?.();
      iframeHost.dispose();
      worker?.terminate();
      host.dispose();
    },
  };
}

/** The core's pure product-account derivation helpers. */
interface ProductAccountDerivation {
  deriveProductAccountPublicKey(
    productSubtreePublicKey: Uint8Array,
    derivationIndex: Uint8Array,
  ): Uint8Array;
  productAccountAddress(publicKey: Uint8Array): string;
}

/** The main-thread signing runtime: hands back a product core directly. */
interface DirectSigningRuntime {
  setGrantAllowancesUnchecked?(granted: boolean): void;
  productSubtreePublicKey(
    productId: string,
    timeoutMs?: number,
  ): Promise<Uint8Array | undefined>;
  setWithheldResources?(tags: string[]): void;
  activateLocalSession(secret: Uint8Array): Promise<void>;
  activateLocalSessionWithIdentity?(
    secret: Uint8Array,
    liteUsername?: string | null,
  ): Promise<void>;
  disconnectSession(): Promise<void>;
  productRuntime(
    product: { productId: string },
    sink: { emitFrame(frame: Uint8Array): void },
  ): ProductCore;
}

/** The worker-backed signing runtime: hands back a wire provider. */
interface WorkerSigningRuntime {
  activateLocalSession(secret: Uint8Array, liteUsername?: string): Promise<void>;
  disconnectSession(): Promise<void>;
  setLogLevel?(level: string): void;
  setGrantAllowancesUnchecked?(granted: boolean): Promise<void>;
  getProductSubtreePublicKey(
    productId: string,
    timeoutMs?: number,
  ): Promise<Uint8Array | undefined>;
  setWithheldResources?(tags: string[]): Promise<void>;
  createProvider(product: { productId: string }): Promise<{
    postMessage(frame: Uint8Array): void;
    subscribe(listener: (frame: Uint8Array) => void): () => void;
    dispose(): void;
    // The core's inbound Chat path. Narrowing it away here is what made
    // `injectChatAction` look unservable: `ChatPlatform` has no inbound method,
    // but the product provider does.
    publishChatAction?(action: unknown): Promise<void>;
  }>;
}

/** The subset of a per-product core this page drives. */
interface ProductCore {
  receiveFrame(frame: Uint8Array): Promise<void>;
  dispose(): void;
  /** Takes the SCALE-encoded item, where the worker provider takes the value. */
  publishChatAction?(action: Uint8Array): void;
}
