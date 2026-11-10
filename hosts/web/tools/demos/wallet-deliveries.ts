import {
  CHAT_ACCOUNT_COST,
  conversationAllowance,
  networkDelivery,
  statementStoreDelivery,
  type NetworkDeliveryDeps,
  type WalletDelivery,
} from "../../src/invitations/wallet-delivery.js";
import type { HostDelivery } from "../../src/invitations/ports.js";

/**
 * DEV ONLY. The deliveries a developer may choose for the signed-in wallet's
 * host requests in the Developer section, in place of the network's
 * (`src/invitations/wallet-delivery.ts`): `validate` uses the network
 * delivery with every claim only validated, `local` keeps requests between
 * tabs of this browser, and `local-validate` does so and only validates each
 * chat account's claim.
 */
export type DevDeliveryMode = "validate" | "local" | "local-validate";

export const DEV_DELIVERY_MODES: readonly DevDeliveryMode[] = [
  "validate",
  "local",
  "local-validate",
];

export interface DevDeliveryDeps extends NetworkDeliveryDeps {
  /** The simulated delivery between tabs of this browser, for the user whose chat key is `self`. */
  local(self: string): HostDelivery;
}

/** Whether `mode` keeps the tab from submitting anything. */
export const submitsNothing = (mode: DevDeliveryMode | "network"): boolean =>
  mode === "validate" || mode === "local-validate";

const NOVA_NEEDS_NETWORK =
  "Nova Chat needs the network delivery: the Nova apps read only the network's Statement Store.";

/** The delivery `mode` names. */
export function devWalletDelivery(
  mode: DevDeliveryMode,
  deps: DevDeliveryDeps,
): WalletDelivery {
  switch (mode) {
    case "validate":
      return {
        ...networkDelivery(deps),
        line: "requests and replies use the network delivery with claims only validated, and nothing is submitted.",
        submits: false,
        costs: () => ({
          summary: "Uses daily slots, only validated",
          request: "1 notification slot, only validated: nothing is submitted.",
          reply: "1 notification slot, only validated: nothing is submitted.",
          chat: `${CHAT_ACCOUNT_COST} Only validated: nothing is submitted.`,
          messages: null,
        }),
        hostDelivery: (session) => statementStoreDelivery(session, false, deps),
        allowance: (session, seen) =>
          conversationAllowance(session, false, seen),
      };
    case "local":
      return {
        line: "requests and replies stay between tabs of this browser.",
        listSuffix: "",
        submits: false,
        hidesWalletContacts: false,
        networkOnlyProblem: NOVA_NEEDS_NETWORK,
        costs: () => null,
        hostDelivery: (_session, self) => deps.local(self),
        allowance: () => undefined,
      };
    case "local-validate":
      return {
        ...devWalletDelivery("local", deps),
        line: "requests and replies stay in this browser, conversation accounts' claims are only validated, and nothing is submitted.",
        hidesWalletContacts: true,
        costs: () => ({
          summary: "Uses chat slots, only validated",
          request: "Nothing: the request stays in this browser.",
          reply: "Nothing: the reply stays in this browser.",
          chat: `${CHAT_ACCOUNT_COST} Only validated: nothing is submitted or sent.`,
          messages: null,
        }),
        allowance: (session, seen) =>
          conversationAllowance(session, false, seen),
      };
  }
}
