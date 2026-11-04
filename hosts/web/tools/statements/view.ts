import "./statements-view.css";
import { copyInPage } from "../../src/app/copy.js";
import type { PeopleAccess } from "./actions.js";
import { readGrants } from "./actions.js";
import { evidenceText } from "./evidence.js";
import { nodeText } from "./decode.js";
import type { PeopleTransport } from "./transport.js";
import {
  HASHES_PER_SUBSCRIPTION,
  expiryText,
  filterText,
  metaDetails,
  ownership,
  short,
  type AccountRecord,
  type EventKind,
  type FactSource,
  type LedgerEvent,
  type ObservedStatement,
  type Ownership,
  type Role,
  type StatementLedger,
  type SubscriptionRecord,
} from "./ledger.js";
import { topicViews, type TopicView } from "./topics.js";
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
 * It draws the ledger (`ledger.ts`), with topics by role (`topics.ts`), and
 * runs the two reads on demand (`actions.ts`). It draws only while shown, at
 * most a few times a second, and keeps a row's open state and the selected
 * account or topic across draws.
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
  /** Open on `target`: an account or a topic selected, or the timeline narrowed to a request. */
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

/**
 * The roles that say what an account is used for, in the order the accounts
 * list prefers them. A product account claim and a chat allowance say how the
 * account was derived and what it may spend, so they show under the account's
 * details instead.
 */
const USES: readonly Role["kind"][] = [
  "conversation-account",
  "conversation-peer",
  "notification-account",
];

/** `account`'s roles that say what it is used for, in {@link USES} order. */
function usesOf(account: AccountRecord): Role[] {
  return USES.flatMap((kind) =>
    account.roles.filter((role) => role.kind === kind),
  );
}

/** A use in a few words, for the accounts list; the detail says it in full. */
function useLabel(role: Role): string {
  switch (role.kind) {
    case "conversation-account":
      return "Chat sender";
    case "conversation-peer":
      return "Chat peer";
    default:
      return "Setup notification";
  }
}

/** The Role column for an account no use is known of: what derived it, else only seen. */
function fallbackRole(account: AccountRecord): string {
  return account.roles.some(
    (role) => role.kind === "product-account" || role.kind === "chat-allowance",
  )
    ? "Product account"
    : "Observed signer";
}

/** The accounts list's columns. */
const COLUMNS = ["Account", "Role", "Owner", "Allocation", "Grant", "Observed"];

/** What each column of the accounts list says, and what it does not. */
const COLUMNS_ABOUT = [
  "Role: what it is used for. Chat sender and Chat peer are what a product reported, unchecked. Product account: derived for a product, use unknown.",
  "Owner: This wallet = the core or this host derived it from the signed-in wallet, this session. Unknown here may still be this wallet's, from another origin, browser or session. Both start again with each session and on Clear.",
  "Allocation: the latest daily slot claim this tab saw.",
  "Grant: statements · size the chain grants, read at a block. Not what is left.",
  "Observed: statements · size · last seen in this tab. Not what the network holds now.",
];

const OWNER_LABEL: Record<Ownership["kind"], string> = {
  "this-wallet": "This wallet",
  reported: "Reported only",
  unknown: "Unknown here",
};

/** Where an account's {@link Ownership} comes from, and what it does not say. */
function ownerFact(owner: Ownership): HTMLElement[] {
  switch (owner.kind) {
    case "this-wallet":
      return fact(
        "Owner",
        OWNER_LABEL[owner.kind],
        owner.evidence.some((role) => role.kind === "product-account")
          ? "core"
          : "host",
        `Derived from the signed-in wallet in this session: ${owner.evidence.map(roleText).join("; ")}. A record of this tab, not a signature check.`,
      );
    case "reported":
      return fact(
        "Owner",
        OWNER_LABEL[owner.kind],
        "product",
        "Only a product named it. The host derived it nowhere in this session, so this says nothing of whose it is.",
      );
    case "unknown":
      return fact(
        "Owner",
        OWNER_LABEL[owner.kind],
        "inspector",
        "Only seen signing or claimed for, or its record was dropped. It may still be this wallet's: another origin, browser or an earlier session derives accounts from indexes and post ids this tab does not hold.",
      );
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
  if (claim === undefined)
    return account.roles.some((role) => role.kind === "chat-allowance")
      ? "Chat allowance"
      : "None seen";
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

/** The topics list's columns. */
const TOPIC_COLUMNS = ["Topic", "Role", "Subscription", "Observed"];

/** What each column of the topics list says, and what it does not. */
function topicColumnsAbout(transport: PeopleTransport): string[] {
  return [
    "Topic: a name only where the host knows it, or a product reported it. Else hex.",
    transport.roleColumn,
    `Subscription: who holds one open on it. ${transport.openSubscription}`,
    "Observed: statements · last seen in this tab. Not what the network holds now.",
  ];
}

/** The one thing to know before reading any fact here. */
const TAKEAWAY = "Observed traffic, not network state";

/** What every fact depends on, one short line each; the full transport note is in the evidence. */
function aboutLines(transport: PeopleTransport): string[] {
  return [
    ...transport.about,
    "Grant ≠ remaining: a grant is the allowance, not what is left.",
    "Statement bytes are public; encrypted contents stay sealed. Contact Chat text is plaintext.",
    `Source tags: ${transport.source === "node" ? "People node" : "light client"}, core, host, chain, reported (unchecked), derived, inspector.`,
  ];
}

/** The People connection's answers to an account's submissions, and its RPC errors. */
function answersText(
  account: AccountRecord,
  transport: PeopleTransport,
): string {
  const parts = Object.entries(account.answers).map(
    ([status, n]) => `${n} ${status === "new" ? transport.wentOut : status}`,
  );
  if (account.rpcErrors > 0)
    parts.push(
      `${account.rpcErrors} RPC error${account.rpcErrors === 1 ? "" : "s"}`,
    );
  return parts.length === 0 ? "none answered" : parts.join(" · ");
}

function phaseText(
  record: SubscriptionRecord,
  transport: PeopleTransport,
): string {
  if (record.state === "refused") return `refused: ${record.error ?? ""}`;
  if (record.state === "ended") return `ended: ${record.endedHow}`;
  switch (record.phase) {
    case "waiting":
      return record.state === "opening" ? "opening" : "open, no page yet";
    case "initial":
      return `in its initial batch: at least ${record.lastRemaining} more to follow`;
    case "later":
      return `after its initial batch: ${transport.laterPages.replace("after the initial batch: ", "")}`;
    case "unknown":
      return "open: no remaining count given";
  }
}

/** Who holds a subscription open on a topic, in a few words. */
function subscriptionLabel(view: TopicView): string {
  if (view.open.length === 0)
    return view.subscriptions.length === 0 ? "None" : "None open";
  const sources = [...new Set(view.open.map((entry) => entry.source))].join(
    ", ",
  );
  return view.open.some((entry) => entry.state === "open")
    ? `${sources} · open`
    : `${sources} · opening`;
}

export function mountStatementsView(
  root: HTMLElement,
  deps: StatementsViewDeps,
): StatementsView {
  const { ledger } = deps;
  const { transport } = ledger;

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
  const caveatsBox = el("details", "si-caveats");
  caveatsBox.setAttribute("role", "note");
  const caveats = el("summary");
  caveats.append(
    el("span", "si-caveat", TAKEAWAY),
    " · ",
    el("span", "si-about", "About this data"),
  );
  caveats.title = transport.note;
  const aboutData = el("ul", "si-note si-about-list");
  aboutData.append(
    ...aboutLines(transport).map((line) => el("li", undefined, line)),
  );
  caveatsBox.append(caveats, aboutData);
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
  root.append(top, warning, caveatsBox, tabs, status, body);

  let tab: Tab = "accounts";
  let visible = false;
  let dirty = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let selectedAccount: string | null = null;
  let selectedTopic: string | null = null;
  /** A link opened the view: the selected row takes the focus after the draw. */
  let revealed = false;
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
    if (revealed) {
      revealed = false;
      const target =
        body.querySelector<HTMLElement>('.si-list-row[aria-pressed="true"]') ??
        body.querySelector<HTMLElement>(".si-detail");
      target?.focus({ preventScroll: true });
      target?.scrollIntoView({ block: "nearest" });
    } else if (focusKey !== undefined) {
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
      clearTimelineFilters();
      timeline.focus = { label, matches };
      tab = "timeline";
      now();
    });
    return focus;
  }

  /** A link shows all of its target: no earlier kind, error or search filter hides it. */
  function clearTimelineFilters(): void {
    timeline.kinds.clear();
    timeline.failedOnly = false;
    timeline.text = "";
    timeline.focus = null;
  }

  /** Select `account` on the Accounts tab. */
  function openAccount(account: string): void {
    tab = "accounts";
    selectedAccount = account.toLowerCase();
    revealed = true;
    now();
  }

  /** Select `topic` on the Topics tab. */
  function openTopic(topic: string): void {
    tab = "topics";
    selectedTopic = topic.toLowerCase();
    revealed = true;
    now();
  }

  /** An account as a link to its row on the Accounts tab. */
  function accountLink(account: string): HTMLButtonElement {
    const link = button("si-link", short(account), `Show account ${account}`);
    link.addEventListener("click", () => openAccount(account));
    return link;
  }

  /** The newest observed statements, each opening to its metadata and captured data. */
  function statementBoxes(
    statements: ObservedStatement[],
    withSigner: boolean,
  ): HTMLElement[] {
    return statements.map((statement) => {
      const box = el("details", "si-statement");
      box.dataset.key = statement.meta.hash;
      const head = el("summary");
      const signer = statement.meta.signer;
      const owner =
        withSigner && signer !== undefined
          ? ownership(ledger.accountRecord(signer))
          : null;
      head.textContent = `${short(statement.meta.hash)} · ${statement.meta.dataBytes} B${owner === null ? "" : ` · signer ${OWNER_LABEL[owner.kind].toLowerCase()}`} · ${statement.via.map((via) => (via === "broadcast" ? transport.wentOut : via)).join(", ")} · observed ${statement.sightings}×, first ${clock(statement.firstAt)}, last ${clock(statement.lastAt)}`;
      box.append(head);
      const facts = el("dl", "si-facts");
      if (owner !== null && signer !== undefined)
        facts.append(
          ...fact("Signer", accountLink(signer), transport.source),
          ...ownerFact(owner),
        );
      for (const [label, value] of metaDetails(statement.meta))
        facts.append(...fact(label, value, transport.source));
      if (statement.payload !== undefined) {
        facts.append(
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
      box.append(facts);
      return box;
    });
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
    const missing =
      selectedAccount !== null &&
      !accounts.some((account) => account.account === selectedAccount)
        ? [missingAccount(selectedAccount)]
        : [];
    if (accounts.length === 0)
      return [tools, ...missing, el("p", "si-empty", "No account seen yet.")];
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
      const [first, ...others] = usesOf(account);
      const role = el("span", "si-cell si-role");
      role.dataset.label = COLUMNS[1];
      role.append(
        first === undefined ? fallbackRole(account) : useLabel(first),
      );
      if (first !== undefined && roleSource(first) === "product")
        role.append(" ", tag("product"));
      if (others.length > 0)
        role.append(el("span", "si-more", ` +${others.length}`));
      const column = (index: number, text: string) => {
        const cell = el("span", "si-cell", text);
        cell.dataset.label = COLUMNS[index];
        return cell;
      };
      const owner = ownership(account);
      const ownerCell = column(2, OWNER_LABEL[owner.kind]);
      ownerCell.dataset.owner = owner.kind;
      row.append(
        el("span", "si-mono", short(account.account)),
        role,
        ownerCell,
        column(3, allocationLabel(account)),
        column(4, grantLabel(account)),
        column(5, observedLabel(ledger, account)),
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
    return [tools, ...missing, table];
  }

  /** A linked account this session's ledger holds nothing of: said so, and nothing is read for it. */
  function missingAccount(account: string): HTMLElement {
    const detail = el("section", "si-detail");
    detail.tabIndex = -1;
    detail.dataset.key = `account:${account}`;
    detail.setAttribute("aria-label", `Account ${account}`);
    const facts = el("dl", "si-facts");
    facts.append(
      ...fact("Account", account, "inspector"),
      ...ownerFact({ kind: "unknown" }),
      ...fact(
        "Observed",
        "nothing in this session",
        "inspector",
        "This tab recorded no statement, claim, grant or role of it, or dropped them to keep its limits. A product may name an account it never used here.",
      ),
    );
    detail.append(facts);
    return detail;
  }

  function accountDetail(account: AccountRecord): HTMLElement {
    const detail = el("section", "si-detail");
    detail.dataset.key = `account:${account.account}`;
    detail.setAttribute("aria-label", `Account ${account.account}`);
    const facts = el("dl", "si-facts");
    facts.append(...fact("Account", account.account, "inspector"));
    facts.append(...ownerFact(ownership(account)));
    for (const role of usesOf(account))
      facts.append(...fact("Role", roleText(role), roleSource(role)));
    for (const role of account.roles)
      if (role.kind === "product-account")
        facts.append(...fact("Derived as", roleText(role), "core"));
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
    for (const role of account.roles)
      if (role.kind === "chat-allowance")
        facts.append(
          ...fact(
            "Allowance",
            `${role.product} · ${roleText(role)}`,
            "host",
            "The host let this account post for the chat that request set up. It says nothing of what was sent.",
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
        transport.latestObservedNote,
      ),
    );
    facts.append(
      ...fact(
        "Posts",
        `${account.submits} tried`,
        "inspector",
        transport.postsNote,
      ),
    );
    if (account.submits > account.refusedLocally)
      facts.append(
        ...fact("Answers", answersText(account, transport), transport.source),
      );
    if (account.refusedLocally > 0)
      facts.append(
        ...fact(
          "Refused here",
          `${account.refusedLocally} by the no-submit guard`,
          "host",
          transport.refusedHereNote,
        ),
      );
    facts.append(
      ...fact(
        transport.refusalsLabel,
        "not visible",
        "inspector",
        transport.peerRefusals,
      ),
    );
    if (account.lastRefusal !== undefined)
      facts.append(
        ...fact(
          "Last refusal",
          account.lastRefusal.answer.shape,
          transport.source,
        ),
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
    if (statements.length > 0)
      detail.append(
        el("h4", "si-sub", "Statements observed (newest 20)"),
        ...statementBoxes(statements, false),
      );
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
    const intro = el("div", "si-note si-no-probe");
    intro.dataset.key = "no-probe";
    intro.append(
      "A topic is a label statements carry; a subscription listens on one or more topics. The inspector opens none of its own.",
      more(transport.noProbeNote),
    );
    const about = el("details", "si-columns-about");
    about.dataset.key = "topics-columns";
    const aboutList = el("ul", "si-note si-about-list");
    aboutList.append(
      ...topicColumnsAbout(transport).map((line) => el("li", undefined, line)),
    );
    about.append(el("summary", undefined, "About these columns"), aboutList);
    const tools = el("div", "si-row-tools");
    tools.append(about);
    const out: HTMLElement[] = [intro, tools];

    const views = topicViews(ledger, deps.network(), Date.now());
    if (
      selectedTopic !== null &&
      !views.some((view) => view.record.topic === selectedTopic)
    )
      out.push(missingTopic(selectedTopic));
    if (views.length === 0) out.push(el("p", "si-empty", "No topic seen yet."));
    else {
      const table = el("div", "si-list si-topics");
      table.setAttribute("role", "list");
      const header = el("div", "si-list-head");
      header.setAttribute("aria-hidden", "true");
      for (const name of TOPIC_COLUMNS)
        header.append(el("span", undefined, name));
      table.append(header);
      for (const view of views) {
        table.append(topicRow(view));
        if (view.record.topic === selectedTopic)
          table.append(topicDetail(view));
      }
      out.push(table);
    }

    const subscriptions = ledger.subscriptionList();
    if (subscriptions.length > 0) {
      const all = el("details", "si-columns-about si-subscriptions");
      all.dataset.key = "subscriptions";
      const list = el("ul", "si-plain");
      list.append(...[...subscriptions].reverse().map(subscriptionRow));
      all.append(
        el("summary", undefined, `All subscriptions (${subscriptions.length})`),
        list,
      );
      out.push(all);
    }
    return out;
  }

  function topicRow(view: TopicView): HTMLButtonElement {
    const { record } = view;
    const row = button(
      "si-list-row",
      "",
      `Show topic ${view.name?.text ?? record.topic}`,
    );
    row.setAttribute("aria-pressed", String(record.topic === selectedTopic));
    const name = el("span", "si-topic-name");
    if (view.name === null)
      name.append(el("span", "si-mono", short(record.topic)));
    else {
      name.append(
        view.name.text,
        " ",
        el("span", "si-hex", short(record.topic, 4)),
      );
      if (view.name.source === "product") name.append(" ", tag("product"));
    }
    const role = el("span", "si-cell si-role");
    role.dataset.label = TOPIC_COLUMNS[1];
    role.append(view.role);
    if (view.reportedOnly.listening || view.reportedOnly.publishing)
      role.append(" ", tag("product"));
    const column = (index: number, text: string) => {
      const cell = el("span", "si-cell", text);
      cell.dataset.label = TOPIC_COLUMNS[index];
      return cell;
    };
    row.append(
      name,
      role,
      column(2, subscriptionLabel(view)),
      column(
        3,
        record.lastAt === undefined
          ? "None seen"
          : `${record.statements} · ${clock(record.lastAt).slice(0, 5)}`,
      ),
    );
    row.addEventListener("click", () => {
      selectedTopic = record.topic === selectedTopic ? null : record.topic;
      now();
    });
    return row;
  }

  function topicDetail(view: TopicView): HTMLElement {
    const { record } = view;
    const detail = el("section", "si-detail");
    detail.tabIndex = -1;
    detail.dataset.key = `topic:${record.topic}`;
    detail.setAttribute("aria-label", `Topic ${record.topic}`);
    const copy = button("si-chip", "Copy topic", "Copy the full topic");
    copy.addEventListener("click", () => {
      void copyInPage(record.topic).then((copied) => {
        copy.textContent = copied ? "Copied" : "Copy failed";
        setTimeout(() => (copy.textContent = "Copy topic"), 1200);
      });
    });
    const actionsRow = el("div", "si-row-tools");
    actionsRow.append(
      copy,
      focusButton(`topic ${view.name?.text ?? short(record.topic)}`, (event) =>
        (event.refs.topics ?? []).includes(record.topic),
      ),
    );
    const facts = el("dl", "si-facts");
    facts.append(...fact("Topic", record.topic, "inspector"));
    if (view.name !== null)
      facts.append(
        ...fact("Name", view.name.text, view.name.source, view.name.detail),
      );
    for (const subscription of view.open)
      facts.append(
        ...fact(
          "Listening",
          `S${subscription.id} · ${subscription.source} · ${phaseText(subscription, transport)}`,
          transport.source,
          `${filterText(subscription.filter)}. ${transport.openSubscription}`,
        ),
      );
    if (view.open.length === 0)
      facts.append(...fact("Listening", "no open subscription", "inspector"));
    for (const report of view.reports)
      facts.append(
        ...fact(
          report.direction === "listen" ? "Listening" : "Publishing",
          `${report.product} says it ${report.direction === "listen" ? "reads it" : "posts here"}${report.stale ? " · worker stopped" : ""}`,
          "product",
          `Conversation ${report.conversation}, as the product reports it.`,
        ),
      );
    facts.append(
      ...fact(
        "Publishing",
        record.submits === 0
          ? "no submission asked here"
          : `${record.wentOut} ${transport.wentOut} · ${record.submits} tried`,
        record.submits === 0 ? "inspector" : transport.source,
        record.submits === 0 ? undefined : transport.topicPostsNote,
      ),
      ...fact(
        "Observed",
        record.lastAt === undefined
          ? "none seen · not the same as none stored"
          : `${record.statements} stmt${record.statements === 1 ? "" : "s"} · last ${clock(record.lastAt)} · not current`,
        "derived",
        transport.topicObservedNote,
      ),
    );
    detail.append(actionsRow, facts);
    const statements = ledger
      .statementList()
      .filter((statement) => statement.meta.topics.includes(record.topic))
      .slice(-20);
    if (statements.length > 0)
      detail.append(
        el("h4", "si-sub", "Statements observed (newest 20)"),
        ...statementBoxes(statements, true),
      );
    if (view.subscriptions.length > 0) {
      const nested = el("details", "si-columns-about");
      const list = el("ul", "si-plain");
      list.append(...view.subscriptions.map(subscriptionRow));
      nested.append(
        el(
          "summary",
          undefined,
          `Subscriptions (${view.subscriptions.length})`,
        ),
        list,
      );
      detail.append(nested);
    }
    return detail;
  }

  /** A linked topic this session's ledger holds nothing of. */
  function missingTopic(topic: string): HTMLElement {
    const detail = el("section", "si-detail");
    detail.tabIndex = -1;
    detail.dataset.key = `topic:${topic}`;
    detail.setAttribute("aria-label", `Topic ${topic}`);
    const facts = el("dl", "si-facts");
    facts.append(
      ...fact("Topic", topic, "inspector"),
      ...fact(
        "Observed",
        "nothing in this session",
        "inspector",
        "This tab recorded no statement, subscription or report on it, or dropped them to keep its limits.",
      ),
    );
    detail.append(facts);
    return detail;
  }

  function subscriptionRow(record: SubscriptionRecord): HTMLElement {
    const item = el("li", "si-sub-row");
    item.dataset.key = `subscription:${record.id}`;
    item.dataset.state = record.state;
    item.append(
      el("span", "si-mono", `S${record.id}`),
      tag(transport.source),
      cell(
        `${record.source}${record.earlier ? " (earlier)" : ""} · ${record.state} · ${record.byDelivery.initial} initial · ${record.byDelivery.later} later · ${record.pages} pages`,
        `${filterText(record.filter)}. ${phaseText(record, transport)}. ${record.statements} statements, ${record.byDelivery.unclassified} unclassified, ${record.duplicates} repeats of its last ${HASHES_PER_SUBSCRIPTION}.${record.earlier ? " Opened before this session; counts from its start." : ""} ${transport.openSubscription}`,
      ),
    );
    return item;
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
    selectedTopic = null;
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
      clearTimelineFilters();
      if ("account" in target) openAccount(target.account);
      else if ("topic" in target) openTopic(target.topic);
      else {
        tab = "timeline";
        timeline.focus = {
          label: target.label,
          matches: (event) => event.refs.handle === target.handle,
        };
        now();
      }
    },
    sessionChanged() {
      ledger.setCapture(false);
      openEvents.clear();
      selectedAccount = null;
      selectedTopic = null;
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
