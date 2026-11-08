import { createClient, createTransport } from "@parity/truapi";
import { bytesToHex } from "@parity/truapi/scale";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { ConversationAllowance } from "../../src/invitations/ports.js";
import type { Network } from "../../src/platform/network.js";
import { LocalStatementNode } from "../../fixtures/statements/local-statement-node.js";
import { pageStore } from "../../fixtures/invitations/shared-store.js";
import {
  StatementStore,
  type StatementStoreLike,
} from "../../src/platform/statement-store.js";

/**
 * DEV ONLY. What the invitation demo gives Kettle Chat
 * (`../chat/kettle.ts`). A signed-in wallet runs it over the wallet's
 * delivery, the network by default: real requests, claims and statements.
 * The invitation demo's
 * made-up users cannot reach the network, so the demo gives Kettle what the
 * network would, in this browser only:
 *
 * - a SIMULATED Statement Store the tabs share
 *   (`fixtures/statements/local-statement-node.ts`), in place of the People
 *   node's statement methods; and
 * - a SIMULATED allowance for each conversation account: `chatAccount`
 *   answers with the account the core derives, and claims nothing.
 *
 * Nothing here reaches the network: it shows the chat product, the host
 * request service and Kettle working together, not the live Statement Store.
 */

/** The simulated node {@link simulatedStatements} made last, or null. */
let simulated: LocalStatementNode | null = null;

/**
 * The host's own client of the simulated Statement Store, for Nova Chat's
 * requests and acceptances: the store the tabs share, with no other method.
 */
export function openSimulatedStatementStore(): Promise<StatementStoreLike> {
  if (simulated === null)
    return Promise.reject(
      new Error("The simulated Statement Store is not running."),
    );
  return Promise.resolve(
    new StatementStore(
      simulated.connect(() =>
        Promise.reject(new Error("Only statement methods are simulated.")),
      ),
    ),
  );
}

/** `network`, with the People node's statement methods answered by this browser's simulated store. */
export function simulatedStatements(
  network: Network,
  log: (line: string) => void,
): Network {
  const node = new LocalStatementNode({
    store: pageStore(),
    network: network.genesis.people,
    log,
  });
  simulated = node;
  log(
    "Kettle Chat demo: People's Statement Store is simulated in this browser; no statement leaves it.",
  );
  const people = network.genesis.people.toLowerCase();
  return {
    ...network,
    chain: {
      async connect(genesisHash) {
        if (bytesToHex(genesisHash).toLowerCase() !== people)
          return network.chain.connect(genesisHash);
        return node.connect(() => network.chain.connect(genesisHash));
      },
    },
  };
}

/** The end of the UTC day, as an allowance period ends. */
const endOfDay = (nowMs: number) =>
  (Math.floor(nowMs / 86_400_000) + 1) * 86_400_000;

/**
 * The simulated Statement Store keeps a statement as soon as the claim
 * answered, so a notification account's allowance settles at once.
 */
export const simulatedSettled = (): Promise<void> => Promise.resolve();

/**
 * A simulated conversation allowance: the account comes from the core, as
 * `account.getAccount` gives it to the product, and nothing is claimed.
 */
export function simulatedAllowance(
  runtime: () => WorkerPairingHostRuntime | null,
  log: (line: string) => void,
): ConversationAllowance {
  return {
    async allow(product, index) {
      const current = runtime();
      if (current === null) throw new Error("The core is not running.");
      const provider = await current.createProvider({
        productId: product,
        executionKind: "App",
      });
      const transport = createTransport(provider, { requestTimeoutMs: 20_000 });
      try {
        const result = await createClient(transport).account.getAccount({
          productAccountId: {
            dotNsIdentifier: product,
            derivationIndex: { tag: "Raw", value: bytesToHex(index) },
          },
        });
        if (result.isErr())
          throw new Error(
            `the core refused the account: ${JSON.stringify(result.error)}`,
          );
        const account = result.value.account.publicKey.toLowerCase();
        log(
          `Kettle Chat demo: account ${account.slice(0, 10)}… may post today. Simulated: nothing was claimed.`,
        );
        return { account, until: endOfDay(Date.now()) };
      } finally {
        transport.dispose();
        provider.dispose();
      }
    },
  };
}
