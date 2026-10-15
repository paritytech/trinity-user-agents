// `@parity/truapi-host/node` under plain Node, with no browser API patched in.
// Needs the built package and WASM: `npm run build && npm run build:wasm`.
// Run with `npm run test:node`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { createClient, createTransport } from "@parity/truapi";

import {
  createNodePairingHostRuntime,
  headlessHostCallbacks,
} from "../../dist/node/index.js";

const PEOPLE =
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";
const BULLETIN =
  "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22";
const ASSET_HUB =
  "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a";

const hostConfig = {
  host: { name: "node entry test", platform: "Cli" },
  people: { genesisHash: PEOPLE },
  bulletin: { genesisHash: BULLETIN },
  assetHub: { genesisHash: ASSET_HUB },
  pairing: { deeplinkScheme: "polkadotapp" },
};

const supportedChains = {
  network: "paseo",
  chains: [
    { identifier: "People", genesisHash: PEOPLE },
    { identifier: "Bulletin", genesisHash: BULLETIN },
    { identifier: "AssetHub", genesisHash: ASSET_HUB },
  ],
};

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

test("a product's requestLogin shows a pairing deeplink and persists the device identity", async () => {
  const storage = memoryStorage();
  let showDeeplink;
  const deeplink = new Promise((resolve) => (showDeeplink = resolve));
  const runtime = await createNodePairingHostRuntime(
    headlessHostCallbacks({
      coreStorage: storage.coreStorage,
      chain: silentChain,
      auth: {
        authStateChanged(state) {
          if (state.tag === "Pairing") showDeeplink(state.value.deeplink);
        },
      },
      productStorage,
      supportedChains,
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
    assert.match(await deeplink, /^polkadotapp:\/\/pair\?handshake=[0-9a-f]+$/);
    assert.deepEqual(storage.tagsWritten(), ["PairingDeviceIdentity"]);
    // Cancelling must end the product's login, not leave it waiting forever.
    runtime.cancelPairing();
    assert.equal((await login)._unsafeUnwrap(), "Rejected");
  } finally {
    runtime.dispose();
  }
});
