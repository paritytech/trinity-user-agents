import { asProductId, createClient, createTransport } from "@parity/truapi";
import { bytesToHex } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { LocalStatementNode } from "../../fixtures/statements/local-statement-node.js";
import { pageStore } from "../../fixtures/invitations/shared-store.js";
import { conversationAllowance } from "../../src/invitations/wallet-delivery.js";
import type { WalletRequestsSession } from "../../src/invitations/wallet-chat.js";
import {
  peopleAllowanceGuard,
  postingRule,
} from "../../src/platform/allowance-usable.js";
import {
  readClaimGrant,
  untilFinalizedAllowance,
} from "../../src/platform/allowance-finality.js";
import { NETWORKS } from "../../src/platform/network-config.js";
import {
  checkedPostingConnection,
  StatementStore,
} from "../../src/platform/statement-store.js";
import type { DemoPostingTest } from "../../tools/demos/dev-tools.js";

export const PEOPLE =
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";

declare global {
  interface Window {
    __postingControl(request: {
      actor: string;
      op: string;
      value: unknown;
    }): Promise<unknown>;
  }
}

/** Inject only into an isolated browser context before the host starts. All chain answers are scripted by its test. */
export function installGuardedPosting(): void {
  const actor = new URL(location.href).searchParams.get("actor") ?? "backup";
  const control = (op: string, value: unknown = null) =>
    window.__postingControl({ actor, op, value });
  const node = new LocalStatementNode({ store: pageStore(), network: PEOPLE });
  let running = true;
  window.addEventListener("pagehide", () => {
    running = false;
  });

  const socket = (): JsonRpcConnection => {
    const local = node.connect(() =>
      Promise.reject(new Error("No real connection exists in this test.")),
    );
    const queued: string[] = [];
    let wake: (() => void) | undefined;
    let closed = false;
    const push = (text: string) => {
      if (!closed) {
        queued.push(text);
        wake?.();
      }
    };
    void (async () => {
      for await (const text of local.responses()) {
        const message = JSON.parse(text) as { method?: string };
        if (
          message.method === "statement_statement" &&
          (await control("deliver")) === false
        )
          continue;
        push(text);
      }
    })();
    return {
      send(text) {
        const request = JSON.parse(text) as {
          id?: unknown;
          method: string;
          params: unknown[];
        };
        void (async () => {
          if (request.method.startsWith("statement_")) {
            if (request.method === "statement_submit") {
              const refused = await control("submit", request.params[0]);
              if (refused === true) {
                push(
                  JSON.stringify({
                    jsonrpc: "2.0",
                    id: request.id,
                    result: { status: "rejected", reason: "noAllowance" },
                  }),
                );
                return;
              }
            }
            local.send(text);
          } else {
            const messages = (await control("rpc", request)) as unknown[];
            for (const message of messages) push(JSON.stringify(message));
          }
        })().catch((error: unknown) =>
          push(
            JSON.stringify({
              jsonrpc: "2.0",
              id: request.id,
              error: { code: -32000, message: String(error) },
            }),
          ),
        );
      },
      async *responses() {
        while (!closed) {
          const next = queued.shift();
          if (next !== undefined) yield next;
          else
            await new Promise<void>((resolve) => {
              wake = resolve;
            });
        }
      },
      close() {
        closed = true;
        local.close();
        wake?.();
      },
    };
  };
  const chain = {
    connect: async () => checkedPostingConnection(socket(), PEOPLE),
  };
  const access = { chain, genesis: PEOPLE as `0x${string}` };
  const guard = peopleAllowanceGuard({
    people: () => (running ? access : null),
    rule: () => postingRule(NETWORKS.paseo, "rpc", PEOPLE),
    log: (line) => {
      void control("log", line);
    },
  });
  const observedGuard: typeof guard = async (claim, request) => {
    const usable = await guard(claim, request);
    await control("usable", {
      account: claim.account,
      wait: request?.wait,
      usableAt: usable.usableAt,
      claimFinal: usable.claimFinal,
      block: usable.block,
    });
    return usable;
  };
  const test: DemoPostingTest = {
    network: (network) => ({ ...network, chain, postingReady: () => true }),
    accounts(runtime) {
      const unavailable = async (): Promise<never> => {
        throw new Error("Not part of this synthetic session.");
      };
      const session: WalletRequestsSession = {
        walletId: actor,
        entropy: new Uint8Array(32),
        network: "paseo",
        networkSuffix: "paseo",
        identityAccountId: undefined,
        chatPublicKey: undefined,
        directory: {
          names: unavailable,
          consumer: unavailable,
          owners: unavailable,
          hasAllowance: unavailable,
        },
        openStatementStore: async () =>
          new StatementStore(await chain.connect(), undefined, PEOPLE),
        claimNotificationAccount: unavailable,
        async claimProductStatementAccount(product, index) {
          const current = runtime();
          if (current === null) throw new Error("The core is not running.");
          const provider = await current.createProvider({
            productId: asProductId(product),
            executionKind: "App",
          });
          const transport = createTransport(provider);
          try {
            const result = await createClient(transport).account.getAccount({
              productAccountId: {
                dotNsIdentifier: asProductId(product),
                derivationIndex: { tag: "Raw", value: bytesToHex(index) },
              },
            });
            if (result.isErr())
              throw new Error("The synthetic core account was refused.");
            const claim = (await control("claim", {
              account: result.value.account.publicKey,
              period: Math.floor(Date.now() / 86_400_000),
              product,
              index: bytesToHex(index),
            })) as { seq: number; blockHash: string };
            return {
              ...claim,
              account: result.value.account.publicKey,
              period: Math.floor(Date.now() / 86_400_000),
              outcome: "registered",
              collection: "LitePeople",
            };
          } finally {
            transport.dispose();
            provider.dispose();
          }
        },
        usableAllowance: observedGuard,
      };
      const accounts = conversationAllowance(session, true, null);
      return {
        allow: async (product, index) => {
          const allowance = await accounts.allow(product, index);
          await control("lease", {
            product,
            index: bytesToHex(index),
            ...allowance,
          });
          return allowance;
        },
      };
    },
    nova: {
      openStore: async () =>
        new StatementStore(await chain.connect(), undefined, PEOPLE),
      claimNotification: async (account, period) => ({
        ...((await control("claim", {
          account: bytesToHex(account),
          period,
          product: "notification",
        })) as { seq: number; blockHash: string }),
        line: "synthetic inclusion, no live claim",
      }),
      notificationGuard: observedGuard,
    },
  };
  window.__truapiDemoPosting = test;
  window.__truapiBackupSettled = async (claim) => {
    await control("backup-wait", claim);
    await untilFinalizedAllowance(claim, {
      read: (account) => readClaimGrant(access, account, claim.blockHash!),
      running: () => running,
      requireClaimFinality: true,
      sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
      now: Date.now,
    });
    await control("backup-final", claim);
  };
}
