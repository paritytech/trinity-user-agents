import { describe, expect, test } from "bun:test";
import { asProductId } from "@parity/truapi";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import type { HostDelivery } from "../../../../src/invitations/ports.js";
import { StatementStoreDelivery } from "../../../../src/invitations/statement-delivery.js";
import type { WalletRequestsSession } from "../../../../src/invitations/wallet-chat.js";
import {
  DAY_INBOX_LISTS,
  networkDelivery,
  type WalletDelivery,
} from "../../../../src/invitations/wallet-delivery.js";
import {
  devWalletDelivery,
  type DevDeliveryMode,
} from "../../../../tools/demos/wallet-deliveries.js";
import { memoryDirectory } from "../../../support/statement-memory.js";
import { finalizedGuard } from "../../../support/allowance.js";

/**
 * The deliveries the Developer section may hand a wallet's chat in place of
 * the network's. Each keeps the network's facts except the one it changes:
 * a validate-only delivery keeps the network's lists and never submits a
 * claim, and a local one keeps its own lists and spends nothing.
 */

const localDelivery = { local: true } as unknown as HostDelivery;

const dev = (mode: DevDeliveryMode): WalletDelivery =>
  devWalletDelivery(mode, { log: () => {}, local: () => localDelivery });

/** A session that answers every product account claim, and records whether it was submitted. */
function session(submits: boolean[]): WalletRequestsSession {
  const never = () => new Promise<never>(() => {});
  return {
    walletId: "wallet",
    entropy: new Uint8Array(16),
    network: "paseo",
    networkSuffix: "paseo",
    identityAccountId: undefined,
    chatPublicKey: undefined,
    directory: memoryDirectory([]),
    openStatementStore: never,
    claimNotificationAccount: never,
    claimProductStatementAccount: async (_product, _index, submit) => {
      submits.push(submit);
      return {
        account: "0xaccount",
        period: 1,
        outcome: submit ? "registered" : "validated",
        seq: 3,
        collection: "People",
      } satisfies StatementAccountClaim;
    },
    usableAllowance: finalizedGuard,
  };
}

/** What a chat account's allowance answers over `delivery`, and whether its claim was submitted. */
async function allow(
  delivery: WalletDelivery,
): Promise<{ answer: string; submitted: boolean[] }> {
  const submitted: boolean[] = [];
  const allowance = delivery.allowance(session(submitted), null);
  if (allowance === undefined) return { answer: "none", submitted };
  const answer = await allowance
    .allow(asProductId("chat.paseo"), new Uint8Array(4))
    .then(
      (granted) => `account ${granted.account}`,
      (error: Error) => error.message,
    );
  return { answer, submitted };
}

describe("a wallet's delivery", () => {
  test("on the network keeps the day inbox's lists, submits claims and spends what the prompts say", async () => {
    const network = networkDelivery({ log: () => {} });
    expect({
      lists: network.listSuffix,
      submits: network.submits,
      hides: network.hidesWalletContacts,
      nova: network.novaProblem,
      messages: network.costs(false)?.messages,
      delivery:
        network.hostDelivery(session([]), "0xself") instanceof
        StatementStoreDelivery,
      allowance: await allow(network),
    }).toEqual({
      lists: DAY_INBOX_LISTS,
      submits: true,
      hides: true,
      nova: null,
      messages: "Not encrypted: anyone can read them.",
      delivery: true,
      allowance: { answer: "account 0xaccount", submitted: [true] },
    });
  });

  test("validate only keeps the network's lists and delivery, and submits no claim", async () => {
    const network = networkDelivery({ log: () => {} });
    const validate = dev("validate");
    expect({
      lists: validate.listSuffix,
      hides: validate.hidesWalletContacts,
      nova: validate.novaProblem,
      submits: validate.submits,
      delivery:
        validate.hostDelivery(session([]), "0xself") instanceof
        StatementStoreDelivery,
      messages: validate.costs(true)?.messages,
      summary: validate.costs(false)?.summary,
    }).toEqual({
      lists: network.listSuffix,
      hides: network.hidesWalletContacts,
      nova: network.novaProblem,
      submits: false,
      delivery: true,
      messages: null,
      summary: "Uses daily slots, only validated",
    });
    const { answer, submitted } = await allow(validate);
    expect(submitted).toEqual([false]);
    expect(answer).toStartWith("This tab only validates claims");
  });

  test("in this browser keeps lists of its own, carries requests between tabs and spends nothing", async () => {
    const local = dev("local");
    expect({
      lists: local.listSuffix,
      hides: local.hidesWalletContacts,
      submits: local.submits,
      delivery: local.hostDelivery(session([]), "0xself"),
      costs: local.costs(false),
      allowance: await allow(local),
    }).toEqual({
      lists: "",
      hides: false,
      submits: false,
      delivery: localDelivery,
      costs: null,
      allowance: { answer: "none", submitted: [] },
    });
    expect(local.novaProblem).toContain("Nova Chat needs the network delivery");
  });

  test("in this browser with chat claims validated keeps the local lists, and validates each chat account's claim", async () => {
    const local = dev("local");
    const localValidate = dev("local-validate");
    expect({
      lists: localValidate.listSuffix,
      hides: localValidate.hidesWalletContacts,
      nova: localValidate.novaProblem,
      delivery: localValidate.hostDelivery(session([]), "0xself"),
      request: localValidate.costs(false)?.request,
    }).toEqual({
      lists: local.listSuffix,
      hides: true,
      nova: local.novaProblem,
      delivery: localDelivery,
      request: "Nothing: the request stays in this browser.",
    });
    const { answer, submitted } = await allow(localValidate);
    expect(submitted).toEqual([false]);
    expect(answer).toStartWith("This tab only validates claims");
  });
});
