/**
 * PROTOTYPE. Nova Chat, a chat product that talks to the official Nova
 * (Polkadot) apps' person-to-person chat. Its worker keeps the
 * conversations (`./session.ts`) and moves their statements through its own
 * TrUAPI Statement Store, signed with `createProof` as one product account
 * per conversation. It holds no key of the user's: the host seals and opens
 * each frame and sends and accepts the requests (`nova-chat/1`,
 * `./contract.ts`). It knows people only by contact handle.
 *
 * The host builds it into an app archive it serves itself
 * (`contact-chat-plugin.ts`) and runs it through the ordinary worker loader.
 * The Chat screen shows its conversations over the Chat Extension v2 draft
 * bridge. What it keeps is on `window.__novaChatWorker`, for a test to read.
 */
import type { HexString } from "@parity/truapi/scale";
import { getClientSync } from "@parity/truapi/sandbox";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { connectChatV2 } from "../draft/product.js";
import { serveConversations, type ReadMarks } from "../draft/conversations.js";
import { ProductStorage } from "../worker/product-storage.js";
import { blake2b256, keyedBlake2b256 } from "../../platform/crypto.js";
import { utf8 } from "../../platform/codec.js";
import { NOVA_HELLO, NOVA_PROTOCOL, type NovaHello } from "./contract.js";
import { NovaSession, novaHostOver } from "./session.js";
import { novaStatements } from "./transport.js";
import { encodeMessage } from "./wire.js";

interface WorkerState {
  stage: "starting" | "running" | "failed";
  reason?: string;
  errors: string[];
  record(): string | null;
}

const RECORD_KEY = "nova-chat:record:v1";
const READS_KEY = "nova-chat:read:v1";

const state: WorkerState = {
  stage: "starting",
  errors: [],
  record: () => null,
};
(window as unknown as { __novaChatWorker: WorkerState }).__novaChatWorker =
  state;
const report = (reason: string) => {
  state.errors = [...state.errors, reason].slice(-20);
};

/** Hand the host this frame's port for `nova-chat/1`. */
function connectHost(): MessagePort {
  const channel = new MessageChannel();
  window.parent.postMessage(
    { type: NOVA_HELLO, protocol: NOVA_PROTOCOL } satisfies NovaHello,
    location.origin,
    [channel.port2],
  );
  return channel.port1;
}

async function run(): Promise<void> {
  const client = getClientSync();
  if (client === null) throw new Error("No TrUAPI host.");
  const storage = await ProductStorage.load(
    client,
    [RECORD_KEY, READS_KEY],
    (reason) => report(`storage: ${reason}`),
  );
  state.record = () => storage.getItem(RECORD_KEY);
  const context = await client.system.getProductContext();
  if (context.isErr())
    throw new Error(`No product context: ${JSON.stringify(context.error)}`);
  const session = new NovaSession({
    host: novaHostOver(connectHost()),
    statements: novaStatements(
      client,
      context.value.productId,
      (task, delayMs) => {
        const timer = setTimeout(task, delayMs);
        return () => clearTimeout(timer);
      },
    ),
    storage,
    recordKey: RECORD_KEY,
    now: Date.now,
    newId: () => crypto.randomUUID(),
    indexFor: (handle) =>
      bytesToHex(
        keyedBlake2b256(
          blake2b256(utf8("nova-chat/conversation-account/v1")),
          hexToBytes(handle as HexString),
        ),
      ),
    schedule: (task, delayMs) => {
      const timer = setTimeout(task, Math.min(delayMs, 2 ** 31 - 1));
      return () => clearTimeout(timer);
    },
    encodeText: (id, timestamp, text) =>
      bytesToHex(
        encodeMessage({ id, timestamp, content: { kind: "Text", text } }),
      ),
    report,
  });
  const marks = (): Record<string, string> => {
    try {
      return JSON.parse(storage.getItem(READS_KEY) ?? "{}") as Record<
        string,
        string
      >;
    } catch {
      return {};
    }
  };
  const reads: ReadMarks = {
    get: (conversationId) => marks()[conversationId] ?? null,
    set: (conversationId, messageId) =>
      storage.setItem(
        READS_KEY,
        JSON.stringify({ ...marks(), [conversationId]: messageId }),
      ),
  };
  const served = serveConversations(connectChatV2(), session, {
    reads,
    onError: (what, reason) => report(`${what}: ${reason}`),
  });
  await session.run();
  await served.ready;
  state.stage = "running";
}

run().catch((error: unknown) => {
  state.stage = "failed";
  state.reason = error instanceof Error ? error.message : String(error);
});
