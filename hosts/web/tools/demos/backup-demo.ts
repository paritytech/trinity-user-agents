import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import type { HostBackupDeps } from "../../src/backup/host-backup.js";
import type { StatementStoreLike } from "../../src/platform/statement-store.js";

/**
 * DEV ONLY. A simulated network for the host's backup, which a browser test
 * runs and every one of its browsers reaches: the test exposes
 * `window.__truapiBackupNetwork` before the page loads, and the dev server's
 * tools hand the backup this network in place of the real one. The core
 * still derives the backup account and signs its pointer. Without the
 * test's function, nothing here is used.
 */

/** The test's end: one operation and its argument, `0x` hex where bytes. */
export type BackupNetworkBridge = (
  op: "submit" | "statements" | "store" | "fetch" | "claim",
  argument: string,
) => Promise<string | string[] | null>;

declare global {
  interface Window {
    /** The simulated backup network a browser test exposes. */
    __truapiBackupNetwork?: BackupNetworkBridge;
  }
}

/** The backup's network, and its pace, as a browser test runs them. */
export type BackupTestNetwork = Required<
  Pick<HostBackupDeps, "network" | "pace">
>;

const POLL_MS = 100;

/** A Statement Store over `bridge`, read by polling it. */
function bridgeStore(bridge: BackupNetworkBridge): StatementStoreLike {
  let ended!: () => void;
  const closed = new Promise<void>((resolve) => (ended = resolve));
  const stops = new Set<() => void>();
  return {
    closed,
    subscribe: async (filter, onStatements) => {
      const topics = "matchAll" in filter ? filter.matchAll : filter.matchAny;
      const seen = new Set<string>();
      let stopped = false;
      const poll = async () => {
        while (!stopped) {
          for (const topic of topics) {
            const held = (await bridge(
              "statements",
              bytesToHex(topic),
            )) as string[];
            const fresh = held.filter((statement) => !seen.has(statement));
            fresh.forEach((statement) => seen.add(statement));
            if (!stopped && fresh.length > 0)
              onStatements(fresh.map((statement) => hexToBytes(statement)));
          }
          await new Promise((resolve) => setTimeout(resolve, POLL_MS));
        }
      };
      void poll();
      const stop = () => {
        stopped = true;
      };
      stops.add(stop);
      return stop;
    },
    submit: async (statement) => ({
      status: (await bridge("submit", bytesToHex(statement))) as string,
    }),
    close: () => {
      for (const stop of stops) stop();
      ended();
    },
  };
}

/** The simulated backup network the page's test exposed, or null when it exposed none. */
export function backupTestNetwork(
  log: (line: string) => void,
): BackupTestNetwork | null {
  const bridge = window.__truapiBackupNetwork;
  if (bridge === undefined) return null;
  log(
    "Backup: this tab keeps its backup on a simulated network that a test runs, not on the network.",
  );
  return {
    network: {
      claimSlot: async () =>
        JSON.parse(
          (await bridge("claim", "")) as string,
        ) as StatementAccountClaim,
      settled: async () => {},
      storeSnapshot: async (sealed) =>
        hexToBytes((await bridge("store", bytesToHex(sealed))) as string),
      fetchSnapshot: async (key) => {
        const held = (await bridge("fetch", bytesToHex(key))) as string | null;
        return held === null ? undefined : hexToBytes(held);
      },
      openStore: async () => bridgeStore(bridge),
    },
    pace: { checkMs: 500, quietMs: 1_000 },
  };
}
