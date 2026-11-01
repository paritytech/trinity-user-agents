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

function roleSource(role: Role): FactSource {
  return role.kind === "conversation-account" ||
    role.kind === "conversation-peer"
    ? "product"
    : role.kind === "product-account"
      ? "core"
      : "host";
}

/**
 * An account's latest claim: an allowance slot for one UTC day, which the
 * core claimed with the person's membership. It is not the person's own
 * registration.
 */
function claimText(account: AccountRecord): string | null {
  const claim = account.claims.at(-1);
  return claim === undefined ? null : claimLine(claim);
}

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
        ? "none stored"
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
    return `none seen${extra} · not none stored`;
  return `${observed.statements} stmt${observed.statements === 1 ? "" : "s"} · ${bytes(observed.dataBytes)} · seen ${clock(observed.lastObservedAt)}${extra} · not current`;
}

/** What the latest observed statements do not say. */
const LATEST_OBSERVED_NOTE =
  'Latest observed per channel, not known expired, this tab\'s own broadcasts included. Not what the network holds now: peers may have refused or evicted them without saying so, or hold others no one sent here. "Dropped" counts channels whose latest statement left the list.';

/** Why the view offers no probe, and what an observed subscription does not show. */
const NO_PROBE_NOTE =
  "In this host's light client every People connection shares one topic filter that is sent to peers, and each connection's subscriptions replace it, so a subscription made to look would take the chat's topics off it. An open subscription below is one the core or the host made; it does not show that peers send to it.";

/** The distinctions every fact depends on, as short as they stay true. */
const CAVEATS = [
  "light client",
  "new = broadcast, not stored",
  "peer refusals unseen",
  "observed ≠ current",
  "grant ≠ remaining",
  "data is public",
];

const PUBLIC_NOTE =
  "Statements are public: anyone who reads the Statement Store reads their topics, accounts, sizes and data.";

const SOURCES_NOTE =
  "Each fact is tagged with its source: light client (its answer, or a page of what peers sent), core (a claim result), host (this host's records), chain (a read at a block), reported (what a product said, unchecked), derived (worked out here from what this tab saw and when) or inspector.";

/** What this tab cannot see of peers' refusals. */
const PEER_REFUSALS =
  "Peers check allowance, account room and channel priority themselves and do not tell the light client. No refusal shown does not mean none happened.";

function answersText(answers: Partial<Record<string, number>>): string {
  const parts = Object.entries(answers).map(
    ([status, n]) => `${n} ${status === "new" ? "broadcast" : status}`,
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
  // The distinctions every fact below depends on, short; the full text opens.
  const transport = el("details", "si-caveats");
  transport.setAttribute("role", "note");
  const caveats = el("summary");
  caveats.append(
    ...CAVEATS.flatMap((caveat, index) => [
      ...(index === 0 ? [] : [" · "]),
      el("span", "si-caveat", caveat),
    ]),
  );
  transport.append(
    caveats,
    el("p", "si-note si-transport", TRANSPORT_NOTE),
    el("p", "si-note", PUBLIC_NOTE),
    el("p", "si-note", SOURCES_NOTE),
  );
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
    summary.textContent = `since ${clock(ledger.sessionStart())} · ${counts.submits} submits · ${answered} answered · ${counts.rpcErrors + counts.refusedLocally} errors · ${counts.delivered} delivered · ${open} subs open · ${counts.connections} conns`;
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
    tools.append(
      readAll,
      el("span", "si-note", "Claimed, reported or seen signing."),
    );
    if (accounts.length === 0)
      return [tools, el("p", "si-empty", "No account seen yet.")];
    const table = el("div", "si-list");
    table.setAttribute("role", "list");
    for (const account of accounts) {
      const row = button("si-list-row", "", `Show ${account.account}`);
      row.setAttribute(
        "aria-pressed",
        String(account.account === selectedAccount),
      );
      const roles = account.roles.map(roleText);
      row.append(
        el("span", "si-mono", short(account.account)),
        el("span", "si-cell", roles.length === 0 ? "signer seen" : roles[0]),
        el("span", "si-cell", claimText(account) ?? "no claim seen"),
        el("span", "si-cell", grantText(account).text),
        el("span", "si-cell", latestObservedText(ledger, account)),
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
      ...fact(
        "Posts",
        `${account.submits} submitted: ${answersText(account.answers)}`,
        "client",
      ),
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
