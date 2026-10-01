import { createClient, createTransport } from "@parity/truapi";
import { hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import { mapKey, readStorageValues } from "./people-chain.js";
import { Reader } from "./scale-reader.js";
import type { VersionRow } from "./versions.js";

/**
 * The open product's public account 0 and what Asset Hub holds for it.
 *
 * The account comes from the core through the product API,
 * `account.getAccount` for the product's own id and `DerivationIndex::Index(0)`,
 * called on a client of its own over a provider the runtime makes for that
 * product. The core derives the key from the wallet's entropy and returns the
 * public key only, so this page never derives anything. A product asking for
 * its own account is not reviewed, and a signing host derives locally, so the
 * call opens no prompt. The balance is a storage read at the finalized block
 * through this host's light client, the way the People chain status is read.
 *
 * Nothing is signed, proven, claimed or sent.
 */

/** `twox128("System") ++ twox128("Account")`. */
const SYSTEM_ACCOUNT_PREFIX =
  "26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9";

/** The account index of the product that is read. */
const ACCOUNT_INDEX = 0;

/** How long the core may take to answer `getAccount`. */
const ACCOUNT_TIMEOUT_MS = 20_000;

/** How long a chain may take to answer one read. */
const CHAIN_TIMEOUT_MS = 30_000;

/**
 * The public key of account 0 of `productId`, from the core.
 *
 * The provider and the client are made for this call and disposed after it, so
 * nothing is left attached to the product the page opened.
 */
export async function readProductAccount(
  source: Pick<WorkerPairingHostRuntime, "createProvider">,
  productId: string,
): Promise<HexString> {
  const provider = await source.createProvider({
    productId,
    executionKind: "App",
  });
  const transport = createTransport(provider, {
    requestTimeoutMs: ACCOUNT_TIMEOUT_MS,
  });
  try {
    const result = await createClient(transport).account.getAccount({
      productAccountId: {
        dotNsIdentifier: productId,
        derivationIndex: { tag: "Index", value: ACCOUNT_INDEX },
      },
    });
    if (result.isErr()) {
      const error = result.error;
      const reason =
        error.tag === "Domain"
          ? error.value.value.tag
          : error.value && "reason" in error.value
            ? error.value.reason
            : error.tag;
      throw new Error(`the core refused the account: ${reason}`);
    }
    return result.value.account.publicKey;
  } finally {
    transport.dispose();
    provider.dispose();
  }
}

/** `System.Account`, the part of `AccountInfo` this shows. */
export interface AssetHubAccount {
  free: bigint;
  reserved: bigint;
  frozen: bigint;
}

/** Decode a `System.Account` value. Throws when it is not the layout this reads. */
export function decodeAssetHubAccount(bytes: Uint8Array): AssetHubAccount {
  const reader = new Reader(bytes);
  reader.take(16);
  const account = {
    free: reader.u128(),
    reserved: reader.u128(),
    frozen: reader.u128(),
  };
  reader.u128();
  reader.finish();
  return account;
}

/** The token a chain declares for its native balance. */
export interface Token {
  decimals: number;
  symbol: string;
}

/** What Asset Hub holds for the account. */
export type BalanceReading =
  | {
      tag: "Read";
      blockHash: HexString;
      /** The account's record, or null when the chain has none for it. */
      account: AssetHubAccount | null;
      /** The chain's own token properties, or null when it did not answer. */
      token: Token | null;
    }
  | { tag: "Unavailable"; reason: string };

const reasonOf = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/**
 * Ask the chain for `chainSpec_v1_properties`, the decimals and symbol of its
 * native token. Null when it does not answer, so a balance is shown in its
 * smallest unit rather than with a guessed scale.
 */
async function readToken(
  chain: ChainProvider,
  genesis: HexString,
): Promise<Token | null> {
  const connection = await chain.connect(hexToBytes(genesis));
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<null>((resolve) => {
    timer = setTimeout(() => resolve(null), CHAIN_TIMEOUT_MS);
  });
  const answer = (async (): Promise<Token | null> => {
    connection.send(
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "chainSpec_v1_properties",
        params: [],
      }),
    );
    for await (const text of connection.responses()) {
      const message = JSON.parse(text) as {
        id?: number;
        result?: { tokenDecimals?: unknown; tokenSymbol?: unknown };
        error?: unknown;
      };
      if (message.id !== 1) continue;
      const { tokenDecimals, tokenSymbol } = message.result ?? {};
      const decimals = Array.isArray(tokenDecimals)
        ? tokenDecimals[0]
        : tokenDecimals;
      const symbol = Array.isArray(tokenSymbol) ? tokenSymbol[0] : tokenSymbol;
      return Number.isInteger(decimals) &&
        decimals >= 0 &&
        decimals <= 36 &&
        typeof symbol === "string"
        ? { decimals, symbol }
        : null;
    }
    return null;
  })();
  try {
    return await Promise.race([answer, timeout]);
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
    connection.close();
  }
}

/** Read `account` on Asset Hub at the finalized block. Never throws. */
export async function readBalance(
  chain: ChainProvider,
  assetHubGenesis: HexString,
  account: HexString,
): Promise<BalanceReading> {
  try {
    const key = mapKey(SYSTEM_ACCOUNT_PREFIX, hexToBytes(account));
    const connection = await chain.connect(hexToBytes(assetHubGenesis));
    const [{ blockHash, values }, token] = await Promise.all([
      readStorageValues(connection, [key], CHAIN_TIMEOUT_MS, [], "Asset Hub"),
      readToken(chain, assetHubGenesis),
    ]);
    const [value] = values;
    return {
      tag: "Read",
      blockHash,
      account: value === null ? null : decodeAssetHubAccount(hexToBytes(value)),
      token,
    };
  } catch (error) {
    return { tag: "Unavailable", reason: reasonOf(error) };
  }
}

/** Where the read for the open product stands. */
export type ProductChainState =
  | { state: "idle" }
  | { state: "loading"; productId: string }
  | { state: "error"; productId: string; message: string }
  | {
      state: "done";
      productId: string;
      account: HexString;
      balance: BalanceReading;
    };

/** `amount` in the smallest unit, shown with `decimals` places and no float. */
export function formatUnits(amount: bigint, decimals: number): string {
  const text = amount.toString();
  if (decimals === 0) return text;
  const padded = text.padStart(decimals + 1, "0");
  const fraction = padded.slice(-decimals).replace(/0+$/, "");
  const whole = padded.slice(0, -decimals);
  return fraction === "" ? whole : `${whole}.${fraction}`;
}

const ACCOUNT_NOTE =
  "From the core: account.getAccount for this product's own id and account index 0, the public key only. The core derives it from the wallet's entropy. This host derives nothing, and the call opens no prompt.";

function abbreviate(hex: string): string {
  return hex.length > 20 ? `${hex.slice(0, 8)}…${hex.slice(-5)}` : hex;
}

function balanceRow(balance: BalanceReading): VersionRow {
  const label = "Asset Hub balance";
  if (balance.tag === "Unavailable")
    return {
      label,
      value: "Unavailable",
      detail: balance.reason,
      state: "warning",
      title: `${balance.reason}\nNothing was changed. This is not a finding about the balance.`,
    };
  const note = `System.Account of account 0 on Asset Hub, at finalized block ${balance.blockHash}. The product may use other accounts, and PGAS is a separate asset that is not read here.`;
  if (balance.account === null)
    return {
      label,
      value: "No account entry",
      title: `The chain has no record for this account, which is not the same as a balance that could not be read.\n${note}`,
    };
  const { free, reserved, frozen } = balance.account;
  const shown = (amount: bigint): string =>
    balance.token === null
      ? `${amount} units`
      : `${formatUnits(amount, balance.token.decimals)} ${balance.token.symbol}`;
  return {
    label,
    value: shown(free),
    detail: balance.token === null ? "decimals not reported" : "free",
    title: `Free ${shown(free)}, reserved ${shown(reserved)}, frozen ${shown(frozen)}.${balance.token === null ? "\nThe chain gave no token properties, so amounts are in the smallest unit." : ""}\n${note}`,
  };
}

/** Rows for the open product's account and balance, from the last Refresh. */
export function productChainRows(state: ProductChainState): VersionRow[] {
  switch (state.state) {
    case "idle":
      return [
        {
          label: "Account 0",
          value: "Not read",
          detail: "press Refresh",
          state: "unknown",
        },
      ];
    case "loading":
      return [{ label: "Account 0", value: "Checking…", state: "unknown" }];
    case "error":
      return [
        {
          label: "Account 0",
          value: "Unavailable",
          detail: state.message,
          state: "warning",
          title: `${state.message}\nNothing was changed.`,
        },
      ];
    case "done":
      return [
        {
          label: "Account 0",
          value: abbreviate(state.account),
          title: `${state.account}\n${ACCOUNT_NOTE}`,
        },
        balanceRow(state.balance),
      ];
  }
}
