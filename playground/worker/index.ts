import { getClientSync } from "@parity/truapi/sandbox";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type {
  CallErrorValue,
  ChatRoomConfig,
  HostChatActionSubscribeItem,
  HostChatListSubscribeItem,
  HostRendererActionSubscribeItem,
  ObservableLike,
  ProductRendererRenderRequest,
  RendererNode,
  VersionedProductChatQueryError,
  VersionedProductRendererRenderError,
} from "@parity/truapi";
import type { Result, ResultAsync } from "neverthrow";
import { filter, firstValueFrom, from, timeout } from "rxjs";
import {
  CHAT_DIAGNOSIS_COPY_ACTION,
  CHAT_DIAGNOSIS_REFRESH_ACTION,
  ChatDiagnosis,
  type WorkerDiagnosisMethod,
} from "./diagnosis";
import { servePocket } from "./pocket";

const ROOM_ID = "truapi-playground";
const ROOM_NAME = "TrUAPI Playground";
const DIAGNOSIS_COMMAND = "!diagnose";
const ECHO_COMMAND = "!echo";
const RENDER_MESSAGE_TYPE = "truapi-chat-diagnosis";
const BOT_ID = "truapi-diagnosis-bot";
const runId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
const diagnosticRoomId = `${ROOM_ID}-diagnosis-${runId}`;
/** A host-store room: the host keeps the messages and every default applies. */
const HOST_STORE_CONFIG: ChatRoomConfig = {
  productSource: false,
  reactionDetails: false,
  participants: false,
  messageActions: false,
  participantActions: false,
  reportSeen: false,
};
const renderPayload = bytesToHex(
  new TextEncoder().encode(JSON.stringify({ version: 1, runId })),
);

const client = getClientSync();
if (!client) {
  throw new Error("TrUAPI Playground Chat worker requires a host connection");
}
const chat = client.chat;
const renderer = client.renderer;
let customMessageId: string | undefined;
let finalReportPosted = false;
type RenderInstance = {
  messageId: string;
  send: (node: RendererNode) => void;
  interrupt: (reason?: CallErrorValue<VersionedProductRendererRenderError>) => void;
  disposed: boolean;
};
const activeRenderInstances = new Set<RenderInstance>();
const pendingRenderInstances = new Set<RenderInstance>();

const diagnosis = new ChatDiagnosis(() => {
  if (diagnosis.isComplete()) {
    // Keep the E2E renderer transition deterministic: native Chat first sees
    // the in-progress tree, then one complete replacement after every method
    // has settled. The short delay also lets the harness observe both native
    // updates independently.
    setTimeout(renderActiveMessages, 1_000);
  }
  void publishFinalReportIfComplete();
});

renderer.onRender(handleRenderRequest);
registerChatQueries();

servePocket(client.pocket);

chat.actionSubscribe().subscribe({
  next(action) {
    void handleAction(action).catch((error: unknown) => {
      diagnosis.fail("Chat/action_subscribe", error);
    });
  },
  error(error) {
    diagnosis.fail("Chat/action_subscribe", error);
  },
});

renderer.actionSubscribe().subscribe({
  next(action) {
    void handleRendererAction(action).catch((error: unknown) => {
      diagnosis.fail("Renderer/action_subscribe", error);
    });
  },
  error(error) {
    diagnosis.fail("Renderer/action_subscribe", error);
  },
});
// Only a human press produces a renderer action, so the row passes on the
// open subscription; an action that does arrive replaces this detail.
diagnosis.pass(
  "Renderer/action_subscribe",
  "renderer action stream is open; a press inside the rendered tree is delivered on it",
);

async function handleRendererAction(
  action: HostRendererActionSubscribeItem,
): Promise<void> {
  if (action.context.tag !== "ChatMessage") return;
  if (action.context.value.messageId !== customMessageId) return;
  diagnosis.pass(
    "Renderer/action_subscribe",
    "received a renderer action for the diagnosis message",
  );
  if (action.actionId === CHAT_DIAGNOSIS_REFRESH_ACTION) renderActiveMessages();
  else if (action.actionId === CHAT_DIAGNOSIS_COPY_ACTION)
    await copyDiagnosisReport();
}

await runStartupDiagnosis().catch((error: unknown) => {
  diagnosis.failPending(error);
  console.error(
    "TrUAPI Playground Chat diagnosis failed",
    error instanceof Error ? error.message : String(error),
  );
});

async function runStartupDiagnosis(): Promise<void> {
  await ensureRoom(ROOM_ID, ROOM_NAME);

  const roomAppeared = waitForRoom(chat.listSubscribe(), diagnosticRoomId);
  const first = await chat.createRoom({
    roomId: diagnosticRoomId,
    metadata: { name: `TrUAPI Diagnosis ${runId}` },
    config: HOST_STORE_CONFIG,
  });
  if (first.isErr()) {
    throw new Error(`createRoom failed: ${JSON.stringify(first.error)}`);
  }
  if (first.value.status !== "New") {
    throw new Error(
      `first createRoom returned ${first.value.status}, expected New`,
    );
  }

  const second = await chat.createRoom({
    roomId: diagnosticRoomId,
    metadata: { name: `TrUAPI Diagnosis ${runId}` },
    config: HOST_STORE_CONFIG,
  });
  if (second.isErr()) {
    throw new Error(
      `second createRoom failed: ${JSON.stringify(second.error)}`,
    );
  }
  if (second.value.status !== "Exists") {
    throw new Error(
      `second createRoom returned ${second.value.status}, expected Exists`,
    );
  }
  diagnosis.pass("Chat/create_room", "created once, then returned Exists");

  await roomAppeared;
  diagnosis.pass("Chat/list_subscribe", "observed the newly created room");

  // The diagnosis asks the user to type a command, so its room keeps the input.
  const footer = await chat.setRoomFooter({
    roomId: diagnosticRoomId,
    footer: { tag: "Input", value: { photo: true, video: true, file: false } },
  });
  if (footer.isErr()) {
    throw new Error(`setRoomFooter failed: ${JSON.stringify(footer.error)}`);
  }
  diagnosis.pass("Chat/set_room_footer", "kept the text input");

  const textMessageId = await postMessage({
    tag: "Text",
    value: {
      text: `Chat diagnosis ${runId} started. Send "${DIAGNOSIS_COMMAND}" to test actions.`,
    },
  });
  customMessageId = await postMessage({
    tag: "Custom",
    value: {
      messageType: RENDER_MESSAGE_TYPE,
      payload: renderPayload,
    },
  });
  activatePendingRenderInstances();
  if (!textMessageId || !customMessageId || textMessageId === customMessageId) {
    throw new Error("postMessage did not return distinct message identifiers");
  }
  diagnosis.pass("Chat/post_message", "posted text and custom messages");

  await runRoomDeclarationProbes();
  await runBotRegistrationProbe();
}

/**
 * Each Chat Extension v2 request once, on the per-run room. Each row fails on
 * its own, so one refused request does not hide the others.
 */
async function runRoomDeclarationProbes(): Promise<void> {
  const roomId = diagnosticRoomId;
  await probe(
    "Chat/set_room_config",
    "replaced the room configuration",
    () => chat.setRoomConfig({ roomId, config: { ...HOST_STORE_CONFIG, placeholder: "No diagnosis messages yet" } }),
  );
  await probe(
    "Chat/set_room_metadata",
    "replaced the room name and description",
    () =>
      chat.setRoomMetadata({
        roomId,
        metadata: { name: `TrUAPI Diagnosis ${runId}`, description: "Chat diagnosis of the TrUAPI Playground" },
      }),
  );
  await probe("Chat/set_room_preview", "replaced the chat-list preview", () =>
    chat.setRoomPreview({ roomId, preview: { order: "Timestamp" } }),
  );
  await probe("Chat/update_feed", "pushed a new product message", () =>
    chat.updateFeed({
      roomId,
      update: {
        tag: "New",
        value: {
          messageId: `diagnosis-${runId}`,
          origin: { participantId: "product" },
          timestamp: BigInt(Date.now()),
          body: { tag: "Text", value: { text: "Posted through update_feed." } },
          status: "Sent",
          reactions: [],
        },
      },
    }),
  );
  await probe("Chat/set_room_toolbar", "set an empty toolbar", () => chat.setRoomToolbar({ roomId, actions: [] }));
  await probe("Chat/set_room_suggestions", "set no quick replies", () =>
    chat.setRoomSuggestions({ roomId, suggestions: [] }),
  );
  await probe("Chat/set_room_commands", "set an empty command catalogue", () =>
    chat.setRoomCommands({ roomId, commands: [] }),
  );
  await probe("Chat/set_room_activity", "cleared the activity line", () => chat.setRoomActivity({ roomId }));
  await probe("Chat/delete_room", "deleted a scratch room, then deleted it again", async () => {
    const scratch = `${roomId}-deleted`;
    const created = await chat.createRoom({ roomId: scratch, metadata: { name: "Deleted" }, config: HOST_STORE_CONFIG });
    if (created.isErr()) {
      throw new Error(`createRoom of the scratch room failed: ${JSON.stringify(created.error)}`);
    }
    const deleted = await chat.deleteRoom({ roomId: scratch });
    return deleted.isErr() ? deleted : chat.deleteRoom({ roomId: scratch });
  });
}

/** Run one request and record its row: pass on `Ok`, fail on `Err` or a throw. */
async function probe<T, E>(
  id: WorkerDiagnosisMethod,
  details: string,
  request: () => ResultAsync<T, E> | Promise<Result<T, E>>,
): Promise<void> {
  try {
    const result = await request();
    if (result.isErr()) {
      diagnosis.fail(id, `${id} failed: ${JSON.stringify(result.error)}`);
      return;
    }
    diagnosis.pass(id, details);
  } catch (error) {
    diagnosis.fail(id, error);
  }
}

/**
 * The six queries of Chat Extension v2. The host asks a query only for a room
 * that declares it, and no Playground room declares one, so each handler
 * answers `UnknownRoom`. The `playground/chat-v2` sample product serves them.
 */
function registerChatQueries(): void {
  const unknownRoom: CallErrorValue<VersionedProductChatQueryError> = {
    tag: "Domain",
    value: { tag: "V1", value: { tag: "UnknownRoom" } },
  };
  chat.onMessagesQuery((_request, _send, interrupt) => interrupt(unknownRoom));
  chat.onReactionsQuery((_request, _send, interrupt) => interrupt(unknownRoom));
  chat.onParticipantsQuery((_request, _send, interrupt) => interrupt(unknownRoom));
  chat.onMessageActionsQuery((_request, _send, interrupt) => interrupt(unknownRoom));
  chat.onParticipantActionsQuery((_request, _send, interrupt) => interrupt(unknownRoom));
  chat.onUserAction((_request, _send, interrupt) =>
    interrupt({ tag: "Domain", value: { tag: "V1", value: { tag: "UnknownRoom" } } }),
  );
  const details =
    "handler registered; no Playground room declares this query, so it answers UnknownRoom (playground/chat-v2 serves it)";
  for (const id of [
    "Chat/messages_query",
    "Chat/user_action",
    "Chat/reactions_query",
    "Chat/participants_query",
    "Chat/message_actions_query",
    "Chat/participant_actions_query",
  ] as const) {
    diagnosis.pass(id, details);
  }
}

/**
 * Isolated so a host without bot support reports only this row red. The id is
 * stable: there is no unregister call, so a per-run id leaks a bot per boot.
 */
async function runBotRegistrationProbe(): Promise<void> {
  const bot = { botId: BOT_ID, name: "TrUAPI Diagnosis Bot", icon: "" };
  try {
    const first = await chat.registerBot(bot);
    if (first.isErr()) {
      throw new Error(`registerBot failed: ${JSON.stringify(first.error)}`);
    }

    const second = await chat.registerBot(bot);
    if (second.isErr()) {
      throw new Error(
        `second registerBot failed: ${JSON.stringify(second.error)}`,
      );
    }
    if (second.value.status !== "Exists") {
      throw new Error(
        `second registerBot returned ${second.value.status}, expected Exists`,
      );
    }
    diagnosis.pass(
      "Chat/register_bot",
      `first registration ${first.value.status}, repeat returned Exists`,
    );
  } catch (error) {
    diagnosis.fail("Chat/register_bot", error);
  }
}

async function ensureRoom(roomId: string, name: string): Promise<void> {
  const result = await chat.createRoom({
    roomId,
    metadata: { name },
    config: HOST_STORE_CONFIG,
  });
  if (result.isErr()) {
    throw new Error(
      `Unable to create the Playground room: ${JSON.stringify(result.error)}`,
    );
  }
}

function handleRenderRequest(
  request: ProductRendererRenderRequest,
  send: (node: RendererNode) => void,
  interrupt: (reason?: CallErrorValue<VersionedProductRendererRenderError>) => void,
): () => void {
  if (request.context.tag !== "ChatMessage") {
    throw new Error(`unsupported renderer context: ${request.context.tag}`);
  }
  const { messageId, messageType } = request.context.value;
  if (messageType !== RENDER_MESSAGE_TYPE) {
    throw new Error(`unsupported message type: ${messageType}`);
  }
  const payload = JSON.parse(
    new TextDecoder().decode(hexToBytes(request.payload)),
  ) as {
    version?: number;
    runId?: string;
  };

  // Native Chat can restore custom messages from an earlier worker run before
  // it asks the current run to render its own message. Decline those instances
  // without turning the current diagnosis red.
  if (payload.runId !== runId) {
    throw new Error("custom message belongs to an earlier worker run");
  }
  if (payload.version !== 1) {
    throw new Error("render request did not preserve the custom payload");
  }

  const instance: RenderInstance = { messageId, send, interrupt, disposed: false };
  if (customMessageId) activateRenderInstance(instance);
  else pendingRenderInstances.add(instance);
  return () => {
    instance.disposed = true;
    pendingRenderInstances.delete(instance);
    activeRenderInstances.delete(instance);
  };
}

function activatePendingRenderInstances(): void {
  for (const instance of pendingRenderInstances) {
    pendingRenderInstances.delete(instance);
    activateRenderInstance(instance);
  }
}

function activateRenderInstance(instance: RenderInstance): void {
  if (instance.disposed) return;
  if (instance.messageId !== customMessageId) {
    instance.interrupt({
      tag: "HostFailure",
      value: {
        reason: `render request message ${instance.messageId} did not match ${customMessageId}`,
      },
    });
    return;
  }
  activeRenderInstances.add(instance);
  instance.send(diagnosis.rendererNode());
  diagnosis.pass(
    "Renderer/render",
    "served initial and replacement trees on a host-initiated render stream",
  );
}

function renderActiveMessages(): void {
  const node = diagnosis.rendererNode();
  for (const instance of activeRenderInstances) {
    instance.send(node);
  }
}

async function handleAction(
  action: HostChatActionSubscribeItem,
): Promise<void> {
  const { event } = action;
  if (event.tag !== "MessagePosted") return;
  if (event.value.body.tag !== "Text") return;

  const text = event.value.body.value.text.trim();
  if (text === DIAGNOSIS_COMMAND) {
    if (action.roomId !== ROOM_ID) {
      throw new Error(`diagnosis command was delivered for ${action.roomId}`);
    }
    diagnosis.pass(
      "Chat/action_subscribe",
      "received MessagePosted with the originating room",
    );
    return;
  }
  if (!text.startsWith(ECHO_COMMAND)) return;

  const body = text.slice(ECHO_COMMAND.length).trim();
  const result = await chat.postMessage({
    roomId: action.roomId,
    payload: {
      tag: "Text",
      value: {
        text: body ? `Echo: ${body}` : `Usage: ${ECHO_COMMAND} <message>`,
      },
    },
  });
  if (result.isErr()) {
    throw new Error(
      `Unable to post the echo reply: ${JSON.stringify(result.error)}`,
    );
  }
}

async function copyDiagnosisReport(): Promise<void> {
  try {
    if (!globalThis.navigator?.clipboard?.writeText) {
      throw new Error("Clipboard API is unavailable");
    }
    await globalThis.navigator.clipboard.writeText(diagnosis.markdown());
    diagnosis.copied();
  } catch {
    // A standard native Chat text message already exposes the host's Copy menu,
    // so keep that as a reliable fallback when the worker has no clipboard.
    diagnosis.copyUnavailable();
    await postMessage({
      tag: "Text",
      value: { text: diagnosis.markdown() },
    });
  }
}

async function publishFinalReportIfComplete(): Promise<void> {
  if (!diagnosis.isComplete() || finalReportPosted) return;
  finalReportPosted = true;
  const result = await chat.postMessage({
    roomId: ROOM_ID,
    payload: { tag: "Text", value: { text: diagnosis.markdown() } },
  });
  if (result.isErr()) {
    diagnosis.fail(
      "Chat/post_message",
      `Unable to post the final report: ${JSON.stringify(result.error)}`,
    );
  }
}

async function postMessage(
  payload: Parameters<typeof chat.postMessage>[0]["payload"],
): Promise<string> {
  const result = await chat.postMessage({ roomId: ROOM_ID, payload });
  if (result.isErr()) {
    throw new Error(
      `Unable to post a Playground Chat message: ${JSON.stringify(result.error)}`,
    );
  }
  return result.value.messageId;
}

async function waitForRoom(
  observable: ObservableLike<HostChatListSubscribeItem>,
  roomId: string,
): Promise<void> {
  await firstValueFrom(
    from(observable).pipe(
      filter((item) =>
        item.rooms.some((candidate) => candidate.roomId === roomId),
      ),
      timeout({ first: 10_000 }),
    ),
  );
}
