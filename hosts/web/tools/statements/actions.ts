import { hexToBytes, type HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import { allowanceKey } from "../../src/platform/chain-directory.js";
import { readStorageValues } from "../../src/platform/people-chain.js";
import { Reader } from "../../src/platform/scale-reader.js";

/**
 * The Statements view's one read on demand: {@link readGrants} reads
 * `:statement_allowance:` for accounts at the newest finalized People block,
 * what the runtime grants each one, not what it uses.
 *
 * The inspector opens no statement subscription of its own. In this host's
 * light client (smoldot-light 2.1.0) every People connection shares one
 * network service, and each connection's statement subscriptions replace
 * the one topic filter that service sends to its peers
 * (`json_rpc_service/background.rs:922-929`, `network_service.rs:2314-2334`).
 * A subscription made to look would take the core's chat topics off that
 * filter while it ran, and perhaps after.
 *
 * A grant read opens a connection, follows the chain and reads storage
 * only. A connection schedules a filter update on a statement subscribe or
 * unsubscribe alone (`background.rs:3146`, `3178`; a new one has none
 * pending, `:734-736`), and closing it drops the shared filter only when it
 * was the chain's last connection (`lib.rs:1010-1020`,
 * `network_service.rs:1769`), which the core's own connection is not.
 */

/** The People chain, as the view reaches it. */
export interface PeopleAccess {
  chain: ChainProvider;
  genesis: HexString;
}

/** A `StatementAllowance` as the runtime stores it, or null when none is. */
export type Grant = { maxCount: number; maxSize: number } | null;

/** The SCALE `StatementAllowance { max_count: u32, max_size: u32 }`. */
export function decodeGrant(hex: string): {
  maxCount: number;
  maxSize: number;
} {
  const reader = new Reader(hexToBytes(hex));
  const grant = { maxCount: reader.u32(), maxSize: reader.u32() };
  reader.finish();
  return grant;
}

export async function readGrants(
  access: PeopleAccess,
  accounts: string[],
): Promise<{ block: string; values: { account: string; value: Grant }[] }> {
  const connection = await access.chain.connect(hexToBytes(access.genesis));
  const { blockHash, values } = await readStorageValues(
    connection,
    accounts.map((account) => allowanceKey(hexToBytes(account))),
    undefined,
    [],
    "People",
  );
  return {
    block: blockHash,
    values: accounts.map((account, index) => {
      const value = values[index];
      return { account, value: value === null ? null : decodeGrant(value) };
    }),
  };
}
