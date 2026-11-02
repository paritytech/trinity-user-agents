/**
 * PROTOTYPE. Contact Chat, the host's chat product: a worker that knows
 * people only by the contact handles its host gives it (`./engine.ts`). The
 * host builds it into an app archive it serves itself
 * (`contact-chat-plugin.ts`), and runs it through the ordinary worker loader
 * for a signed-in wallet, or for a test user of the invitation demo.
 *
 * In its frame:
 *
 * - the host's Chat screen shows its conversations over the Chat Extension
 *   v2 draft bridge (`src/chat/draft/conversations.ts`), and names and draws
 *   each person from the handle;
 * - setup goes through the proposed host request service
 *   (`src/invitations/contract.ts`), with `simple-text/1` as its protocol;
 * - text goes through its own TrUAPI Statement Store, each conversation
 *   posting as the product's own account at a random raw index, signed with
 *   `createProof`, which the host allows through `chatAccount`;
 * - its record is kept in its own TrUAPI storage.
 *
 * What it received from the host, and what it keeps, are on
 * `window.__contactChatWorker`, for a test to read. A test also drives there
 * what a product does on its own, outside the Chat screen: pick a contact
 * through TrUAPI's `contacts.pick` and send a request to the handle it gets,
 * or cancel its own request.
 */
import { getClientSync } from "@parity/truapi/sandbox";
import { connectChatV2 } from "../draft/product.js";
import { serveConversations, type ReadMarks } from "../draft/conversations.js";
import { ProductStorage } from "../worker/product-storage.js";
import { postDiagnostics } from "../../products/diagnostics.js";
import { cryptoRandom } from "../../invitations/sealing.js";
import { ContactChat } from "./engine.js";
import { connectHostRequests, type HostRequests } from "./requests.js";
import { simpleText } from "./simple-text.js";
import { truapiStatements } from "./transport.js";

interface WorkerState {
  stage: "starting" | "running" | "failed";
  reason?: string;
  errors: string[];
  /** Every message the host sent this product, as JSON, in order. */
  received: string[];
  /** Every start the host asked for, as JSON. */
  starts: string[];
  /** The product's whole stored record, as JSON. */
  record(): string | null;
  /** Pick a contact with `contacts.pick` and start a chat with the handle: the room, or the pick's outcome when none was picked. */
  pickAndStart(
    welcome: string,
  ): Promise<{ roomId: string } | { outcome: string }>;
  /** Cancel the request of `roomId` through the host. */
  cancel(roomId: string): Promise<void>;
}

const notRunning = () =>
  Promise.reject(new Error("Contact Chat is not running."));

const RECORD_KEY = "contact-chat:record:v3";
const READS_KEY = "contact-chat:read:v1";
const RECEIVED_MAX = 200;

const state: WorkerState = {
  stage: "starting",
  errors: [],
  received: [],
  starts: [],
  record: () => null,
  pickAndStart: notRunning,
  cancel: notRunning,
};
(
  window as unknown as { __contactChatWorker: WorkerState }
).__contactChatWorker = state;
const report = (reason: string) => {
  state.errors = [...state.errors, reason].slice(-20);
};
const heard = (value: unknown) => {
  state.received = [...state.received, JSON.stringify(value)].slice(
    -RECEIVED_MAX,
  );
};

/** The request service, with every host message it hears kept for a test. */
function observed(requests: HostRequests): HostRequests {
  return {
    call: async (call) => {
      const result = await requests.call(call);
      heard({ result });
      return result;
    },
    onMessage: (listener) =>
      requests.onMessage((message) => {
        heard(message);
        listener(message);
      }),
  };
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
  const productId = context.value.productId;
  const engine = new ContactChat({
    requests: observed(connectHostRequests()),
    protocol: simpleText,
    statements: truapiStatements(client, productId, (task, delayMs) => {
      const timer = setTimeout(task, delayMs);
      return () => clearTimeout(timer);
    }),
    storage,
    recordKey: RECORD_KEY,
    random: cryptoRandom,
    now: Date.now,
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
  const served = serveConversations(
    connectChatV2(),
    {
      conversations: () => engine.conversations(),
      messages: (id) => engine.messages(id),
      send: (id, text) => engine.send(id, text),
      retry: (id, messageId) => engine.retry(id, messageId),
      start: (target, welcome) => {
        state.starts = [...state.starts, JSON.stringify(target)];
        return engine.start(target, welcome);
      },
      onChange: (listener) => engine.onChange(listener),
    },
    { reads, onError: (what, reason) => report(`${what}: ${reason}`) },
  );
  state.pickAndStart = async (welcome) => {
    const picked = await client.contacts.pick({});
    if (picked.isErr()) throw new Error(JSON.stringify(picked.error));
    const outcome = picked.value.outcome;
    if (outcome.tag !== "Picked") return { outcome: outcome.tag };
    return {
      roomId: await engine.start({ handle: outcome.value.handle }, welcome),
    };
  };
  state.cancel = (roomId) => engine.cancel(roomId);
  // What the developer tools may see of each conversation, at most twice a second.
  let diagnosticsTimer: ReturnType<typeof setTimeout> | undefined;
  const postReport = () => {
    diagnosticsTimer = undefined;
    postDiagnostics({ conversations: engine.diagnostics() });
  };
  engine.onDiagnostics(() => {
    diagnosticsTimer ??= setTimeout(postReport, 500);
  });
  engine.run();
  postReport();
  await served.ready;
  state.stage = "running";
}

run().catch((error: unknown) => {
  state.stage = "failed";
  state.reason = error instanceof Error ? error.message : String(error);
});
