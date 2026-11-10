import * as sr25519 from "@scure/sr25519";
import { hexToBytes, type HexString } from "@parity/truapi/scale";
import { deriveHardSecret } from "../../invitations/chat-keys.js";
import type { ContactChatHost } from "../../invitations/contact-chat-host.js";
import { ownIdentityProblem } from "../../invitations/published-keys.js";
import {
  confirmAccept,
  confirmSend,
  type ChatProductStart,
} from "../../invitations/wallet-chat.js";
import {
  CHAT_ACCOUNT_COST,
  conversationAllowance,
  type ChatCosts,
} from "../../invitations/wallet-delivery.js";
import { lookupChatIdentity } from "../../platform/chain-directory.js";
import { NovaBoundary } from "./boundary.js";
import { startNovaChatHost } from "./host.js";

/**
 * PROTOTYPE. Nova Chat for a signed-in wallet, as its chat product spec
 * starts it ({@link ChatProductStart}): the official Nova apps' chat
 * requests, sealed and opened by the wallet's keys in the host, read and
 * posted on the network's Statement Store. It does not run over a delivery
 * that says why it cannot (`WalletDelivery.networkOnlyProblem`).
 */
export function startNovaChat({
  session,
  spec,
  contacts,
  keys,
  deps,
}: ChatProductStart): ContactChatHost {
  const { submits: submit, networkOnlyProblem } = deps.delivery;
  const product = spec.product(session.networkSuffix);
  const chatKeyOf = async (account: Uint8Array) =>
    (await lookupChatIdentity(session.directory, account))?.chatKey ?? null;
  return startNovaChatHost({
    product,
    storageKey: `truapi-web-signing-host:network:${session.network}:nova-chat:${session.walletId}:v1`,
    boundary: NovaBoundary.fromEntropy(
      session.entropy,
      session.networkSuffix,
      chatKeyOf,
    ),
    handles: deps.handles,
    nameOf: (account) => contacts.nameOf(account),
    lookupName: async (account) =>
      (
        await lookupChatIdentity(
          session.directory,
          hexToBytes(account as HexString),
        ).catch(() => null)
      )?.name ?? null,
    onNamesChange: (listener) => deps.names.onChange(listener),
    blocked: deps.blocked,
    notifications: deps.notifications,
    verify: () =>
      networkOnlyProblem !== null
        ? Promise.resolve({ reason: networkOnlyProblem, again: false })
        : ownIdentityProblem(session.directory, keys),
    openStore: () => session.openStatementStore(),
    notificationSigner: (kind, requestId, period) => {
      const secret = deriveHardSecret(session.entropy, [
        kind,
        requestId,
        String(period),
      ]);
      return {
        public: sr25519.getPublicKey(secret),
        sign: (payload) => sr25519.sign(secret, payload),
      };
    },
    claimNotification: async (account, period) => {
      const claim = await session.claimNotificationAccount(account, submit);
      if (claim.period !== period)
        throw new Error("the day changed while the account was claimed");
      if (!submit)
        throw new Error(
          `claims are only validated in this tab: today's claim would take slot ${claim.seq}.`,
        );
      return {
        line: `notification account ${claim.outcome} for day ${claim.period}, slot ${claim.seq}.`,
        seq: claim.seq,
        ...(claim.collection === undefined
          ? {}
          : { collection: claim.collection }),
        ...(claim.blockHash === undefined
          ? {}
          : { blockHash: claim.blockHash }),
      };
    },
    notificationGuard: async (claim, request) => {
      const usable = await session.usableAllowance(claim, {
        ...request,
        wording: {
          account: "Nova Chat's account",
          retry: "it may be tried again",
        },
      });
      spec.allowanceSeen?.(usable);
      return usable;
    },
    accounts: conversationAllowance(
      session,
      submit,
      spec.allowanceSeen ?? null,
    ),
    confirmSend: (prompt) => confirmSend(prompt, NOVA_COSTS),
    confirmAccept: (prompt) => confirmAccept(prompt, NOVA_COSTS),
    showChat: deps.showChat,
    onSenders: deps.namedChanged,
    log: deps.log,
  });
}

/** What Nova Chat spends and exposes, for its prompts. Each request and acceptance is posted once. */
export const NOVA_COSTS: ChatCosts = {
  summary: "Uses daily slots · Messages are end-to-end encrypted",
  request: "1 notification slot, once. It is not posted again on a later day.",
  reply: "1 notification slot, once, for the acceptance.",
  chat: CHAT_ACCOUNT_COST,
  messages:
    "End-to-end encrypted, as the Nova apps do. Who posts, to which topics, when and how much is public.",
};
