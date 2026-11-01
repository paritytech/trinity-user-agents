import "../statements/statements-view.css";
import type {
  ConversationRecord,
  RequestRecord,
  StatementLedger,
} from "../statements/ledger.js";
import { short } from "../statements/ledger.js";
import type { StatementsTarget } from "../statements/view.js";
import {
  button,
  clock,
  el,
  fact,
  more,
  openDisclosures,
  reopenDisclosures,
} from "../statements/ui.js";

/**
 * The inspector's Chat view: the conversations product workers report about
 * themselves (`src/products/diagnostics.ts`), and the host's requests that
 * set them up. The host reads no chat payload to fill it: protocol, counts
 * and window are what a product reported, and say so.
 *
 * Resource facts (claims, grants, submissions, channels, subscriptions)
 * belong to the Statements view, which knows no protocol; a conversation
 * links there for its account, topics and request. An account a product
 * names is joined with the host's facts, here a cost per message, only when
 * a claim or an allowance of that product ties them.
 */

export interface ChatViewDeps {
  ledger: StatementLedger;
  /** Open the Statements view on an account, a topic or a request. */
  showStatements(target: StatementsTarget): void;
  /** Show `productId`'s TrUAPI traces. */
  showTraces(productId: string): void;
  /** Show `name`'s worker in the Workers view. */
  inspectWorker(name: string): void;
}

export interface ChatView {
  show(visible: boolean): void;
}

/** How often the view draws at most while shown. */
const DRAW_MS = 250;

const PROTOCOL_NOTE =
  "The protocol name and version the product says it runs. The host does not read its payloads.";

const ACCOUNT_NOTE =
  "The account the product says it posts as: the product's own account at a raw derivation index, which the core holds the key of. Not the person's identity account.";

export function mountChatView(root: HTMLElement, deps: ChatViewDeps): ChatView {
  const { ledger } = deps;
  const top = el("div", "si-top");
  const title = el("span", "si-title", "Chat ");
  title.append(el("span", "accent", "Inspector"));
  const summary = el("span", "si-summary");
  top.append(title, summary);
  const note = el("div", "si-caveats");
  note.append(
    "Product-reported · the host reads no chat payload · resources are in Statements",
  );
  const body = el("div", "si-body");
  root.append(top, note, body);

  let visible = false;
  let dirty = true;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const link = (text: string, target: StatementsTarget, label: string) => {
    const chip = button("si-chip", text, `Show ${label} in Statements`);
    chip.addEventListener("click", () => deps.showStatements(target));
    return chip;
  };

  /** The host's request a report names, only when the host holds it for the same product. */
  const joinedRequest = (record: ConversationRecord) => {
    const handle = record.report.request;
    if (handle === undefined) return undefined;
    const request = ledger.requestRecord(handle);
    return request?.product === record.product ? request : undefined;
  };

  function requestFacts(request: RequestRecord): HTMLElement[] {
    return [
      ...fact(
        "Request",
        `${short(request.handle, 4)} · ${request.role} · ${request.state}`,
        "host",
        request.problem === undefined
          ? undefined
          : `Problem: ${request.problem}`,
      ),
      ...(request.index === undefined
        ? []
        : fact(
            "chatAccount",
            request.refusal !== undefined
              ? `refused: ${request.refusal}`
              : request.account === undefined
                ? `index ${short(request.index, 4)} bound`
                : `allowed ${short(request.account)}${request.until === undefined ? "" : ` · until ${clock(request.until)}`}`,
            "host",
            `Raw derivation index ${request.index}.${request.account === undefined ? "" : ` Allowed account ${request.account}.`}`,
          )),
    ];
  }

  function head(
    text: string,
    badges: string[],
    actions: HTMLButtonElement[],
  ): HTMLElement {
    const row = el("div", "si-card-head");
    row.append(el("h3", "si-card-title", text));
    for (const badge of badges) row.append(el("span", "si-badge", badge));
    const actionsRow = el("div", "si-card-actions");
    actionsRow.append(...actions);
    row.append(actionsRow);
    return row;
  }

  function topicRow(label: string, topics: string[]): HTMLElement[] {
    if (topics.length === 0) return [];
    const value = el("span");
    value.append(
      ...topics.flatMap((topic, index) => [
        ...(index === 0 ? [] : [" "]),
        link(short(topic), { topic }, `topic ${short(topic)}`),
      ]),
    );
    return fact(label, value, "product");
  }

  function conversationCard(record: ConversationRecord): HTMLElement {
    const { report } = record;
    const card = el("article", "si-card");
    card.dataset.key = `conversation:${record.key}`;
    if (record.stale) card.dataset.stale = "true";
    const worker = button(
      "si-chip",
      "Worker",
      `Show ${record.product} in Workers`,
    );
    worker.addEventListener("click", () => deps.inspectWorker(record.product));
    const traces = button(
      "si-chip",
      "TrUAPI",
      `Show ${record.product}'s frames`,
    );
    traces.addEventListener("click", () => deps.showTraces(record.product));
    card.append(
      head(
        `Conversation ${short(report.id, 4)}`,
        [
          record.product,
          ...(report.state === undefined ? [] : [report.state]),
          ...(record.stale ? ["stale: worker stopped"] : []),
        ],
        [worker, traces],
      ),
    );
    const facts = el("dl", "si-facts");
    facts.append(
      ...fact("Protocol", report.protocol, "product", PROTOCOL_NOTE),
    );
    const request = joinedRequest(record);
    if (request !== undefined) {
      facts.append(...requestFacts(request));
      facts.append(
        ...fact(
          "Delivery",
          link(
            "Statements",
            {
              handle: request.handle,
              label: `request ${short(request.handle, 4)}`,
            },
            `request ${short(request.handle, 4)}`,
          ),
          "host",
          "The host's request and delivery events for it, in the Statements timeline. Submissions and claims of its account are there by account.",
        ),
      );
    } else if (report.request !== undefined)
      facts.append(
        ...fact(
          "Request",
          `${short(report.request, 4)} · not held by host`,
          "product",
          `Not a request this host holds for ${record.product} in this session.`,
        ),
      );
    if (report.account !== undefined) {
      facts.append(
        ...fact("Account", short(report.account), "product", ACCOUNT_NOTE),
      );
      if (ledger.owns(record.product, report.account)) {
        const value = el("span");
        value.append(
          "tied by host ",
          link(
            "Statements",
            { account: report.account },
            `account ${short(report.account)}`,
          ),
        );
        facts.append(
          ...fact(
            "Tie",
            value,
            "host",
            `A claim or an allowance of ${record.product} names this account: its resources are in Statements.`,
          ),
        );
      } else
        facts.append(
          ...fact(
            "Tie",
            "none · reported only",
            "inspector",
            `No claim or allowance of ${record.product} names this account, so the host joins none of its facts here.`,
          ),
        );
    }
    if ((report.peers ?? []).length > 0)
      facts.append(
        ...fact(
          "Peers",
          `${report.peers!.length} account${report.peers!.length === 1 ? "" : "s"}`,
          "product",
          `The accounts whose statements it reads: ${report.peers!.join(", ")}.`,
        ),
      );
    facts.append(...topicRow("Read topics", report.listen ?? []));
    facts.append(...topicRow("Post topics", report.post?.topics ?? []));
    if (report.logical !== undefined)
      facts.append(
        ...fact(
          "Messages",
          Object.entries(report.logical)
            .map(([key, n]) => `${n} ${key}`)
            .join(" · "),
          "product",
          "Logical messages, as the product counts them since its worker started. Duplicates: messages read again and dropped.",
        ),
      );
    if (report.window !== undefined) {
      const window = report.window;
      facts.append(
        ...fact(
          "Window",
          `${window.messages}${window.maxMessages === undefined ? "" : `/${window.maxMessages}`} msgs · ${window.bytes} B${window.maxBytes === undefined ? "" : `/${window.maxBytes} B`}${window.retentionSeconds === undefined ? "" : ` · ${Math.round(window.retentionSeconds / 3600)} h`}`,
          "product",
          "What its last post carried, and the most a post carries, as the product reports it.",
        ),
      );
    }
    const cost = ledger.cost(record);
    facts.append(
      ...(cost.known
        ? fact(
            "Per message",
            `${(cost.wentOut / cost.sent).toFixed(2)} stmts · ${Math.round(cost.wentOutDataBytes / cost.sent)} B (${cost.wentOut} broadcast · ${cost.attempts} tried · ${cost.sent} sent)`,
            "derived",
            `Since ${clock(cost.since)}, this worker run's first report seen here: submissions answered new (broadcast, not shown stored) signed by its account, over the messages it reports sent in the same window.`,
          )
        : fact("Per message", `not known: ${cost.why}`, "inspector")),
    );
    for (const entry of report.facts ?? [])
      facts.append(...fact(entry.label, entry.value, "product"));
    card.append(facts);
    return card;
  }

  function requestCard(request: RequestRecord): HTMLElement {
    const card = el("article", "si-card");
    card.dataset.key = `request:${request.handle}`;
    card.append(
      head(
        `Request ${short(request.handle, 4)}`,
        [request.product],
        [
          link(
            "Statements",
            {
              handle: request.handle,
              label: `request ${short(request.handle, 4)}`,
            },
            `request ${short(request.handle, 4)}`,
          ),
        ],
      ),
    );
    const facts = el("dl", "si-facts");
    facts.append(...requestFacts(request));
    card.append(facts);
    return card;
  }

  const draw = () => {
    timer = undefined;
    if (!visible) return;
    dirty = false;
    const conversations = ledger.conversationList();
    const linked = new Set(
      conversations
        .map(joinedRequest)
        .filter((request) => request !== undefined),
    );
    const requests = ledger
      .requestList()
      .filter((request) => !linked.has(request));
    summary.textContent = `${conversations.length} conversation${conversations.length === 1 ? "" : "s"} · ${requests.length} other request${requests.length === 1 ? "" : "s"}`;
    const empty = el("div", "si-empty");
    empty.append(
      "No conversation reported. ",
      more(
        "A product worker that sends diagnostics reports (Contact Chat does) shows each of its conversations here. Requests the host's request service handles show too.",
      ),
    );
    empty.dataset.key = "empty";
    const opened = openDisclosures(body);
    body.replaceChildren(
      ...conversations.map(conversationCard),
      ...requests.map(requestCard),
      ...(conversations.length + requests.length === 0 ? [empty] : []),
    );
    reopenDisclosures(body, opened);
  };

  ledger.onChange(() => {
    dirty = true;
    if (visible && timer === undefined) timer = setTimeout(draw, DRAW_MS);
  });

  return {
    show(next) {
      visible = next;
      root.hidden = !next;
      if (next && dirty) {
        clearTimeout(timer);
        draw();
      }
    },
  };
}
