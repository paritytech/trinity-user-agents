/**
 * PROTOTYPE. Kettle Chat: a chat product of its own (`kettle-chat.<network
 * suffix>`) running Kettle (`paritytech/kettle-core`, MLS over the Statement
 * Store), alone or beside Contact Chat. The host runs it when the developer
 * tools' Chat tab turns it on (`tools/chat/products.ts`), on the dev server or
 * in a build made with `WEB_SIGNING_KETTLE_CHAT=1`: for a signed-in wallet
 * over the network, or for the invitation demo over a simulated Statement
 * Store.
 *
 * In its frame, as for Contact Chat (`../contact-chat/worker.ts`):
 *
 * - the host's Chat screen shows its conversations over the Chat Extension
 *   v2 draft bridge, and names each person from the contact handle;
 * - setup goes through the proposed host request service, with Kettle Chat's
 *   payloads (`./engine.ts`);
 * - Kettle runs as WebAssembly from this worker's own archive
 *   (`kettle-chat-wasm/`), one node per conversation account, over the
 *   product's TrUAPI Statement Store, entropy and storage (`./ports.ts`);
 * - when the host asks for iroh in the page's address ({@link irohOfParams}),
 *   each node also runs Kettle's log transfer, on n0's public test relays or
 *   one custom relay. The host only chooses the relays; the endpoints are
 *   this product's.
 *
 * It says on its console which Kettle it runs and where it stands
 * (`../worker/probe.ts`); with its test probe, what it keeps is on
 * `window.__kettleChatWorker`, for a test to read.
 */
import { getClientSync } from "@parity/truapi/sandbox";
import { connectChatV2 } from "../draft/product.js";
import { serveConversations, type ReadMarks } from "../draft/conversations.js";
import { ProductStorage } from "../worker/product-storage.js";
import { postDiagnostics } from "../../products/diagnostics.js";
import { cryptoRandom } from "../../invitations/sealing.js";
import { connectHostRequests } from "../contact-chat/requests.js";
import { KettleChat } from "./engine.js";
import {
  conversationPorts,
  loadKettle,
  type IrohStatus,
  type KettleModule,
  type KettlePorts,
  type KettleSubmission,
  type KettleTrace,
} from "./ports.js";
import { BAD_RELAY_URL, irohOfParams, type IrohRelays } from "./route.js";
import {
  installProbe,
  probeAsked,
  workerStatus,
  type ProbeStatus,
} from "../worker/probe.js";

/** What Kettle Chat's test probe holds. */
interface KettleChatProbe extends ProbeStatus {
  /** The Kettle revision and protocol the WebAssembly reports. */
  kettle?: { revision: string; protocol: number; loadMs: number };
  /** Kettle's statement submissions this run, by the store's answer or the error's text. */
  submits: Record<string, number>;
  /** Every statement submitted this run, signed, with the store's answer. */
  submissions: KettleSubmission[];
  /** What each node did this run, newest last: its start and its events, by account prefix. */
  trace: string[];
  /** The iroh relays the host asked for, or null: no log transfer. */
  iroh: IrohRelays | null;
  /** What each node runs of the log transfer, by account. */
  nodes: Record<string, IrohStatus>;
  /** Each node's log transfers and the peer outboxes it read this run, newest last. */
  transfers: (KettleTrace & { account: string; at: number })[];
  record(): string | null;
}

const RECORD_KEY = "kettle-chat:record:v1";
const READS_KEY = "kettle-chat:read:v1";

const probe =
  __WORKER_PROBE__ && probeAsked()
    ? installProbe<KettleChatProbe>("__kettleChatWorker", {
        stage: "starting",
        errors: [],
        submits: {},
        submissions: [],
        trace: [],
        iroh: null,
        nodes: {},
        transfers: [],
        record: () => null,
      })
    : null;
const status = workerStatus("Kettle Chat", probe, 50);
const report = status.report;

/**
 * `kettle`, saying when a node runs without the log transfer it was asked
 * for, and with each node's start and events kept on `into` when there is
 * one.
 */
function started(
  kettle: KettleModule,
  into: KettleChatProbe | null,
): KettleModule {
  const trace = (line: string) => {
    if (into === null) return;
    into.trace = [
      ...into.trace,
      `${new Date().toISOString().slice(11, 23)} ${line}`,
    ].slice(-300);
  };
  return {
    ...kettle,
    start: async (account, ports, onEvent, options) => {
      const who = account.slice(0, 10);
      trace(`${who} start`);
      try {
        const node = await kettle.start(
          account,
          ports,
          into === null
            ? onEvent
            : (event) => {
                trace(
                  `${who} ${event.type}${typeof event.kind === "string" ? ` ${event.kind}` : ""}${typeof event.message === "string" ? ` ${event.message}` : ""}`,
                );
                onEvent(event);
              },
          into === null
            ? options
            : {
                ...options,
                onTrace: (event: KettleTrace) => {
                  into.transfers = [
                    ...into.transfers,
                    { ...event, account, at: Date.now() },
                  ].slice(-500);
                },
              },
        );
        const iroh = node.iroh();
        if (into !== null) into.nodes = { ...into.nodes, [account]: iroh };
        trace(`${who} started · log transfer ${iroh.state}`);
        if (iroh.state === "unavailable")
          report(
            `iroh: ${iroh.reason}${iroh.relay === undefined ? "" : ` (${iroh.relay})`}; this chat runs on the Statement Store alone`,
          );
        return node;
      } catch (error) {
        trace(
          `${who} start failed: ${error instanceof Error ? error.message : String(error)}`,
        );
        throw error;
      }
    },
  };
}

/** `ports`, with each submission's outcome counted in {@link KettleChatProbe.submits}. */
function counted(ports: KettlePorts, into: KettleChatProbe): KettlePorts {
  const count = (key: string) => {
    into.submits = { ...into.submits, [key]: (into.submits[key] ?? 0) + 1 };
  };
  return {
    ...ports,
    submit: async (statement) => {
      try {
        const answer = await ports.submit(statement);
        count(
          answer.reason === undefined
            ? answer.status
            : `${answer.status}: ${answer.reason}`,
        );
        return answer;
      } catch (error) {
        count(
          `error: ${error instanceof Error ? error.message : String(error)}`,
        );
        throw error;
      }
    },
  };
}

/** The relays the host asked for in this frame's address. A value this does not read runs none, said without repeating it. */
function irohRelays(): IrohRelays | null {
  const { iroh, refused } = irohOfParams(new URLSearchParams(location.search));
  if (refused) report(`iroh: ${BAD_RELAY_URL} This chat runs without it.`);
  return iroh;
}

async function run(): Promise<void> {
  const client = getClientSync();
  if (client === null) throw new Error("No TrUAPI host.");
  const loadStarted = performance.now();
  const kettle = await loadKettle(import.meta.url);
  const loaded = {
    revision: kettle.revision,
    protocol: kettle.protocol,
    loadMs: Math.round(performance.now() - loadStarted),
  };
  console.info(
    `Kettle Chat runs Kettle ${loaded.revision}, protocol ${loaded.protocol}, loaded in ${loaded.loadMs} ms.`,
  );
  if (probe !== null) probe.kettle = loaded;
  const storage = await ProductStorage.load(
    client,
    [RECORD_KEY, READS_KEY],
    (reason) => report(`storage: ${reason}`),
  );
  if (probe !== null) probe.record = () => storage.getItem(RECORD_KEY);
  const context = await client.system.getProductContext();
  if (context.isErr())
    throw new Error(`No product context: ${JSON.stringify(context.error)}`);
  const productId = context.value.productId;
  const iroh = irohRelays();
  if (probe !== null) probe.iroh = iroh;
  const engine = new KettleChat({
    requests: connectHostRequests(),
    kettle: started(kettle, probe),
    account: async (index) => {
      const result = await client.account.getAccount({
        productAccountId: {
          dotNsIdentifier: productId,
          derivationIndex: { tag: "Raw", value: index as `0x${string}` },
        },
      });
      if (result.isErr()) throw new Error(JSON.stringify(result.error));
      return result.value.account.publicKey.toLowerCase();
    },
    ports: (index, allow) => {
      const ports = conversationPorts({
        client,
        productId,
        index,
        allow,
        report,
        ...(probe === null
          ? {}
          : {
              observe: (submission: KettleSubmission) => {
                probe.submissions = [...probe.submissions, submission].slice(
                  -200,
                );
              },
            }),
      });
      return probe === null ? ports : counted(ports, probe);
    },
    storage,
    recordKey: RECORD_KEY,
    random: cryptoRandom,
    now: Date.now,
    report,
    ...(iroh === null ? {} : { iroh }),
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
      start: (target, welcome) => engine.start(target, welcome),
      onChange: (listener) => engine.onChange(listener),
    },
    { reads, onError: (what, reason) => report(`${what}: ${reason}`) },
  );
  let diagnosticsTimer: ReturnType<typeof setTimeout> | undefined;
  engine.onDiagnostics(() => {
    diagnosticsTimer ??= setTimeout(() => {
      diagnosticsTimer = undefined;
      postDiagnostics({ conversations: engine.diagnostics() });
    }, 500);
  });
  engine.run();
  postDiagnostics({ conversations: engine.diagnostics() });
  await served.ready;
  status.running();
}

run().catch((error: unknown) =>
  status.failed(error instanceof Error ? error.message : String(error)),
);
