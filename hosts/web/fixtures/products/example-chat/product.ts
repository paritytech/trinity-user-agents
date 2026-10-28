import { getClientSync } from "@parity/truapi/sandbox";
import {
  PRODUCT_HELLO,
  PRODUCT_PROTOCOL,
  type CallResult,
  type ContactHandle,
  type HostMessage,
  type IncomingRequest,
  type OutgoingView,
  type ProductHello,
  type RepliedView,
} from "../../../src/invitations/contract.js";
import {
  cryptoRandom,
  fromHex,
  fromUtf8,
  toHex,
  utf8,
} from "../../../src/invitations/sealing.js";
import {
  checkResponse,
  createInvitation,
  respond,
  type SenderSecrets,
  type Topics,
} from "./setup.js";

/**
 * DEV ONLY. "Example Chat", a test product. The dev server packs it as an app
 * archive (`chat-product-demo-plugin.ts`), and the host opens it as it opens
 * any product by name: through the loader, which checks the archive, and over
 * TrUAPI to the core.
 *
 * - **Pick a contact** calls `contacts.pick`. The host draws its picker; the
 *   product gets the core's opaque handle, or why there is none.
 * - **New chat** uses the proposed request service
 *   (`src/invitations/contract.ts`): choose a contact through the host and get
 *   a contact handle, make the invitation payload with `./setup.ts`, and ask
 *   the host to send it. A reply reaches this product only after its host
 *   authenticated it; the product checks it with `./setup.ts`, keeps the
 *   conversation topics, and only then calls `complete`. Waiting invitations
 *   can be cancelled.
 * - When the user chooses Accept in the host's Notifications, the host hands
 *   it the invitation payload. It checks the payload before anything is sent,
 *   makes both conversation topics and its response, and replies through the
 *   host with the request's handle. It never sees a route, key or proof.
 *
 * Its secrets and topics are kept in its TrUAPI local storage. It would watch
 * its topics through TrUAPI's `statementStore`; a test user reaches no chain,
 * so it only keeps them.
 */

const STATE_KEY = "example-chat:invitations:v2";

const STATUS: Record<string, string> = {
  waiting: "Waiting",
  completed: "Connected",
  cancelled: "Cancelled",
  expired: "Expired",
};

const STYLE = `
:root { color-scheme: dark; font: 15px/20px system-ui, -apple-system, "Segoe UI", sans-serif; }
body { margin: 0; background: #0b0c0f; color: #f4f4f5; }
main { display: grid; gap: 20px; padding: 20px 16px 32px; }
h1 { margin: 0; font-size: 28px; line-height: 34px; font-weight: 700; }
h2 { margin: 0 0 8px; font-size: 13px; font-weight: 600; color: #8c8f98; }
.card { display: grid; gap: 10px; padding: 16px; border-radius: 20px; background: #17181c; }
.card h2 { margin: 0; }
textarea, select { font: inherit; padding: 10px 12px; border-radius: 12px; border: 1px solid #2a2b33; background: #0b0c0f; color: inherit; min-width: 0; resize: none; }
button { font: inherit; font-weight: 600; min-height: 44px; border-radius: 999px; cursor: pointer; }
.primary { border: none; background: #f4f4f5; color: #0b0c0f; }
.secondary { border: 1px solid #2a2b33; background: transparent; color: inherit; }
.small { min-height: 32px; padding: 0 14px; font-size: 13px; justify-self: start; }
button:disabled { opacity: .4; cursor: default; }
.note { margin: 0; font-size: 13px; color: #8c8f98; overflow-wrap: anywhere; }
.list { margin: 0 -16px; padding: 0; list-style: none; }
.row { display: grid; gap: 2px; padding: 10px 16px; border-bottom: 1px solid #2a2b33; }
.row-top { display: flex; justify-content: space-between; gap: 12px; }
.row-top strong { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.status { flex: none; font-size: 13px; color: #8c8f98; }
.row[data-status="completed"] .status, .row[data-status="accepted"] .status { color: #4ade80; }
.row[data-status="cancelled"] .status, .row[data-status="expired"] .status { color: #ff8f8f; }
.detail { font-size: 14px; color: #8c8f98; overflow-wrap: anywhere; }
`;

function make<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== "") node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const shortHex = (hex: string): string => `${hex.slice(2, 6)}…${hex.slice(-4)}`;
const shortContact = (contact: ContactHandle): string =>
  shortHex(contact.bytes);

const clock = (at: number): string =>
  new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/** A refused call, as the product shows it. */
function describe(error: unknown): string {
  const tags: string[] = [];
  let value = error as { tag?: unknown; value?: unknown } | undefined;
  while (typeof value?.tag === "string") {
    tags.push(value.tag);
    value = value.value as typeof value;
  }
  return tags.length > 0 ? tags.join(" · ") : String(error);
}

function row(
  id: string,
  title: string,
  status: string,
  statusText: string,
  detail: string,
  action?: HTMLButtonElement,
): HTMLLIElement {
  const item = make("li", "row");
  item.dataset.id = id;
  item.dataset.status = status;
  const top = make("div", "row-top");
  top.append(make("strong", "", title), make("span", "status", statusText));
  item.append(top, make("span", "detail", detail));
  if (action !== undefined) item.append(action);
  return item;
}

function section(root: HTMLElement, title: string, rows: HTMLLIElement[]) {
  const list = make("ul", "list");
  list.append(...rows);
  root.hidden = rows.length === 0;
  root.replaceChildren(make("h2", "", title), list);
}

/** An invitation this product sent: its secrets, and the topics once connected. */
interface SentState {
  secrets: SenderSecrets;
  topics: Topics | null;
}

/** An invitation this product replied to: its protocol secret and the topics it made. */
interface ReceivedState {
  protocolSecret: string;
  topics: Topics;
}

interface State {
  /** By request handle, or by a local id while the host decides. */
  sent: Record<string, SentState>;
  received: Record<string, ReceivedState>;
}

const client = getClientSync();
if (client === null) throw new Error("Example Chat needs a TrUAPI host.");

/** This product's own storage, through TrUAPI. */
async function loadState(): Promise<State> {
  const empty: State = { sent: {}, received: {} };
  const read = await client!.localStorage.read({ key: STATE_KEY });
  if (read.isErr() || read.value.value === undefined) return empty;
  try {
    const saved = JSON.parse(fromUtf8(fromHex(read.value.value))) as State;
    return { sent: saved.sent ?? {}, received: saved.received ?? {} };
  } catch {
    return empty;
  }
}

let saving: Promise<unknown> = Promise.resolve();
function saveState(): Promise<unknown> {
  const value = toHex(utf8(JSON.stringify(state)));
  saving = saving.then(() =>
    client!.localStorage.write({
      key: STATE_KEY,
      value: value as `0x${string}`,
    }),
  );
  return saving;
}

const style = make("style");
style.textContent = STYLE;
document.head.append(style);

// Contacts: the product asks, the host's picker answers with a handle.
const picked: string[] = [];
let picking = false;
let pickNote = "";
const pick = make("button", "secondary", "Pick a contact");
pick.type = "button";
pick.dataset.action = "pick-contact";
const pickStatus = make("p", "note");
pickStatus.setAttribute("role", "status");
const contacts = make("section", "card");
contacts.dataset.card = "contacts";
contacts.append(make("h2", "", "Contacts"), pick, pickStatus);
const pickedList = make("section");
pickedList.dataset.list = "picked";

// Invitations.
let state: State = { sent: {}, received: {} };
const sent = new Map<string, OutgoingView>();
const replied = new Map<string, RepliedView>();
let chosen: ContactHandle | null = null;
let busy = false;
let inviteNote = "";
const choose = make("button", "secondary", "Choose contact");
choose.type = "button";
choose.dataset.action = "choose-contact";
const purpose = make("textarea");
purpose.name = "purpose";
purpose.rows = 2;
purpose.maxLength = 280;
purpose.value = "Let's chat in Example Chat.";
purpose.setAttribute("aria-label", "Message");
const expiry = make("select");
expiry.name = "expiry";
expiry.setAttribute("aria-label", "Expires after");
for (const [label, ms] of [
  ["2 minutes", 2 * 60_000],
  ["1 hour", 60 * 60_000],
  ["1 day", 24 * 60 * 60_000],
] as const) {
  const option = make("option", "", `Expires in ${label}`);
  option.value = String(ms);
  option.selected = label === "1 hour";
  expiry.append(option);
}
const invite = make("button", "primary", "Invite");
invite.type = "button";
invite.dataset.action = "invite";
const inviteStatus = make("p", "note");
inviteStatus.setAttribute("role", "status");
const form = make("section", "card");
form.dataset.card = "invite";
form.append(
  make("h2", "", "New chat"),
  choose,
  purpose,
  expiry,
  invite,
  inviteStatus,
);
const sentList = make("section");
sentList.dataset.list = "sent";
const acceptedList = make("section");
acceptedList.dataset.list = "accepted";

const main = make("main");
main.append(
  make("h1", "", "Example Chat"),
  contacts,
  pickedList,
  form,
  sentList,
  acceptedList,
);
document.body.append(main);

function render(): void {
  pick.disabled = picking;
  pickStatus.textContent = picking ? "Waiting for the host…" : pickNote;
  pickStatus.hidden = pickStatus.textContent === "";
  section(
    pickedList,
    "Picked",
    picked.map((handle, index) =>
      row(
        handle,
        `Contact ${shortHex(handle)}`,
        "picked",
        `#${index + 1}`,
        handle,
      ),
    ),
  );

  choose.disabled = busy;
  choose.textContent =
    chosen === null ? "Choose contact" : `To contact ${shortContact(chosen)}`;
  choose.dataset.contact = chosen?.bytes ?? "";
  invite.disabled = busy || chosen === null;
  inviteStatus.textContent = busy ? "Waiting for the host…" : inviteNote;
  inviteStatus.hidden = inviteStatus.textContent === "";
  section(
    sentList,
    "Invitations",
    [...sent.values()]
      .sort((left, right) => right.createdAt - left.createdAt)
      .map((item) => {
        const when =
          item.status === "waiting"
            ? `Expires ${clock(item.deadline)}`
            : item.status === "expired"
              ? `Expired ${clock(item.deadline)}`
              : clock(item.closedAt ?? item.deadline);
        let action: HTMLButtonElement | undefined;
        if (item.status === "waiting") {
          action = make("button", "secondary small", "Cancel");
          action.type = "button";
          action.dataset.action = "cancel-invitation";
          action.addEventListener("click", () => void cancel(item.handle));
        }
        return row(
          item.handle,
          item.purpose,
          item.status,
          STATUS[item.status]!,
          `Contact ${shortContact(item.contact)} · ${when}`,
          action,
        );
      }),
  );
  section(
    acceptedList,
    "Accepted by you",
    [...replied.values()]
      .sort((left, right) => right.repliedAt - left.repliedAt)
      .map((item) =>
        row(
          item.handle,
          `Chat with contact ${shortContact(item.sender)}`,
          "accepted",
          "Replied",
          clock(item.repliedAt),
        ),
      ),
  );
}

pick.addEventListener("click", async () => {
  if (picking) return;
  picking = true;
  pickNote = "";
  render();
  const result = await client.contacts.pick({});
  picking = false;
  if (result.isErr()) {
    pickStatus.dataset.outcome = "Error";
    pickNote = `Not available: ${describe(result.error)}`;
  } else {
    const outcome = result.value.outcome;
    pickStatus.dataset.outcome = outcome.tag;
    if (outcome.tag === "Picked") picked.push(outcome.value.handle.bytes);
    pickNote =
      outcome.tag === "Picked"
        ? ""
        : outcome.tag === "Dismissed"
          ? "No contact picked."
          : "No contacts yet.";
  }
  render();
});

const channel = new MessageChannel();
const port = channel.port1;
let nextCall = 1;
const calls = new Map<number, (result: CallResult) => void>();

function call(fields: Record<string, unknown>): Promise<CallResult> {
  const id = nextCall;
  nextCall += 1;
  return new Promise((resolve) => {
    calls.set(id, resolve);
    port.postMessage({ id, ...fields });
  });
}

choose.addEventListener("click", async () => {
  if (busy) return;
  busy = true;
  inviteNote = "";
  render();
  const result = await call({ call: "pickContact" });
  busy = false;
  if (result.ok && result.outcome?.tag === "Picked")
    chosen = result.outcome.value.handle;
  else inviteNote = "No contact chosen.";
  render();
});

invite.addEventListener("click", async () => {
  if (busy || chosen === null) return;
  busy = true;
  inviteNote = "";
  render();
  const { payload, secrets } = createInvitation(cryptoRandom);
  // The secrets are kept before the invitation leaves, under a local id until the host names it.
  const local = `local:${toHex(cryptoRandom(8))}`;
  state.sent[local] = { secrets, topics: null };
  await saveState();
  const result = await call({
    call: "request",
    request: {
      contact: chosen,
      purpose: purpose.value,
      ttlMs: Number(expiry.value),
      payload,
    },
  });
  busy = false;
  delete state.sent[local];
  if (result.ok && result.request !== undefined) {
    sent.set(result.request.handle, result.request);
    state.sent[result.request.handle] = { secrets, topics: null };
    chosen = null;
  } else if (!result.ok)
    inviteNote = result.reason === "Dismissed" ? "Not sent." : result.detail;
  void saveState();
  render();
});

async function cancel(handle: string): Promise<void> {
  const result = await call({ call: "cancel", handle });
  if (result.ok && result.request !== undefined)
    sent.set(handle, result.request);
  render();
}

/** Replies being checked, so two copies of one are checked once. */
const checking = new Set<string>();

/** An authenticated reply from the host: check it, keep the topics, then complete. */
async function responseArrived(handle: string, payload: string): Promise<void> {
  const entry = state.sent[handle];
  if (entry === undefined || sent.get(handle)?.status !== "waiting") return;
  if (checking.has(payload)) return;
  const checked = checkResponse(entry.secrets, payload);
  if (!checked.ok) return;
  checking.add(payload);
  try {
    entry.topics = checked.topics;
    await saveState();
    const completed = await call({ call: "complete", handle });
    if (completed.ok && completed.request !== undefined)
      sent.set(handle, completed.request);
    render();
  } finally {
    checking.delete(payload);
  }
}

/** The user chose Accept: check the invitation, make the topics and response, reply through the host. */
async function acceptIntent(request: IncomingRequest): Promise<void> {
  const { handle, payload } = request;
  if (state.received[handle] !== undefined) return;
  const made = respond(payload, cryptoRandom);
  if (made === null) {
    void call({
      call: "refuse",
      handle,
      reason: "Example Chat cannot use this invitation.",
    });
    return;
  }
  state.received[handle] = {
    protocolSecret: made.protocolSecret,
    topics: made.topics,
  };
  await saveState();
  const result = await call({
    call: "reply",
    handle,
    payload: made.response,
    open: true,
  });
  if (!result.ok) {
    delete state.received[handle];
    void saveState();
    inviteNote = `Not accepted: ${result.detail}`;
    render();
    return;
  }
  replied.set(handle, {
    handle,
    sender: request.sender,
    repliedAt: Date.now(),
  });
  render();
}

port.onmessage = (event: MessageEvent<HostMessage>) => {
  const message = event.data;
  if (message.kind === "result") {
    calls.get(message.id)?.(message.result);
    calls.delete(message.id);
  } else if (message.kind === "outgoing")
    sent.set(message.request.handle, message.request);
  else if (message.kind === "response")
    void responseArrived(message.handle, message.payload);
  else if (message.kind === "replied")
    replied.set(message.request.handle, message.request);
  else if (message.kind === "acceptIntent") void acceptIntent(message.request);
  else if (message.kind === "openIntent")
    void call({ call: "open", handle: message.handle });
  render();
};

render();
void loadState().then((saved) => {
  state = saved;
  render();
  parent.postMessage(
    { type: PRODUCT_HELLO, protocol: PRODUCT_PROTOCOL } satisfies ProductHello,
    location.origin,
    [channel.port2],
  );
});
