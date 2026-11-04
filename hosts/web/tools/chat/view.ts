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
 * and protocol details are what a product reported, and say so.
 *
 * Resource facts (claims, grants, submissions, channels, subscriptions)
 * belong to the Statements view, which knows no protocol; each account and
 * topic here links to it, and the request to its timeline. A link only
 * navigates. An account a product names is joined with the host's facts,
 * here a cost per message, only when a claim or an allowance of that product
 * ties them.
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

const CONVERSATION_ID_NOTE =
  "The product's own local id for this conversation. Not an account, a key or a topic.";

/** A conversation's title, from the host's request when the host holds it for the same product. */
function titleOf(request: RequestRecord | undefined): string {
  switch (request?.role) {
    case "sender":
      return "Chat you started";
    case "recipient":
      return "Chat you were invited to";
    default:
      return "Chat";
  }
}

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

  /** An account or a topic, shown short, as a link to its detail in Statements. */
  const hexLink = (hex: string, target: StatementsTarget, what: string) => {
    const node = button(
      "si-link",
      short(hex),
      `Show ${what} ${hex} in Statements`,
    );
    node.addEventListener("click", () => deps.showStatements(target));
    return node;
  };

  const links = (nodes: HTMLElement[]) => {
    const value = el("span");
    nodes.forEach((node, index) =>
      value.append(...(index === 0 ? [] : [" "]), node),
    );
    return value;
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
                : links([
                    el("span", undefined, "allowed"),
                    hexLink(
                      request.account,
                      { account: request.account },
                      "account",
                    ),
                    ...(request.until === undefined
                      ? []
                      : [
                          el(
                            "span",
                            undefined,
                            `· until ${clock(request.until)}`,
                          ),
                        ]),
                  ]),
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

  function topicRow(
    label: string,
    topics: string[],
    note: string,
  ): HTMLElement[] {
    if (topics.length === 0) return [];
    return fact(
      label,
      links(topics.map((topic) => hexLink(topic, { topic }, "topic"))),
      "product",
      note,
    );
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
    const request = joinedRequest(record);
    card.append(
      head(
        titleOf(request),
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
    if (request !== undefined) {
      facts.append(...requestFacts(request));
      facts.append(
        ...fact(
          "Delivery",
          link(
            "Request timeline",
            {
              handle: request.handle,
              label: `request ${short(request.handle, 4)}`,
            },
            `request ${short(request.handle, 4)}`,
          ),
          "host",
          "The Statements timeline narrowed to this request: its send or reply, its delivery's notification accounts and problems, and chatAccount. The chat account's own posts are under its account.",
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
        ...fact(
          "Account",
          hexLink(report.account, { account: report.account }, "account"),
          "product",
          ACCOUNT_NOTE,
        ),
      );
      facts.append(
        ...(ledger.owns(record.product, report.account)
          ? fact(
              "Tie",
              `tied to ${record.product}`,
              "host",
              `A claim or an allowance of ${record.product} names this account. The host ties accounts to a product, not to a conversation.`,
            )
          : fact(
              "Tie",
              "none · reported only",
              "inspector",
              `No claim or allowance of ${record.product} names this account, so the host joins none of its facts here. Its link shows what this tab saw of it, if anything.`,
            )),
      );
    }
    if ((report.peers ?? []).length > 0)
      facts.append(
        ...fact(
          "Peers",
          links(
            report.peers!.map((peer) =>
              hexLink(peer, { account: peer }, "account"),
            ),
          ),
          "product",
          "The accounts whose statements it says it reads. Reported, not who they belong to.",
        ),
      );
    facts.append(
      ...topicRow(
        "Listens on",
        report.listen ?? [],
        "Topics it says it reads. Open one in Statements to see who listens there and what was seen.",
      ),
    );
    facts.append(
      ...topicRow(
        "Publishes on",
        report.post?.topics ?? [],
        "Topics it says it posts on.",
      ),
    );
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
    const cost = ledger.cost(record);
    facts.append(
      ...(cost.known
        ? fact(
            "Per message",
            `${(cost.wentOut / cost.sent).toFixed(2)} stmts · ${Math.round(cost.wentOutDataBytes / cost.sent)} B (${cost.wentOut} ${ledger.transport.wentOut} · ${cost.attempts} tried · ${cost.sent} sent)`,
            "derived",
            `Since ${clock(cost.since)}, this worker run's first report seen here: ${ledger.transport.chatCost}, over the messages it reports sent in the same span.`,
          )
        : fact("Per message", `not known: ${cost.why}`, "inspector")),
    );
    card.append(facts);
    if ((report.facts ?? []).length > 0) {
      const protocol = el("dl", "si-facts");
      for (const entry of report.facts!)
        protocol.append(...fact(entry.label, entry.value, "product"));
      card.append(el("h4", "si-sub", "Protocol details"), protocol);
    }
    const details = el("details", "si-columns-about");
    const idFacts = el("dl", "si-facts");
    idFacts.append(
      ...fact("Conversation ID", report.id, "product", CONVERSATION_ID_NOTE),
    );
    details.append(el("summary", undefined, "Details"), idFacts);
    card.append(details);
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
