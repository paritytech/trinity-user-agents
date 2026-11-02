import "./statements-view.css";
import { copyInPage } from "../../src/app/copy.js";
import type { PeopleAccess } from "./actions.js";
import { readGrants } from "./actions.js";
import { evidenceText } from "./evidence.js";
import { TRANSPORT_NOTE, nodeText } from "./decode.js";
import {
  HASHES_PER_SUBSCRIPTION,
  expiryText,
  filterText,
  metaDetails,
  short,
  type AccountRecord,
  type EventKind,
  type FactSource,
  type LedgerEvent,
  type Role,
  type StatementLedger,
  type SubscriptionRecord,
} from "./ledger.js";
import {
  bytes,
  button,
  cell,
  clock,
  el,
  fact,
  more,
  openDisclosures,
  reopenDisclosures,
  tag,
} from "./ui.js";

/**
 * The inspector's Statements view: what this tab saw of the Statement Store,
 * by account, topic and time, with where each fact came from. It knows no
 * chat protocol: conversations, as products report them, are the Chat
 * view's (`tools/chat/`), which links here for their accounts and topics.
 *
 * It draws the ledger (`ledger.ts`) and runs the two reads on demand
 * (`actions.ts`). It draws only while shown, at most a few times a second,
 * and keeps a row's open state and the selected account across draws.
 */

export interface StatementsViewDeps {
  ledger: StatementLedger;
  /** The People chain for grant reads, or null before the network is up. */
  people(): PeopleAccess | null;
  network(): string;
  /** Show `productId`'s TrUAPI traces, as the Workers view does. */
  showTraces(productId: string): void;
  /** Show `name`'s worker in the Workers view. */
  inspectWorker(name: string): void;
}

export interface StatementsView {
  show(visible: boolean): void;
  /** Open on `target`: an account selected, or the timeline narrowed to a topic or a request. */
  reveal(target: StatementsTarget): void;
  /** The session changed: stop capturing, and drop what was selected. */
  sessionChanged(): void;
}

type Tab = "accounts" | "topics" | "timeline";

/** What another view asks the Statements view to show. */
export type StatementsTarget =
  | { account: string }
  | { topic: string }
  | { handle: string; label: string };

const TABS: { value: Tab; label: string }[] = [
  { value: "accounts", label: "Accounts" },
  { value: "topics", label: "Topics" },
  { value: "timeline", label: "Timeline" },
];

const KINDS: { value: EventKind; label: string }[] = [
  { value: "submit", label: "Submits" },
  { value: "page", label: "Pages" },
  { value: "subscription", label: "Subscriptions" },
  { value: "claim", label: "Claims" },
  { value: "request", label: "Requests" },
  { value: "grant", label: "Grants" },
  { value: "connection", label: "Connections" },
  { value: "session", label: "Session" },
];

/** How often the view draws at most while shown. */
const DRAW_MS = 250;

function roleText(role: Role): string {
  switch (role.kind) {
    case "product-account":
      return `${role.product}'s account at Raw(${short(role.index, 4)})`;
    case "notification-account":
      return role.post === undefined
        ? "notification account"
        : `notification account of post ${short(role.post, 4)}, day ${role.period}`;
    case "conversation-account":
      return `${role.product} says it posts as this account`;
    case "conversation-peer":
      return `${role.product} says it reads this account`;
    case "chat-allowance":
      return `allowed for request ${short(role.handle, 4)}`;
  }
}

/** A role in a few words, for the accounts list; the detail says it in full. */
function roleLabel(role: Role): string {
  switch (role.kind) {
    case "product-account":
      return "Product account";
    case "notification-account":
      return "Setup notification";
    case "conversation-account":
      return "Chat sender";
    case "conversation-peer":
      return "Chat peer";
    case "chat-allowance":
      return "Chat allowance";
  }
}

/** The accounts list's columns. */
const COLUMNS = ["Account", "Role", "Allocation", "Grant", "Observed"];

/** What each column of the accounts list says, and what it does not. */
const COLUMNS_ABOUT = [
  "Role: what the host or core recorded. Reported: what a product said, unchecked.",
  "Allocation: the latest daily slot claim this tab saw.",
  "Grant: statements · size the chain grants, read at a block. Not what is left.",
  "Observed: statements · size · last seen in this tab. Not what the network holds now.",
];

function roleSource(role: Role): FactSource {
  return role.kind === "conversation-account" ||
    role.kind === "conversation-peer"
    ? "product"
    : role.kind === "product-account"
      ? "core"
      : "host";
}

/**
 * A claim: an allowance slot for one UTC day, which the core claimed with the
 * person's membership. It is not the person's own registration.
 */
function claimLine(claim: AccountRecord["claims"][number]): string {
  const slot =
    claim.seq === undefined
      ? "allowance slot"
      : `allowance slot ${claim.seq}${claim.collection === undefined ? "" : ` (${claim.collection})`}`;
  switch (claim.outcome) {
    case "registered":
      return `${slot} · day ${claim.period} · claimed now`;
    case "alreadyAllocated":
      return `${slot} · day ${claim.period} · already held`;
    case "validated":
      return `${slot} · validated only · nothing allocated`;
    case "failed":
      return `no slot: ${claim.reason ?? "no reason"}`;
  }
}

/** An account's latest claim, in a few words; the detail has its day, collection and time. */
function allocationLabel(account: AccountRecord): string {
  const claim = account.claims.at(-1);
  if (claim === undefined) return "None seen";
  const slot = claim.seq === undefined ? "Slot" : `Slot ${claim.seq}`;
  switch (claim.outcome) {
    case "registered":
      return `${slot} · claimed`;
    case "alreadyAllocated":
      return `${slot} · held`;
    case "validated":
      return "Validated only";
    case "failed":
      return "Claim failed";
  }
}

/** An account's grant, in a few words. */
function grantLabel(account: AccountRecord): string {
  const grant = account.grant;
  if (grant === undefined) return "Not read";
  if ("error" in grant) return "Read failed";
  return grant.value === null
    ? "No grant"
    : `${grant.value.maxCount} · ${bytes(grant.value.maxSize)}`;
}

/** An account's latest observed statements, in a few words. */
function observedLabel(
  ledger: StatementLedger,
  account: AccountRecord,
): string {
  const observed = ledger.latestObserved(account.account);
  if (observed.lastObservedAt === null) return "Not observed";
  return `${observed.statements} · ${bytes(observed.dataBytes)} · ${clock(observed.lastObservedAt).slice(0, 5)}`;
}

function grantText(account: AccountRecord): {
  text: string;
  note?: string;
} {
  const grant = account.grant;
  if (grant === undefined) return { text: "not read" };
  if ("error" in grant) return { text: `read failed: ${grant.error}` };
  return {
    text:
      grant.value === null
        ? "no grant stored"
        : `${grant.value.maxCount} stmts · ${bytes(grant.value.maxSize)} · not remaining`,
    note: `Read at block ${short(grant.block)}, ${clock(grant.at)}. A grant, not what is used or what is left.`,
  };
}

/** An account's latest observed statements, said as an observation: never as occupancy or room left. */
function latestObservedText(
  ledger: StatementLedger,
  account: AccountRecord,
): string {
  const observed = ledger.latestObserved(account.account);
  const notes = [
    observed.expired > 0 ? `${observed.expired} expired` : null,
    observed.dropped > 0 ? `${observed.dropped} dropped` : null,
  ].filter((note) => note !== null);
  const extra = notes.length === 0 ? "" : ` · ${notes.join(" · ")}`;
  if (observed.lastObservedAt === null)
    return `not observed${extra} · not the same as none stored`;
  return `${observed.statements} stmt${observed.statements === 1 ? "" : "s"} · ${bytes(observed.dataBytes)} · seen ${clock(observed.lastObservedAt)}${extra} · not current`;
}

/** What the latest observed statements do not say. */
const LATEST_OBSERVED_NOTE =
  'Latest observed per channel, not known expired, this tab\'s own broadcasts included. Not what the network holds now: peers may have refused or evicted them without saying so, or hold others no one sent here. "Dropped" counts channels whose latest statement left the list.';

/** Why the view offers no probe, and what an observed subscription does not show. */
const NO_PROBE_NOTE =
  "In this host's light client every People connection shares one topic filter that is sent to peers, and each connection's subscriptions replace it, so a subscription made to look would take the chat's topics off it. An open subscription below is one the core or the host made; it does not show that peers send to it.";

/** The one thing to know before reading any fact here. */
const TAKEAWAY = "Observed traffic, not network state";

/** What every fact depends on, one short line each; the full transport note is in the evidence. */
const ABOUT = [
  "Source: this tab's light client. It holds no statements.",
  "new = broadcast to a peer, not stored. Peers' refusals never reach this tab.",
  "Observed ≠ current: a first page is always empty; later pages carry what peers send.",
  "Grant ≠ remaining: a grant is the allowance, not what is left.",
  "Statement bytes are public; encrypted contents stay sealed. Contact Chat text is plaintext.",
  "Source tags: light client, core, host, chain, reported (unchecked), derived, inspector.",
];

/** What this tab cannot see of peers' refusals. */
const PEER_REFUSALS =
  "Peers check allowance, account room and channel priority themselves and do not tell the light client. No refusal shown does not mean none happened.";

/** What a submission the inspector saw does not show. */
const POSTS_NOTE =
  "Submissions signed by this account that the core or the host asked for. Asking does not mean the light client got it.";

/** Why a submission refused here never reached the network. */
const REFUSED_HERE_NOTE =
  "This tab submits nothing: its no-submit guard answered these before the light client, so no peer got them.";

/** The light client's answers to an account's submissions, and its RPC errors. */
function answersText(account: AccountRecord): string {
  const parts = Object.entries(account.answers).map(
    ([status, n]) => `${n} ${status === "new" ? "broadcast" : status}`,
  );
  if (account.rpcErrors > 0)
    parts.push(
      `${account.rpcErrors} RPC error${account.rpcErrors === 1 ? "" : "s"}`,
    );
  return parts.length === 0 ? "none answered" : parts.join(" · ");
}

function phaseText(record: SubscriptionRecord): string {
  if (record.state === "refused") return `refused: ${record.error ?? ""}`;
  if (record.state === "ended") return `ended: ${record.endedHow}`;
  switch (record.phase) {
    case "waiting":
      return record.state === "opening" ? "opening" : "open, no page yet";
    case "initial":
      return `in its initial batch: at least ${record.lastRemaining} more to follow`;
    case "later":
      return "after its initial batch: peers' statements, new or long stored";
    case "unknown":
      return "open: no remaining count given";
  }
}

export function mountStatementsView(
  root: HTMLElement,
  deps: StatementsViewDeps,
): StatementsView {
  const { ledger } = deps;

  const top = el("div", "si-top");
  const title = el("span", "si-title", "Statements ");
  title.append(el("span", "accent", "Inspector"));
  const summary = el("span", "si-summary");
  const actions = el("div", "si-actions");
  const captureButton = button(
    "si-chip",
    "Capture data",
    "Keep statement data in memory in this tab, within a budget, to read it here. Never exported. Off again forgets it.",
  );
  const copyButton = button(
    "si-chip",
    "Copy evidence",
    "Copy what the ledger holds as JSON: metadata only, never statement data",
  );
  const clearButton = button(
    "si-chip",
    "Clear",
    "Forget everything recorded in this session",
  );
  actions.append(captureButton, copyButton, clearButton);
  top.append(title, summary, actions);

  const warning = el("p", "si-warning");
  warning.setAttribute("role", "note");
  // One takeaway; the distinctions every fact depends on open below it.
  const transport = el("details", "si-caveats");
  transport.setAttribute("role", "note");
  const caveats = el("summary");
  caveats.append(
    el("span", "si-caveat", TAKEAWAY),
    " · ",
    el("span", "si-about", "About this data"),
  );
  caveats.title = TRANSPORT_NOTE;
  const about = el("ul", "si-note si-about-list");
  about.append(...ABOUT.map((line) => el("li", undefined, line)));
  transport.append(caveats, about);
  const status = el("p", "si-status");
  status.setAttribute("role", "status");

  const tabs = el("div", "si-tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "Statements view");
  const tabButtons = new Map(
    TABS.map(({ value, label }) => {
      const tab = button("si-tab", label, `Show ${label.toLowerCase()}`);
      tab.setAttribute("role", "tab");
      tab.dataset.tab = value;
      tabs.append(tab);
      return [value, tab] as const;
    }),
  );
  const body = el("div", "si-body");
  root.append(top, warning, transport, tabs, status, body);

  let tab: Tab = "accounts";
  let visible = false;
  let dirty = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let selectedAccount: string | null = null;
  const openEvents = new Set<number>();
  const timeline = {
    kinds: new Set<EventKind>(),
    failedOnly: false,
    /** The search as typed. */
    text: "",
    focus: null as null | {
      label: string;
      matches(event: LedgerEvent): boolean;
    },
  };
  let grantsBusy = false;

  const say = (text: string) => {
    status.textContent = text;
  };

  // --- drawing --------------------------------------------------------------

  const draw = () => {
    timer = undefined;
    if (!visible) return;
    dirty = false;
    const counts = ledger.counts();
    const answered = Object.values(counts.answers).reduce(
      (sum: number, n) => sum + (n ?? 0),
      0,
    );
    const open = ledger
      .subscriptionList()
      .filter((s) => s.state === "open" || s.state === "opening").length;
    const refusedHere =
      counts.refusedLocally > 0
        ? ` · ${counts.refusedLocally} refused here`
        : "";
    summary.textContent = `since ${clock(ledger.sessionStart())} · ${counts.submits} submits · ${answered} answered · ${counts.rpcErrors} errors${refusedHere} · ${counts.delivered} delivered · ${open} subs open · ${counts.connections} conns`;
    summary.title =
      "Counters count since the session started in this tab. Lists below keep their newest entries within bounds.";
    const payload = ledger.payloadUse();
    captureButton.setAttribute("aria-pressed", String(ledger.capture));
    captureButton.textContent = ledger.capture
      ? `Capturing ${bytes(payload.bytes)}${payload.full ? " (full)" : ""}`
      : "Capture data";
    warning.hidden = !ledger.capture;
    warning.textContent = ledger.capture
      ? `Capturing data: ${bytes(payload.bytes)} of ${bytes(payload.limit)}${payload.full ? " (full)" : ""} · memory only · never exported`
      : "";
    warning.dataset.capturing = String(ledger.capture);
    for (const [value, tabButton] of tabButtons) {
      tabButton.setAttribute("aria-selected", String(value === tab));
      tabButton.tabIndex = value === tab ? 0 : -1;
    }
    const dropped = Object.entries(ledger.dropped()).filter(([, n]) => n > 0);
    // Controls are drawn again each time: the focused one gets its focus back.
    const active = document.activeElement as HTMLElement | null;
    const focusKey =
      active !== null && body.contains(active)
        ? active.dataset.focusKey
        : undefined;
    const keepScroll = body.scrollTop;
    const atEnd =
      body.scrollHeight - body.scrollTop - body.clientHeight < 8 &&
      tab === "timeline";
    const opened = openDisclosures(body);
    body.replaceChildren(
      ...(dropped.length === 0
        ? []
        : [
            el(
              "p",
              "si-note si-dropped",
              `Dropped to keep the limits: ${dropped.map(([name, n]) => `${n} ${name}`).join(", ")}.`,
            ),
          ]),
      ...(tab === "accounts"
        ? drawAccounts()
        : tab === "topics"
          ? drawTopics()
          : drawTimeline()),
    );
    reopenDisclosures(body, opened);
    body.scrollTop = atEnd ? body.scrollHeight : keepScroll;
    if (focusKey !== undefined) {
      const again = [
        ...body.querySelectorAll<HTMLElement>("[data-focus-key]"),
      ].find((node) => node.dataset.focusKey === focusKey);
      again?.focus();
      if (again instanceof HTMLInputElement && again.type === "search")
        again.setSelectionRange(again.value.length, again.value.length);
    }
  };

  const schedule = () => {
    dirty = true;
    if (!visible || timer !== undefined) return;
    timer = setTimeout(draw, DRAW_MS);
  };

  const now = () => {
    clearTimeout(timer);
    timer = undefined;
    dirty = true;
    draw();
  };

  function focusButton(
    label: string,
    matches: (event: LedgerEvent) => boolean,
  ): HTMLButtonElement {
    const focus = button(
      "si-chip",
      "Timeline",
      `Show only the timeline events of ${label}`,
    );
    focus.addEventListener("click", () => {
      timeline.focus = { label, matches };
      tab = "timeline";
      now();
    });
    return focus;
  }

  // --- accounts -----------------------------------------------------------------

  function drawAccounts(): HTMLElement[] {
    const accounts = ledger.accountList();
    const tools = el("div", "si-row-tools");
    const readAll = button(
      "si-chip",
      grantsBusy
        ? "Reading grants…"
        : `Read grants (${Math.min(accounts.length, 64)})`,
      "Read what the runtime grants each account here, at the newest finalized People block. Read only.",
    );
    readAll.disabled = grantsBusy || accounts.length === 0;
    readAll.addEventListener(
      "click",
      () =>
        void readGrantsFor(accounts.slice(0, 64).map((entry) => entry.account)),
    );
    const about = el("details", "si-columns-about");
    about.dataset.key = "accounts-columns";
    const aboutList = el("ul", "si-note si-about-list");
    aboutList.append(...COLUMNS_ABOUT.map((line) => el("li", undefined, line)));
    about.append(el("summary", undefined, "About these columns"), aboutList);
    tools.append(readAll, about);
    if (accounts.length === 0)
      return [tools, el("p", "si-empty", "No account seen yet.")];
    const table = el("div", "si-list si-accounts");
    table.setAttribute("role", "list");
    const header = el("div", "si-list-head");
    header.setAttribute("aria-hidden", "true");
    for (const name of COLUMNS) header.append(el("span", undefined, name));
    table.append(header);
    for (const account of accounts) {
      const row = button("si-list-row", "", `Show ${account.account}`);
      row.setAttribute(
        "aria-pressed",
        String(account.account === selectedAccount),
      );
      const [first, ...others] = account.roles;
      const role = el("span", "si-cell si-role");
      role.dataset.label = COLUMNS[1];
      role.append(first === undefined ? "Observed signer" : roleLabel(first));
      if (first !== undefined && roleSource(first) === "product")
        role.append(" ", tag("product"));
      if (others.length > 0)
        role.append(el("span", "si-more", ` +${others.length}`));
      const column = (index: number, text: string) => {
        const cell = el("span", "si-cell", text);
        cell.dataset.label = COLUMNS[index];
        return cell;
      };
      row.append(
        el("span", "si-mono", short(account.account)),
        role,
        column(2, allocationLabel(account)),
        column(3, grantLabel(account)),
        column(4, observedLabel(ledger, account)),
      );
      row.addEventListener("click", () => {
        selectedAccount =
          account.account === selectedAccount ? null : account.account;
        now();
      });
      table.append(row);
      if (account.account === selectedAccount)
        table.append(accountDetail(account));
    }
    return [tools, table];
  }

  function accountDetail(account: AccountRecord): HTMLElement {
    const detail = el("section", "si-detail");
    detail.dataset.key = `account:${account.account}`;
    detail.setAttribute("aria-label", `Account ${account.account}`);
    const facts = el("dl", "si-facts");
    facts.append(...fact("Account", account.account, "inspector"));
    for (const role of account.roles)
      facts.append(...fact("Role", roleText(role), roleSource(role)));
    for (const claim of account.claims)
      facts.append(
        ...fact(
          "Claim",
          `${clock(claim.at)} · ${claimLine(claim)}`,
          "core",
          claim.index === undefined
            ? undefined
            : `${claim.product ?? "The product"}'s own account at raw derivation index ${claim.index}, derived by the core. The slot is an allowance for one UTC day, not the person's registration.`,
        ),
      );
    const grant = grantText(account);
    facts.append(
      ...fact(
        "Grant",
        grant.text,
        account.grant === undefined ? "inspector" : "chain",
        grant.note,
      ),
    );
    facts.append(
      ...fact(
        "Observed",
        latestObservedText(ledger, account),
        "derived",
        LATEST_OBSERVED_NOTE,
      ),
    );
    facts.append(
      ...fact("Posts", `${account.submits} tried`, "inspector", POSTS_NOTE),
    );
    if (account.submits > account.refusedLocally)
      facts.append(...fact("Answers", answersText(account), "client"));
    if (account.refusedLocally > 0)
      facts.append(
        ...fact(
          "Refused here",
          `${account.refusedLocally} by the no-submit guard`,
          "host",
          REFUSED_HERE_NOTE,
        ),
      );
    facts.append(
      ...fact("Peer refusals", "not visible", "inspector", PEER_REFUSALS),
    );
    if (account.lastRefusal !== undefined)
      facts.append(
        ...fact("Last refusal", account.lastRefusal.answer.shape, "client"),
      );
    const read = button(
      "si-chip",
      "Read grant",
      "Read this account's grant now",
    );
    read.disabled = grantsBusy;
    read.addEventListener("click", () => void readGrantsFor([account.account]));
    const actionsRow = el("div", "si-row-tools");
    actionsRow.append(
      read,
      focusButton(`account ${short(account.account)}`, (event) =>
        (event.refs.accounts ?? []).includes(account.account),
      ),
    );
    detail.append(actionsRow, facts);
    const channels = ledger.channelList(account.account);
    if (channels.length > 0) {
      detail.append(el("h4", "si-sub", "Channels"));
      const list = el("ul", "si-plain");
      for (const channel of channels) {
        const latest = ledger.statement(channel.latest);
        const item = el(
          "li",
          undefined,
          `${short(channel.channel)} · ${latest === undefined ? "latest dropped from list" : `${latest.meta.dataBytes} B · seen ${clock(latest.lastAt)}`} · ${channel.replacements} replaced`,
        );
        item.dataset.key = `channel:${channel.channel}`;
        item.append(
          more(
            `Latest observed ${channel.latest}${latest === undefined ? "" : `, expires ${expiryText(latest.meta)}`}. ${channel.older} older seen later${channel.priorityRefusals > 0 ? `, ${channel.priorityRefusals} answered lower priority` : ""}. Not what peers hold now.`,
          ),
        );
        list.append(item);
      }
      detail.append(list);
    }
    const statements = ledger
      .statementList()
      .filter((statement) => statement.meta.signer === account.account)
      .slice(-20);
    if (statements.length > 0) {
      detail.append(el("h4", "si-sub", "Statements observed (newest 20)"));
      for (const statement of statements) {
        const box = el("details", "si-statement");
        box.dataset.key = statement.meta.hash;
        const head = el("summary");
        head.textContent = `${short(statement.meta.hash)} · ${statement.meta.dataBytes} B · ${statement.via.join(", ")} · observed ${statement.sightings}×, first ${clock(statement.firstAt)}, last ${clock(statement.lastAt)}`;
        box.append(head);
        const facts2 = el("dl", "si-facts");
        for (const [label, value] of metaDetails(statement.meta))
          facts2.append(...fact(label, value, "client"));
        if (statement.payload !== undefined) {
          facts2.append(
            ...fact(
              "Data",
              dataPreview(statement.payload),
              "inspector",
              statement.payloadCut
                ? " Cut to the capture limit."
                : " Captured in this tab.",
            ),
          );
        }
        box.append(facts2);
        detail.append(box);
      }
    }
    return detail;
  }

  async function readGrantsFor(accounts: string[]): Promise<void> {
    const access = deps.people();
    if (access === null) {
      say("The network is not up yet: no grant can be read.");
      return;
    }
    grantsBusy = true;
    const epoch = ledger.epoch();
    say(`Reading ${accounts.length} grant${accounts.length === 1 ? "" : "s"}…`);
    now();
    try {
      const read = await readGrants(access, accounts);
      if (ledger.grants(epoch, read.block, read.values))
        say(
          `Read ${accounts.length} grant${accounts.length === 1 ? "" : "s"} at block ${short(read.block)}.`,
        );
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      if (ledger.grantsFailed(epoch, accounts, reason))
        say(`Grant read failed: ${nodeText(reason)}`);
    } finally {
      grantsBusy = false;
      now();
    }
  }

  // --- topics -------------------------------------------------------------------

  function drawTopics(): HTMLElement[] {
    const noProbe = el("div", "si-note si-no-probe");
    noProbe.dataset.key = "no-probe";
    noProbe.append(
      "No probe: the inspector opens no statement subscription of its own. ",
      more(NO_PROBE_NOTE),
    );
    const out: HTMLElement[] = [noProbe];

    const subscriptions = ledger.subscriptionList();
    out.push(el("h4", "si-sub", `Subscriptions (${subscriptions.length})`));
    if (subscriptions.length === 0)
      out.push(el("p", "si-empty", "No subscription seen yet."));
    const subList = el("ul", "si-plain");
    for (const record of [...subscriptions].reverse()) {
      const item = el("li", "si-sub-row");
      item.dataset.key = `subscription:${record.id}`;
      item.dataset.state = record.state;
      item.append(
        el("span", "si-mono", `S${record.id}`),
        tag("client"),
        cell(
          `${record.source}${record.earlier ? " (earlier)" : ""} · ${record.state} · ${record.byDelivery.initial} initial · ${record.byDelivery.later} later · ${record.pages} pages`,
          `${filterText(record.filter)}. ${phaseText(record)}. ${record.statements} statements, ${record.byDelivery.unclassified} unclassified, ${record.duplicates} repeats of its last ${HASHES_PER_SUBSCRIPTION}.${record.earlier ? " Opened before this session; counts from its start." : ""} Open does not mean peers send to it.`,
        ),
      );
      subList.append(item);
    }
    out.push(subList);

    const topics = ledger.topicList();
    out.push(el("h4", "si-sub", `Topics (${topics.length})`));
    if (topics.length === 0)
      out.push(el("p", "si-empty", "No topic seen yet."));
    const topicList = el("ul", "si-plain");
    for (const topic of topics) {
      const item = el("li", "si-topic-row");
      item.dataset.key = `topic:${topic.topic}`;
      const open = subscriptions.filter(
        (entry) =>
          entry.state === "open" &&
          "topics" in entry.filter &&
          entry.filter.topics.includes(topic.topic),
      ).length;
      item.append(
        el("span", "si-mono", short(topic.topic)),
        cell(
          `${topic.statements} stmts · ${open} open${topic.labels.length === 0 ? "" : ` · ${topic.labels.join("; ")}`}`,
          "Statements recorded on it; one dropped from the list and seen again counts again. Open: subscriptions the core or host holds on it.",
        ),
      );
      if (topic.labels.length > 0) item.append(tag("product"));
      topicList.append(item);
    }
    out.push(topicList);
    return out;
  }

  // --- timeline -----------------------------------------------------------------

  function drawTimeline(): HTMLElement[] {
    const query = timeline.text.trim().toLowerCase();
    const tools = el("div", "si-row-tools si-timeline-tools");
    const search = el("input", "si-search");
    search.type = "search";
    search.placeholder = "Search";
    search.value = timeline.text;
    search.setAttribute("aria-label", "Search the timeline");
    search.dataset.focusKey = "search";
    search.addEventListener("input", () => {
      timeline.text = search.value;
      now();
    });
    tools.append(search);
    const all = button("si-chip", "All", "Show every kind");
    all.setAttribute("aria-pressed", String(timeline.kinds.size === 0));
    all.addEventListener("click", () => {
      timeline.kinds.clear();
      now();
    });
    tools.append(all);
    for (const { value, label } of KINDS) {
      const chip = button("si-chip", label, `Show ${label.toLowerCase()} only`);
      chip.setAttribute("aria-pressed", String(timeline.kinds.has(value)));
      chip.addEventListener("click", () => {
        if (timeline.kinds.has(value)) timeline.kinds.delete(value);
        else timeline.kinds.add(value);
        now();
      });
      tools.append(chip);
    }
    const errors = button(
      "si-chip si-errors",
      "Errors",
      "Show only what failed or was refused",
    );
    errors.setAttribute("aria-pressed", String(timeline.failedOnly));
    errors.addEventListener("click", () => {
      timeline.failedOnly = !timeline.failedOnly;
      now();
    });
    tools.append(errors);
    if (timeline.focus !== null) {
      const focus = button(
        "si-chip si-focus",
        `${timeline.focus.label} ×`,
        "Show every event again",
      );
      focus.setAttribute("aria-pressed", "true");
      focus.addEventListener("click", () => {
        timeline.focus = null;
        now();
      });
      tools.append(focus);
    }
    // Product reports belong to the Chat view: this timeline is transport and resources.
    const events = ledger
      .eventList()
      .filter((event) => event.kind !== "product");
    const shown = events.filter(
      (event) =>
        (timeline.kinds.size === 0 || timeline.kinds.has(event.kind)) &&
        (!timeline.failedOnly || event.failed === true) &&
        (timeline.focus === null || timeline.focus.matches(event)) &&
        (query === "" ||
          [event.text, ...event.details.flat()]
            .join("\n")
            .toLowerCase()
            .includes(query)),
    );
    const list = el("ol", "si-events");
    list.setAttribute("aria-label", "Timeline, oldest first");
    for (const event of shown) {
      const item = el("li", "si-event");
      item.dataset.kind = event.kind;
      if (event.failed === true) item.dataset.failed = "true";
      const head = el(
        event.details.length > 0 ? "button" : "div",
        "si-event-head",
      ) as HTMLElement;
      head.append(
        el("time", "si-time", clock(event.at)),
        tag(event.source),
        el("span", "si-event-text", event.text),
      );
      item.append(head);
      if (event.details.length > 0) {
        const opened = openEvents.has(event.id);
        (head as HTMLButtonElement).type = "button";
        head.dataset.focusKey = `event:${event.id}`;
        head.setAttribute("aria-expanded", String(opened));
        const details = el("dl", "si-facts si-event-details");
        details.hidden = !opened;
        for (const [label, value] of event.details)
          details.append(
            el("dt", undefined, label),
            el("dd", undefined, value),
          );
        item.append(details);
        head.addEventListener("click", () => {
          if (openEvents.has(event.id)) openEvents.delete(event.id);
          else openEvents.add(event.id);
          const isOpen = openEvents.has(event.id);
          head.setAttribute("aria-expanded", String(isOpen));
          details.hidden = !isOpen;
        });
      }
      list.append(item);
    }
    return [
      tools,
      el(
        "p",
        "si-note",
        `${shown.length} of ${events.length} events kept in this session.`,
      ),
      list,
      ...(shown.length === 0
        ? [
            el(
              "p",
              "si-empty",
              events.length === 0
                ? "Nothing recorded yet."
                : "No events match.",
            ),
          ]
        : []),
    ];
  }

  // --- wiring -------------------------------------------------------------------

  ledger.onChange(schedule);
  for (const [value, tabButton] of tabButtons)
    tabButton.addEventListener("click", () => {
      tab = value;
      now();
    });
  tabs.addEventListener("keydown", (event) => {
    const step =
      event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (step === 0) return;
    event.preventDefault();
    const at = TABS.findIndex((entry) => entry.value === tab);
    tab = TABS[(at + step + TABS.length) % TABS.length].value;
    now();
    tabButtons.get(tab)?.focus();
  });
  captureButton.addEventListener("click", () => {
    ledger.setCapture(!ledger.capture);
    now();
  });
  copyButton.addEventListener("click", () => {
    const text = evidenceText(ledger, {
      network: deps.network(),
      exportedAt: Date.now(),
    });
    void copyInPage(text).then((copied) => {
      copyButton.textContent = copied ? "Copied" : "Copy failed";
      setTimeout(() => (copyButton.textContent = "Copy evidence"), 1200);
    });
  });
  clearButton.addEventListener("click", () => {
    ledger.clear();
    openEvents.clear();
    selectedAccount = null;
    say("Cleared.");
    now();
  });

  return {
    show(next) {
      visible = next;
      root.hidden = !next;
      if (next && dirty) now();
    },
    reveal(target) {
      // A link shows all of its target: no earlier kind, error or search filter hides it.
      timeline.kinds.clear();
      timeline.failedOnly = false;
      timeline.text = "";
      timeline.focus = null;
      if ("account" in target) {
        tab = "accounts";
        selectedAccount = target.account.toLowerCase();
      } else if ("topic" in target) {
        const topic = target.topic.toLowerCase();
        tab = "timeline";
        timeline.focus = {
          label: `topic ${short(topic)}`,
          matches: (event) => (event.refs.topics ?? []).includes(topic),
        };
      } else {
        tab = "timeline";
        timeline.focus = {
          label: target.label,
          matches: (event) => event.refs.handle === target.handle,
        };
      }
      now();
    },
    sessionChanged() {
      ledger.setCapture(false);
      openEvents.clear();
      selectedAccount = null;
      timeline.focus = null;
      say("");
      schedule();
    },
  };
}

/** Captured data: as text when it reads as UTF-8 text, else as hex, cut for the page. */
function dataPreview(hex: string): string {
  const raw = new Uint8Array(
    (hex.slice(2).match(/../g) ?? []).map((pair) => parseInt(pair, 16)),
  );
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(raw);
    if (!/[\u0000-\u0008\u000e-\u001f]/.test(text))
      return text.length > 600 ? `${text.slice(0, 600)}…` : text;
  } catch {
    // Not text: shown as hex.
  }
  return hex.length > 600 ? `${hex.slice(0, 600)}…` : hex;
}
