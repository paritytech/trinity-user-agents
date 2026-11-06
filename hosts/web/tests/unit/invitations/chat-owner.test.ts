import { describe, expect, test } from "bun:test";
import {
  contactChatOwner,
  type RunningChat,
} from "../../../src/invitations/wallet-chat.js";
import type { ContactChatHost } from "../../../src/invitations/contact-chat-host.js";

/** A host of `productId` that owns the request ids in `owns`, and records what it was asked. */
function host(productId: string, owns: string[], startsChats = true) {
  const asked: string[] = [];
  const fake = {
    product: { id: productId, name: productId },
    chatOptions: { tag: productId, start: { tag: `${productId} start` } },
    owns: (id: string) => owns.includes(id),
    accept: async (item: { id: string }) =>
      void asked.push(`accept ${item.id}`),
    open: (item: { id: string }) => void asked.push(`open ${item.id}`),
    attachChat: () => void asked.push("attach"),
    detachChat: () => void asked.push("detach"),
    people: () => [`0x${productId.length}`],
    sent: () => [],
    cancel: (id: string) => {
      if (!owns.includes(id)) return false;
      asked.push(`cancel ${id}`);
      return true;
    },
  };
  return {
    asked,
    chat: {
      host: fake as unknown as ContactChatHost,
      workerPath: `${productId}/worker.json`,
      startsChats,
    } satisfies RunningChat,
  };
}

describe("the host's chat products, each by its own product id", () => {
  test("each request, port and option reaches the product it belongs to, and no other", async () => {
    const contact = host("contact-chat.example", ["0xc1"]);
    const kettle = host("kettle-chat.example", ["0xk1"], false);
    const owner = contactChatOwner({
      base: "/",
      chats: () => [contact.chat, kettle.chat],
      onSentChange: () => () => {},
    });
    const item = (id: string) => ({ id, state: { tag: "accepted" } }) as never;
    await owner.notificationActions.accept(item("0xk1"));
    await owner.notificationActions.accept(item("0xc1"));
    owner.notificationActions.open?.(item("0xk1"));
    owner.notificationActions.cancel?.("0xc1");
    owner.requests.connect("kettle-chat.example", {} as MessagePort);
    owner.requests.disconnect("contact-chat.example");
    expect([contact.asked, kettle.asked]).toEqual([
      ["accept 0xc1", "cancel 0xc1", "detach"],
      ["accept 0xk1", "open 0xk1", "attach"],
    ]);
    expect([
      owner.workerNames(),
      owner.chatOptions("contact-chat.example") as unknown,
      owner.chatOptions("kettle-chat.example") as unknown,
      owner.chatOptions("other.example"),
      owner.isContactChat("other.example"),
      owner.notificationActions.canAccept(item("0xzz")),
    ]).toEqual([
      ["contact-chat.example", "kettle-chat.example"],
      // New chat starts chats in one product only; the other keeps its chats.
      {
        tag: "contact-chat.example",
        start: { tag: "contact-chat.example start" },
      },
      { tag: "kettle-chat.example" },
      null,
      false,
      false,
    ]);
    await expect(owner.resolveWorker("other.example")).rejects.toThrow(
      "other.example is not a chat product of this host.",
    );
  });
});
