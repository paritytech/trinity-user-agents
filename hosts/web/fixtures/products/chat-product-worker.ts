/**
 * A chat product's worker for the browser test, built to find out what a
 * chat product (the `chat.<tld>` worker) can do through TrUAPI as it ships.
 *
 * It owns its conversations and their history, as the Chat Extension v2
 * draft has a product be the message source, and uses only canonical calls:
 * `chat.createRoom`, `setRoomFooter`, `postMessage` and `actionSubscribe`,
 * and `contacts.pick`. Each thing it cannot hand the host is recorded on
 * `window.__chatProduct` for the test to read. The people and messages are
 * made up, and nothing here is a peer chat protocol.
 */
import { getClientSync } from "@parity/truapi/sandbox";

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

/** Messages this product holds for the direct conversation, oldest first. */
const HISTORY = 210;

interface OwnMessage {
  id: string;
  /** Who sent it, in the product's own model. */
  from: "ada" | "grace" | "me";
  text: string;
}

interface Seen {
  ready: boolean;
  rooms: string[];
  /** Messages the product owns, per room. */
  owned: Record<string, number>;
  /** Of those, how many the host accepted, per room. */
  posted: Record<string, number>;
  /** Host ids of what the product posted, by its own message id. */
  hostIds: Record<string, string>;
  /** Every chat action the host delivered, as it arrived. */
  actions: unknown[];
  errors: string[];
}

const seen: Seen = {
  ready: false,
  rooms: [],
  owned: {},
  posted: {},
  hostIds: {},
  actions: [],
  errors: [],
};

const history = (room: string, count: number, senders: OwnMessage["from"][]) =>
  Array.from(
    { length: count },
    (_, index): OwnMessage => ({
      id: `${room}-${index + 1}`,
      from: senders[index % senders.length],
      text: `#${index + 1}`,
    }),
  );

/** The product's own store: conversations, their people and their history. */
const conversations: Record<string, { name: string; messages: OwnMessage[] }> =
  {
    ada: { name: "Ada", messages: history("ada", HISTORY, ["ada", "me"]) },
    design: {
      name: "Design group",
      messages: history("design", 6, ["ada", "grace", "me"]),
    },
  };

client.chat.actionSubscribe().subscribe({
  next(item) {
    seen.actions.push(item);
    const payload = item.payload;
    if (payload.tag !== "MessagePosted" || payload.value.tag !== "Text") return;
    const conversation = conversations[item.roomId];
    if (conversation === undefined) return;
    // The product records the user's message in its own history, but the
    // host gave it no id, so it makes one up and cannot tell the host which
    // bubble this is, nor move it on from sent.
    const own: OwnMessage = {
      id: `${item.roomId}-user-${conversation.messages.length + 1}`,
      from: "me",
      text: payload.value.value.text,
    };
    conversation.messages.push(own);
    void post(item.roomId, {
      id: `${own.id}-answer`,
      from: "ada",
      text: `Got it: ${own.text}`,
    });
  },
});

async function post(room: string, message: OwnMessage): Promise<void> {
  const result = await client.chat.postMessage({
    roomId: room,
    payload: { tag: "Text", value: { text: message.text } },
  });
  if (result.isErr()) {
    seen.errors.push(`post ${room}: ${JSON.stringify(result.error)}`);
    return;
  }
  seen.posted[room] = (seen.posted[room] ?? 0) + 1;
  seen.hostIds[message.id] = result.value.messageId;
}

for (const [room, conversation] of Object.entries(conversations)) {
  const created = await client.chat.createRoom({
    roomId: room,
    name: conversation.name,
    icon: "",
  });
  if (created.isErr()) {
    seen.errors.push(`room ${room}: ${JSON.stringify(created.error)}`);
    continue;
  }
  seen.rooms.push(room);
  seen.owned[room] = conversation.messages.length;
  await client.chat.setRoomFooter({ roomId: room, footer: "TextInput" });
  // The host asks a product for nothing, so the only way to show history is
  // to push all of it now, and the host keeps what it keeps.
  for (const message of conversation.messages) await post(room, message);
}
seen.ready = true;

(
  window as unknown as {
    __chatProduct: { seen: Seen; pick(): Promise<unknown> };
  }
).__chatProduct = {
  seen,
  /** Ask the host's picker for a contact, as a chat product starting a conversation would. */
  async pick() {
    const result = await client.contacts.pick({});
    return result.isOk()
      ? { outcome: result.value.outcome }
      : { error: result.error };
  },
};
