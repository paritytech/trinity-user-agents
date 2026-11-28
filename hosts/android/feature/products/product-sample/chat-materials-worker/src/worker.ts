// A chat demo worker whose card wraps its text in a `Materials` node, so a host that draws
// materials shows the layers of the material over it, and any other host shows the text plain.
import { getClientSync } from "@parity/truapi/sandbox";
import type { HostChatActionSubscribeItem, ProductRendererRenderRequest, RendererNode } from "@parity/truapi";

const ROOM_ID = "materials-demo";
const ROOM_NAME = "Materials demo";
const MESSAGE_TYPE = "materials-card";

// Replace with the Bulletin ID of a published material: the BLAKE2b-256 hash of its canonical
// text, as 0x and 64 hex digits. The card draws plain until one resolves.
const MATERIAL_ID_PLACEHOLDER = `0x${"00".repeat(32)}` as const;

const client = getClientSync();
if (!client) {
  throw new Error("The materials demo worker needs a TrUAPI host connection");
}

function text(value: string, style: "TitleMediumRegular" | "BodyMediumRegular"): RendererNode {
  return {
    tag: "Text",
    value: { modifiers: [], props: { style, color: "FgPrimary" }, children: [{ tag: "String", value: { text: value } }] },
  };
}

function card(): RendererNode {
  return {
    tag: "Materials",
    value: {
      props: { layers: [{ material: MATERIAL_ID_PLACEHOLDER, args: [] }] },
      children: [
        {
          tag: "Column",
          value: {
            modifiers: [{ tag: "Padding", value: { top: 16, end: 16 } }],
            props: {},
            children: [text("Daily Dollar", "TitleMediumRegular"), text("Tilt your phone", "BodyMediumRegular")],
          },
        },
      ],
    },
  };
}

client.renderer.onRender((request: ProductRendererRenderRequest, send: (node: RendererNode) => void) => {
  if (request.context.tag !== "ChatMessage" || request.context.value.messageType !== MESSAGE_TYPE) {
    throw new Error(`unsupported render context: ${JSON.stringify(request.context)}`);
  }
  send(card());
  return () => {};
});

client.chat.actionSubscribe().subscribe({
  next(action: HostChatActionSubscribeItem) {
    if (action.roomId !== ROOM_ID || action.payload.tag !== "MessagePosted" || action.payload.value.tag !== "Text") return;
    void client.chat
      .postMessage({ roomId: ROOM_ID, payload: { tag: "Custom", value: { messageType: MESSAGE_TYPE, payload: "0x" } } })
      .then((result) => {
        if (result.isErr()) console.error("Materials demo: post_message failed", JSON.stringify(result.error));
      });
  },
  error(reason: unknown) {
    console.error("Materials demo: chat action stream failed", reason);
  },
});

const room = await client.chat.createRoom({ roomId: ROOM_ID, name: ROOM_NAME, icon: "" });
if (room.isErr()) {
  console.error("Materials demo: create_room failed", JSON.stringify(room.error));
}
