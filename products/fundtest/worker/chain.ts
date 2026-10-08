// Chain clients routed through the host, the way getcash's worker opens them
// (worker/src/shared.js): one client per chain for one step, checked against
// the genesis it was asked for, destroyed when the step ends.

import { NETWORK } from "@getsome/core";
import { getHostProvider } from "@parity/product-sdk-host";
import { createClient, type PolkadotClient } from "polkadot-api";

const CONNECT_TIMEOUT_MS = 18_000;
const ANCHOR_TIMEOUT_MS = 8_000;

/** Paseo Next v2 Asset Hub, the only network this provider runs on. */
export const PASEO_NEXT_V2_ASSET_HUB =
  "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a";

/** Rejects with a timeout error when `promise` takes longer than `ms`. */
export async function bounded<T>(
  promise: Promise<T>,
  ms: number,
  what: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`${what} timed out after ${ms}ms`)),
      ms,
    );
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

async function connect(
  genesis: `0x${string}`,
  what: string,
): Promise<PolkadotClient> {
  const provider = await bounded(
    getHostProvider(genesis),
    CONNECT_TIMEOUT_MS,
    `${what} provider`,
  );
  if (!provider) throw new Error(`${what}: the host offers no chain provider`);
  const client = createClient(provider);
  try {
    const spec = await bounded(
      client.getChainSpecData(),
      CONNECT_TIMEOUT_MS,
      `${what} chain spec`,
    );
    if (spec.genesisHash !== genesis) {
      throw new Error(`${what}: the host routed genesis ${spec.genesisHash}`);
    }
    return client;
  } catch (error) {
    client.destroy();
    throw error;
  }
}

/** Both chains a session touches. */
export interface Chains {
  assetHub: PolkadotClient;
  people: PolkadotClient;
}

/** Runs `work` with fresh Asset Hub and People clients, destroyed after. */
export async function withChains<T>(
  work: (chains: Chains) => Promise<T>,
): Promise<T> {
  const assetHub = await connect(NETWORK.assetHub.genesis, "asset hub");
  let people: PolkadotClient | undefined;
  try {
    people = await connect(NETWORK.people.genesis, "people");
    return await work({ assetHub, people });
  } finally {
    people?.destroy();
    assetHub.destroy();
  }
}

/** Signing options anchored at Asset Hub's best block. */
export async function signOptionsFor(
  client: PolkadotClient,
): Promise<{ at: string }> {
  const best = await bounded(
    client.getBestBlocks(),
    ANCHOR_TIMEOUT_MS,
    "best block",
  );
  const block = best[0];
  if (!block) throw new Error("no best block to anchor a transaction at");
  return { at: block.hash };
}

/** Whether the network the build targets is the Paseo Next v2 testnet. */
export const onTestnet = () =>
  NETWORK.testnet && NETWORK.assetHub.genesis === PASEO_NEXT_V2_ASSET_HUB;
