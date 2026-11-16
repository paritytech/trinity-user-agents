// A chat product that posts a "Slide to register" card. The card is a SlideToConfirm node the host
// draws natively; completing the slide arrives on the renderer action stream.
import { getClientSync } from "@parity/truapi/sandbox";
import { bytesToHex } from "@parity/truapi/scale";
import type {
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
  RendererNode,
} from "@parity/truapi";

const ROOM_ID = "slide-demo";
const ROOM_NAME = "Slide demo";
const CARD_TYPE = "register-card";
const REVEAL_CARD_TYPE = "attend-card";
const REGISTER_ACTION = "register";
const WHEN = "Oct 7 at 4:00 PM";
const INTRO = "Tonight's game is open. Register to play for these amazing prizes.";
const LOADING_MS = 1500;

type CardState = "idle" | "loading" | "done";

const client = getClientSync();
if (!client) {
  throw new Error("The slide demo worker needs a TrUAPI host connection");
}
const chat = client.chat;
const renderer = client.renderer;

const log = (...args: unknown[]) => console.log("slide-demo:", ...args);

// Per message id, in memory. A message with no entry has not been registered.
const states = new Map<string, CardState>();
const types = new Map<string, string>();
const openStreams = new Map<string, Set<(node: RendererNode) => void>>();

function card(type: string, state: CardState): RendererNode {
  const reveal = type === REVEAL_CARD_TYPE;
  return {
    tag: "SlideToConfirm",
    value: {
      modifiers: [{ tag: "FillWidth", value: true }],
      props: {
        text: reveal ? "Next game" : "Slide to register",
        secondaryText: WHEN,
        confirmedText: reveal ? "I'll be there" : "Registered",
        variant: reveal ? "Reveal" : undefined,
        accent: reveal ? "FgWarning" : undefined,
        enabled: undefined,
        loading: state === "loading" ? true : undefined,
        confirmed: state === "done" ? true : undefined,
        confirmAction: state === "idle" ? REGISTER_ACTION : undefined,
      },
    },
  };
}

function redraw(messageId: string): void {
  const tree = card(types.get(messageId) ?? CARD_TYPE, states.get(messageId) ?? "idle");
  for (const send of openStreams.get(messageId) ?? []) send(tree);
}

renderer.onRender((request: ProductRendererRenderRequest, send: (node: RendererNode) => void) => {
  if (
    request.context.tag !== "ChatMessage" ||
    ![CARD_TYPE, REVEAL_CARD_TYPE].includes(request.context.value.messageType)
  ) {
    throw new Error(`unsupported render context: ${JSON.stringify(request.context)}`);
  }
  const { messageId, messageType } = request.context.value;
  types.set(messageId, messageType);
  let streams = openStreams.get(messageId);
  if (!streams) openStreams.set(messageId, (streams = new Set()));
  streams.add(send);
  log("render", messageId, states.get(messageId) ?? "idle");
  send(card(messageType, states.get(messageId) ?? "idle"));
  return () => {
    streams.delete(send);
    if (streams.size === 0) openStreams.delete(messageId);
    log("render closed", messageId);
  };
});

renderer.actionSubscribe().subscribe({
  next(action: HostRendererActionSubscribeItem) {
    if (action.context.tag !== "ChatMessage" || action.actionId !== REGISTER_ACTION) return;
    const { roomId, messageId } = action.context.value;
    if (states.get(messageId) !== undefined) return;
    log("slide completed", messageId);
    states.set(messageId, "loading");
    redraw(messageId);
    setTimeout(() => {
      states.set(messageId, "done");
      redraw(messageId);
      log("registered", messageId);
      void postText(
        roomId,
        types.get(messageId) === REVEAL_CARD_TYPE ? `See you ${WHEN}.` : `You're registered for ${WHEN}.`,
      ).catch((error: unknown) => {
        console.error("slide-demo: confirmation failed", error);
      });
    }, LOADING_MS);
  },
  error(reason: unknown) {
    console.error("slide-demo: renderer action stream failed", reason);
  },
});

chat.actionSubscribe().subscribe({
  next(action: HostChatActionSubscribeItem) {
    void handleChatAction(action).catch((error: unknown) => {
      console.error("slide-demo: chat action failed", error);
    });
  },
  error(reason: unknown) {
    console.error("slide-demo: chat action stream failed", reason);
  },
});

async function handleChatAction(action: HostChatActionSubscribeItem): Promise<void> {
  if (action.payload.tag !== "MessagePosted" || action.payload.value.tag !== "Text") return;
  const text = action.payload.value.value.text.trim();
  if (text === "!reset") {
    log("reset");
    await postCard(action.roomId);
    await postCard(action.roomId, REVEAL_CARD_TYPE);
    await postText(action.roomId, "Reset: new cards posted");
  } else if (text.startsWith("!echo")) {
    const body = text.slice("!echo".length).trim();
    log("echo", body);
    await postText(action.roomId, body ? `Echo: ${body}` : "Usage: !echo <message>");
  }
}

async function postText(roomId: string, text: string): Promise<string> {
  const result = await chat.postMessage({ roomId, payload: { tag: "Text", value: { text } } });
  if (result.isErr()) throw new Error(`postMessage failed: ${JSON.stringify(result.error)}`);
  return result.value.messageId;
}

async function postCard(roomId: string, messageType: string = CARD_TYPE): Promise<string> {
  const result = await chat.postMessage({
    roomId,
    payload: {
      tag: "Custom",
      value: {
        messageType,
        payload: bytesToHex(new TextEncoder().encode(JSON.stringify({ version: 1 }))),
      },
    },
  });
  if (result.isErr()) throw new Error(`postMessage failed: ${JSON.stringify(result.error)}`);
  log("posted card", result.value.messageId);
  return result.value.messageId;
}

try {
  const room = await chat.createRoom({ roomId: ROOM_ID, name: ROOM_NAME, icon: "" });
  if (room.isErr()) throw new Error(`createRoom failed: ${JSON.stringify(room.error)}`);
  log("room", room.value.status);
  if (room.value.status === "New") {
    await postText(ROOM_ID, INTRO);
    await postCard(ROOM_ID);
    await postCard(ROOM_ID, REVEAL_CARD_TYPE);
  }
} catch (error) {
  console.error("slide-demo: startup failed", error);
}
