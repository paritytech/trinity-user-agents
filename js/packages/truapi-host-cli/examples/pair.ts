// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Runnable pairing demo: renders a QR in your terminal, waits for a phone
// scan, and round-trips a product localStorage value through the paired core.
//
//   npx tsx examples/pair.ts
//
// Needs network egress to the Paseo Next V2 endpoints below, and a phone with
// the Polkadot app to scan the QR. State lands in ~/.dotli-host-cli-example
// (0600). Delete the directory to force a re-pair.

import { homedir } from "node:os";
import { join } from "node:path";
import { createClient, createTransport } from "@parity/truapi";
import { createCliHost, explainProductError } from "../src/index.js";

// Paseo Next V2, the same values dotli's web host uses (see
// packages/config/src/network.ts). An embedding app supplies its own network
// map: the host is parameterized, not opinionated.
const PEOPLE_GENESIS =
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";
const BULLETIN_GENESIS =
  "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22";
const ASSET_HUB_GENESIS =
  "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a";

const host = await createCliHost({
  host: { name: "host-cli example", version: "0.1.0" },
  pairing: { deeplinkScheme: "polkadotapp" },
  people: { genesisHash: PEOPLE_GENESIS },
  bulletin: { genesisHash: BULLETIN_GENESIS },
  assetHub: { genesisHash: ASSET_HUB_GENESIS },
  chains: {
    [ASSET_HUB_GENESIS]: {
      name: "Asset Hub",
      role: "AssetHub",
      rpc: "wss://paseo-asset-hub-next-rpc.polkadot.io",
    },
    [PEOPLE_GENESIS]: {
      name: "People",
      role: "People",
      rpc: "wss://paseo-people-next-system-rpc.polkadot.io",
    },
    [BULLETIN_GENESIS]: {
      name: "Bulletin",
      role: "Bulletin",
      rpc: "wss://paseo-bulletin-next-rpc.polkadot.io",
    },
  },
  network: "paseo-next-v2",
  storageDir: join(homedir(), ".dotli-host-cli-example"),
});

const product = host.createProduct({ productId: "host-cli-example.dot" });
const client = createClient(createTransport(product.provider));

// Drive login from the product side. The host renders the QR and progress.
// On an already-paired store this resolves in milliseconds with no phone.
try {
  const outcome = await client.account.requestLogin({
    reason: "host-cli pairing example",
  });
  console.log(`requestLogin -> ${JSON.stringify(outcome)}`);
} catch (error) {
  // The 180s wallet timeout surfaces as a bare TxError with no message.
  // explainProductError turns that into something a user can act on.
  console.error(explainProductError(error) ?? error);
  process.exit(1);
}

const written = await client.localStorage.write({
  key: "example",
  value: "0xdeadbeef",
});
console.log(`localStorage.write -> ${written.isOk() ? "Ok" : "Err"}`);
const read = await client.localStorage.read({ key: "example" });
console.log(
  `localStorage.read  -> ${read.isOk() ? JSON.stringify(read.value) : "Err"}`,
);

product.dispose();
host.dispose();
process.exit(0);
