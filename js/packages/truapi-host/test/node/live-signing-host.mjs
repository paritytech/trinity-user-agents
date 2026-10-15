// End to end against a real wallet: the truapi CLI signing host pairs with this
// Node host over the live paseo-next-v2 People chain, then answers a product's
// requests. Opt-in, since it needs the network and a provisioned CLI account:
//
//   TRUAPI_HOST_CLI=~/.local/bin/truapi-host \
//   TRUAPI_HOST_BASE=/path/to/signing-host-base TRUAPI_HOST_SESSION=<name> \
//   npm run test:node
//
// Each run pairs a new device, which uses one of the CLI account's
// statement-store allowance slots.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { test } from "node:test";

import { createClient, createTransport } from "@parity/truapi";

import {
  createNodePairingHostRuntime,
  headlessHostCallbacks,
} from "../../dist/node/index.js";

const { TRUAPI_HOST_CLI, TRUAPI_HOST_BASE, TRUAPI_HOST_SESSION } = process.env;

const CHAINS = {
  People: [
    "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
    "wss://paseo-people-next-system-rpc.polkadot.io",
  ],
  Bulletin: [
    "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
    "wss://paseo-bulletin-next-rpc.polkadot.io",
  ],
  AssetHub: [
    "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
    "wss://paseo-asset-hub-next-rpc.polkadot.io",
  ],
};
const endpointByGenesis = new Map(Object.values(CHAINS));

function webSocketChain() {
  return {
    async connect(genesisHash) {
      const socket = new WebSocket(
        endpointByGenesis.get(`0x${Buffer.from(genesisHash).toString("hex")}`),
      );
      const queued = [];
      const received = [];
      let wake;
      let closed = false;
      socket.addEventListener("open", () =>
        queued.splice(0).forEach((request) => socket.send(request)),
      );
      socket.addEventListener("message", (event) => {
        received.push(String(event.data));
        wake?.();
      });
      socket.addEventListener("close", () => {
        closed = true;
        wake?.();
      });
      return {
        send(request) {
          if (socket.readyState === WebSocket.OPEN) socket.send(request);
          else queued.push(request);
        },
        async *responses() {
          while (!closed || received.length > 0) {
            if (received.length > 0) yield received.shift();
            else await new Promise((resolve) => (wake = resolve));
          }
        },
        close: () => socket.close(),
      };
    },
  };
}

function memoryStorage() {
  const entries = new Map();
  return {
    async readCoreStorage(key) {
      return entries.get(JSON.stringify(key));
    },
    async writeCoreStorage(key, value) {
      entries.set(JSON.stringify(key), value);
    },
    async clearCoreStorage(key) {
      entries.delete(JSON.stringify(key));
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
  "pairs with the CLI signing host, then the wallet answers getAccount and signRaw",
  {
    skip:
      !TRUAPI_HOST_CLI || !TRUAPI_HOST_BASE
        ? "set TRUAPI_HOST_CLI and TRUAPI_HOST_BASE"
        : false,
    timeout: 600_000,
  },
  async () => {
    let signingHost;
    let connected;
    // Stands in for the user answering "yes" at a CLI prompt.
    const reviewsAsked = [];
    const userConfirmation = {
      async confirmUserAction(review) {
        reviewsAsked.push(review.tag);
        return true;
      },
      async confirmPermission(review) {
        reviewsAsked.push(review.tag);
        return "AllowOnce";
      },
    };
    const session = new Promise((resolve) => (connected = resolve));
    const runtime = await createNodePairingHostRuntime(
      headlessHostCallbacks({
        coreStorage: memoryStorage(),
        chain: webSocketChain(),
        auth: {
          authStateChanged(state) {
            if (state.tag === "Pairing") {
              signingHost = spawn(
                TRUAPI_HOST_CLI,
                [
                  "signing-host",
                  "--network",
                  "paseo-next-v2",
                  "--base-path",
                  TRUAPI_HOST_BASE,
                  ...(TRUAPI_HOST_SESSION
                    ? ["--session", TRUAPI_HOST_SESSION]
                    : []),
                  "--auto-accept",
                  "--serve",
                  "--deeplink",
                  state.value.deeplink,
                ],
                { stdio: "ignore" },
              );
            }
            if (state.tag === "Connected") connected(state.value);
          },
        },
        productStorage,
        userConfirmation,
        permissions: {
          async devicePermission(_product, request) {
            reviewsAsked.push(request);
            return "AllowOnce";
          },
          async remotePermission(_product, request) {
            reviewsAsked.push(request.permission.tag);
            return "AllowOnce";
          },
        },
        supportedChains: {
          network: "paseo",
          chains: Object.entries(CHAINS).map(([identifier, [genesisHash]]) => ({
            identifier,
            genesisHash,
          })),
        },
      }),
      {
        hostConfig: {
          host: { name: "truapi-host node live test", platform: "Cli" },
          people: { genesisHash: CHAINS.People[0] },
          bulletin: { genesisHash: CHAINS.Bulletin[0] },
          assetHub: { genesisHash: CHAINS.AssetHub[0] },
          pairing: { deeplinkScheme: "polkadotapp" },
        },
      },
    );
    try {
      const productId = "truapi-host-node-live.dot";
      const client = createClient(
        createTransport(await runtime.createProvider({ productId }), {
          requestTimeoutMs: 300_000,
        }),
      );
      const account = {
        dotNsIdentifier: productId,
        derivationIndex: { tag: "Index", value: 0 },
      };

      const login = await client.account.requestLogin({ reason: "live test" });
      assert.ok(login.isOk(), JSON.stringify(login.isErr() && login.error));
      assert.equal(login.value, "Success");
      await session;
      const got = await client.account.getAccount({
        productAccountId: account,
      });
      assert.ok(
        got.isOk(),
        `getAccount: ${JSON.stringify(got.isErr() && got.error)}`,
      );
      const { publicKey } = got.value.account;
      assert.match(publicKey, /^0x[0-9a-f]{64}$/);
      const signed = await client.signing.signRaw({
        account,
        payload: { tag: "Bytes", value: { bytes: "0x6869" } },
      });
      assert.ok(
        signed.isOk(),
        `signRaw: ${JSON.stringify(signed.isErr() && signed.error)}`,
      );
      const { signature } = signed.value;
      assert.match(signature, /^0x[0-9a-f]{128}$/);
      // What a CLI host must answer for these two calls.
      assert.deepEqual(reviewsAsked, [
        "ProductSubtree",
        "ChainSubmit",
        "SignRaw",
      ]);
    } finally {
      runtime.dispose();
      signingHost?.kill();
    }
  },
);
