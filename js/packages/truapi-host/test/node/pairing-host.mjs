// `@parity/truapi-host/node` under plain Node, with no browser API patched in.
// Needs the built package and WASM: `npm run build && npm run build:wasm`.
// Run with `npm run test:node`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { createClient, createTransport } from "@parity/truapi";

import {
  createNodePairingHostRuntime,
  createHeadlessHostCallbacks,
} from "@parity/truapi-host/node";
import { wasmIsBuilt } from "../../dist/testing/require-wasm.js";
import { bundledStack } from "./support/stack.mjs";
import { openPorts, portsBackTo } from "./support/threads.mjs";

const hostConfig = {
  host: { name: "node entry test", platform: "Cli" },
  stack: bundledStack("paseo-next-v2"),
  pairing: { deeplinkScheme: "polkadotapp" },
};

// The People, Bulletin and Asset Hub chains of the paseo-next-v2 stack.
const servedChains = [
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
  "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
  "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
];

/** A chain connection that accepts requests and never answers. */
const silentChain = {
  async connect() {
    let close;
    const closed = new Promise((resolve) => (close = resolve));
    return {
      send() {},
      async *responses() {
        await closed;
      },
      close,
    };
  },
};

function memoryStorage() {
  const entries = new Map();
  return {
    tagsWritten: () => [...entries.keys()].map((key) => JSON.parse(key).tag),
    coreStorage: {
      async readCoreStorage(key) {
        return entries.get(JSON.stringify(key));
      },
      async writeCoreStorage(key, value) {
        entries.set(JSON.stringify(key), value);
      },
      async clearCoreStorage(key) {
        entries.delete(JSON.stringify(key));
      },
    },
  };
}

const productStorage = {
  async read() {
    return undefined;
  },
  async write() {},
  async clear() {},
  subscribeStorage() {
    return {
      [Symbol.asyncIterator]: () => ({ next: () => new Promise(() => {}) }),
    };
  },
};

test(
  "a product's requestLogin shows a pairing deeplink and persists the device identity",
  { skip: !wasmIsBuilt("web/truapi_server_bg.wasm") && "WASM not built" },
  async () => {
    const baseline = openPorts();
    const storage = memoryStorage();
    let showDeeplink;
    const deeplink = new Promise((resolve) => (showDeeplink = resolve));
    const runtime = await createNodePairingHostRuntime(
      createHeadlessHostCallbacks({
        coreStorage: storage.coreStorage,
        chain: silentChain,
        auth: {
          authStateChanged(state) {
            if (state.tag === "Pairing") showDeeplink(state.value.deeplink);
          },
        },
        productStorage,
        servedChains,
        userConfirmation: {
          confirmUserAction: async () => false,
          confirmPermission: async () => "Deny",
        },
        permissions: {
          devicePermission: async () => "Deny",
          remotePermission: async () => "Deny",
        },
      }),
      { hostConfig },
    );
    try {
      await assert.rejects(runtime.activateStoredSession(), /absent/);
      const provider = await runtime.createProvider({
        productId: "node-entry-test.dot",
      });
      const client = createClient(createTransport(provider));

      const login = client.account.requestLogin({ reason: "node entry test" });
      assert.match(
        await deeplink,
        /^polkadotapp:\/\/pair\?handshake=[0-9a-f]+$/,
      );
      assert.deepEqual(storage.tagsWritten(), ["PairingDeviceIdentity"]);
      // Cancelling must end the product's login, not leave it waiting forever.
      runtime.cancelPairing();
      assert.equal((await login)._unsafeUnwrap(), "Rejected");
    } finally {
      runtime.dispose();
    }
    // The README promises dispose() stops the thread that keeps the process up.
    await portsBackTo(baseline);
  },
);
