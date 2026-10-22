/**
 * A product worker for the browser test: it creates one room, posts a text
 * and a counter card, draws the card with an image from its own archive,
 * counts button presses, and answers what the user types. It reaches the host
 * only through TrUAPI, and makes no other request. What it saw is kept on
 * `window.__fixture` for the test to read.
 */
import type { RendererNode } from "@parity/truapi";
import { getClientSync } from "@parity/truapi/sandbox";

const ROOM = "fixture";
/** A 1x1 transparent PNG: rooms need an icon, and an inline image is one the core accepts. */
const ICON =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

interface Seen {
  registerBot: "pending" | "accepted" | "refused";
  room: "pending" | "created" | "failed";
  posted: string[];
  actions: string[];
  messages: string[];
  errors: string[];
}

const seen: Seen = {
  registerBot: "pending",
  room: "pending",
  posted: [],
  actions: [],
  messages: [],
  errors: [],
};
(window as unknown as { __fixture: Seen }).__fixture = seen;

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

let count = 0;
const text = (
  value: string,
  style: "TitleMediumRegular" | "BodySmallRegular",
): RendererNode => ({
  tag: "Text",
  value: {
    modifiers: [],
    props: { style },
    children: [{ tag: "String", value: { text: value } }],
  },
});
const card = (): RendererNode => ({
  tag: "Column",
  value: {
    modifiers: [{ tag: "Padding", value: { top: 12, end: 12 } }],
    props: { horizontalAlignment: "Start" },
    children: [
      {
        tag: "Image",
        value: {
          modifiers: [
            { tag: "Width", value: 16 },
            { tag: "Height", value: 16 },
          ],
          props: { source: { tag: "Archive", value: "images/dot.svg" } },
        },
      },
      text(`count ${count}`, "TitleMediumRegular"),
      text("<b>not markup</b>", "BodySmallRegular"),
      {
        tag: "Button",
        value: {
          modifiers: [],
          props: { text: "bump", clickAction: "bump" },
          children: [],
        },
      },
    ],
  },
});

const redraws = new Set<() => void>();
client.renderer.onRender((_request, send) => {
  const draw = () => send(card());
  redraws.add(draw);
  draw();
  return () => redraws.delete(draw);
});
client.renderer.actionSubscribe().subscribe({
  next(item) {
    seen.actions.push(item.actionId);
    if (item.actionId !== "bump") return;
    count += 1;
    for (const draw of redraws) draw();
  },
});
client.chat.actionSubscribe().subscribe({
  next(item) {
    const payload = item.payload;
    if (payload.tag !== "MessagePosted" || payload.value.tag !== "Text") return;
    seen.messages.push(payload.value.value.text);
    void post({
      tag: "Text",
      value: { text: `echo: ${payload.value.value.text}` },
    });
  },
});

async function post(
  payload: Parameters<typeof client.chat.postMessage>[0]["payload"],
  alt?: string,
): Promise<void> {
  const result = await client.chat.postMessage({ roomId: ROOM, payload, alt });
  if (result.isOk()) seen.posted.push(result.value.messageId);
  else seen.errors.push(`post: ${JSON.stringify(result.error)}`);
}

const bot = await client.chat.registerBot({
  botId: "fixture-bot",
  name: "Fixture",
  icon: ICON,
});
seen.registerBot = bot.isOk() ? "accepted" : "refused";
const room = await client.chat.createRoom({
  roomId: ROOM,
  name: "Fixture bot",
  icon: ICON,
});
if (room.isErr()) {
  seen.room = "failed";
  seen.errors.push(`room: ${JSON.stringify(room.error)}`);
} else {
  seen.room = "created";
  await client.chat.setRoomFooter({ roomId: ROOM, footer: "TextInput" });
  await post({ tag: "Text", value: { text: "hello from the fixture" } });
  await post(
    {
      tag: "Custom",
      value: { messageType: "fixture.counter", payload: "0x01" },
    },
    "A counter",
  );
}
