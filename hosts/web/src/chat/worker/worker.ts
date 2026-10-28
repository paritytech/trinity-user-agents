/**
 * DEV ONLY. The Nova chat worker: the host's Nova-compatible chat engine
 * (`../nova/engine.ts`) running inside an ordinary product worker, as a chat
 * product would run it.
 *
 * The dev server builds this module into an app archive
 * (`chat-product-demo-plugin.ts`), and the host runs it through the ordinary
 * worker loader, as a `Worker` execution, when the user switches chat to the
 * worker (`tools/demos/worker-mode.ts`). Here, in the product's frame:
 *
 * - the engine owns the protocol, the history, delivery and acknowledgements;
 * - the Statement Store, chain reads and storage go through the product's
 *   own TrUAPI client (`./truapi-ports.ts`), scoped to this product;
 * - the wallet's chat keys stay with the host, which lends their operations
 *   over the identity bridge (`../draft/identity-bridge.ts`);
 * - the host's Chat screen shows the conversations over the Chat Extension
 *   v2 draft bridge (`../draft/conversations.ts`).
 *
 * What it does is on `window.__novaChatWorker`, for a test to read.
 */
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { getClientSync } from "@parity/truapi/sandbox";
import { ChatEngine, type ChatSnapshot } from "../nova/engine.js";
import { connectChatV2 } from "../draft/product.js";
import { serveConversations, type ReadMarks } from "../draft/conversations.js";
import { requestNovaIdentity } from "../draft/identity-bridge.js";
import { novaConversations } from "./nova-conversations.js";
import {
  ProductStorage,
  truapiDirectory,
  truapiStatementStore,
} from "./truapi-ports.js";

interface WorkerState {
  stage: "starting" | "running" | "failed";
  reason?: string;
  /** What became of an offered copy of the host's history. */
  history: "none offered" | "copied" | "kept own history";
  identity?: HexString;
  errors: string[];
  snapshot(): ChatSnapshot | null;
  /** The rooms as this worker declares them to the host: ids and its own titles. */
  rooms(): { id: string; title: string }[];
}

const state: WorkerState = {
  stage: "starting",
  history: "none offered",
  errors: [],
  snapshot: () => null,
  rooms: () => [],
};
(window as unknown as { __novaChatWorker: WorkerState }).__novaChatWorker =
  state;
const report = (reason: string) => {
  state.errors = [...state.errors, reason].slice(-20);
};

async function run(): Promise<void> {
  const client = getClientSync();
  if (client === null) throw new Error("No TrUAPI host.");
  const { identity, takeHistory, resolveContact } = await requestNovaIdentity();
  state.identity = bytesToHex(identity.identity);

  const chain = async (name: "AssetHub" | "People") => {
    const info = await client.chain.getChainInfo({ chain: name });
    if (info.isErr())
      throw new Error(
        `The host serves no ${name} chain: ${JSON.stringify(info.error)}`,
      );
    return info.value;
  };
  const [assetHub, people] = await Promise.all([
    chain("AssetHub"),
    chain("People"),
  ]);

  // One record per network and identity, in this product's storage.
  const recordKey = `nova-chat:${people.network}:${state.identity}:v1`;
  const readsKey = `${recordKey}:read`;
  const storage = await ProductStorage.load(
    client,
    [recordKey, readsKey],
    (reason) => report(`storage: ${reason}`),
  );
  const offered = await takeHistory();
  if (offered !== null) {
    if (storage.getItem(recordKey) === null) {
      storage.setItem(recordKey, offered);
      state.history = "copied";
    } else state.history = "kept own history";
  }

  const permission = await client.permissions.requestRemotePermission({
    permission: { tag: "StatementSubmit" },
  });
  if (permission.isErr())
    report(`statement permission: ${JSON.stringify(permission.error)}`);

  const engine = new ChatEngine({
    identity,
    openStore: async () => truapiStatementStore(client),
    directory: truapiDirectory(client, {
      assetHub: assetHub.genesisHash,
      people: people.genesisHash,
    }),
    storage,
    recordKey,
    writes: () => true,
  });
  state.snapshot = () => engine.snapshot();

  const marks = (): Record<string, string> => {
    try {
      return JSON.parse(storage.getItem(readsKey) ?? "{}") as Record<
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
        readsKey,
        JSON.stringify({ ...marks(), [conversationId]: messageId }),
      ),
  };
  const chat = connectChatV2();
  const conversations = novaConversations(engine, { resolveContact });
  state.rooms = () =>
    conversations.conversations().map(({ id, title }) => ({ id, title }));
  const served = serveConversations(chat, conversations, {
    reads,
    onError: (what, reason) => report(`${what}: ${reason}`),
  });
  await engine.start();
  await served.ready;
  state.stage = "running";
}

run().catch((error: unknown) => {
  state.stage = "failed";
  state.reason = error instanceof Error ? error.message : String(error);
});
