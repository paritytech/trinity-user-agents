/**
 * DRAFT, NOT TRUAPI. What a product worker may tell its host about the
 * conversations it runs, for the inspector's Statements view: which protocol
 * each one runs, the accounts and topics it uses, and counts only the
 * product can know, such as messages it sent, received and dropped as
 * copies. A product reports what it chooses; the host shows it as reported
 * by the product, beside what the host saw itself, and never acts on it.
 *
 * It travels as a window message from the worker's frame, heard like a
 * console line (`worker-console.ts`): the host attributes it to the worker
 * whose frame posted it, never to a name the report carries. Each report
 * replaces the last one of that worker run.
 *
 * A report carries no payload: the host does not read a protocol's bytes to
 * fill it, and a product should put in it nothing it would not show anyone
 * who opens the developer tools. Keys, secrets and what people wrote do not
 * belong in it.
 */

/** The `type` of the window message. */
export const PRODUCT_DIAGNOSTICS = "truapi-product-diagnostics";
/** The version of the report. */
export const DIAGNOSTICS_PROTOCOL = "product-diagnostics/draft-1";

/** Counts of logical messages, as the product counts them since its worker started. */
export interface LogicalCounts {
  /** This side's messages a post carried that the store accepted. */
  sent?: number;
  /** The other side's messages kept for the first time. */
  received?: number;
  /** Messages read again that the product already had, and dropped. */
  duplicates?: number;
  /** This side's messages that failed to post. */
  failed?: number;
  /** Posts made: statements submitted. */
  posts?: number;
}

/**
 * One labelled fact the product wants shown as it gives it: what only its
 * protocol defines, such as how many messages one post carries. The host
 * shows it as text and never computes it.
 */
export interface DiagnosticFact {
  label: string;
  value: string;
}

/** One conversation as the product reports it. */
export interface ConversationDiagnostics {
  /** The product's own id for it. */
  id: string;
  /** The protocol it runs, such as `contact-chat/simple-text/1`. */
  protocol: string;
  /** The product's own word for where it stands. */
  state?: string;
  /** The host request handle that set it up, `0x` hex, when the host request service did. */
  request?: string;
  /** The account this side posts as, `0x` hex. */
  account?: string;
  /** Accounts whose statements this side reads, `0x` hex. */
  peers?: string[];
  /** Topics this side reads, `0x` hex. */
  listen?: string[];
  /** Where this side posts. */
  post?: { topics: string[]; channel?: string };
  logical?: LogicalCounts;
  /** Protocol details, in the product's words. */
  facts?: DiagnosticFact[];
}

export interface DiagnosticsReport {
  conversations: ConversationDiagnostics[];
}

/** The message a worker frame posts. */
export interface DiagnosticsMessage {
  type: typeof PRODUCT_DIAGNOSTICS;
  protocol: typeof DIAGNOSTICS_PROTOCOL;
  report: DiagnosticsReport;
}

/** Bounds on what the host keeps of one report. Anything past them is dropped. */
export const DIAGNOSTICS_LIMITS = {
  conversations: 32,
  text: 80,
  accounts: 8,
  topics: 16,
  facts: 12,
} as const;

const HEX32 = /^0x[0-9a-f]{64}$/;
const HEX16 = /^0x[0-9a-f]{32}$/;

function text(value: unknown, max: number = DIAGNOSTICS_LIMITS.text) {
  return typeof value === "string" && value.trim() !== ""
    ? value.slice(0, max)
    : undefined;
}

const lower = (value: unknown): string | undefined =>
  typeof value === "string" ? value.toLowerCase() : undefined;

function hexList(value: unknown, max: number): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const list = value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.toLowerCase())
    .filter((item) => HEX32.test(item))
    .slice(0, max);
  return list;
}

function count(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function counts<K extends string>(
  value: unknown,
  keys: readonly K[],
): Partial<Record<K, number>> | undefined {
  if (typeof value !== "object" || value === null) return undefined;
  const record = value as Record<string, unknown>;
  const out: Partial<Record<K, number>> = {};
  for (const key of keys) {
    const read = count(record[key]);
    if (read !== undefined) out[key] = read;
  }
  return Object.keys(out).length === 0 ? undefined : out;
}

function readConversation(value: unknown): ConversationDiagnostics | null {
  if (typeof value !== "object" || value === null) return null;
  const record = value as Record<string, unknown>;
  const id = text(record.id);
  const protocol = text(record.protocol);
  if (id === undefined || protocol === undefined) return null;
  const out: ConversationDiagnostics = { id, protocol };
  const state = text(record.state, 32);
  if (state !== undefined) out.state = state;
  const request = lower(record.request);
  if (request !== undefined && HEX16.test(request)) out.request = request;
  const account = lower(record.account);
  if (account !== undefined && HEX32.test(account)) out.account = account;
  const peers = hexList(record.peers, DIAGNOSTICS_LIMITS.accounts);
  if (peers !== undefined) out.peers = peers;
  const listen = hexList(record.listen, DIAGNOSTICS_LIMITS.topics);
  if (listen !== undefined) out.listen = listen;
  if (typeof record.post === "object" && record.post !== null) {
    const post = record.post as Record<string, unknown>;
    const topics = hexList(post.topics, 4) ?? [];
    const channel = lower(post.channel);
    out.post = {
      topics,
      ...(channel !== undefined && HEX32.test(channel) ? { channel } : {}),
    };
  }
  const logical = counts(record.logical, [
    "sent",
    "received",
    "duplicates",
    "failed",
    "posts",
  ] as const);
  if (logical !== undefined) out.logical = logical;
  if (Array.isArray(record.facts)) {
    const facts = record.facts
      .map((fact) => {
        const entry = fact as Record<string, unknown> | null;
        const label = text(entry?.label, 40);
        const value =
          typeof entry?.value === "number"
            ? String(entry.value)
            : text(entry?.value);
        return label === undefined || value === undefined
          ? null
          : { label, value };
      })
      .filter((fact): fact is DiagnosticFact => fact !== null)
      .slice(0, DIAGNOSTICS_LIMITS.facts);
    if (facts.length > 0) out.facts = facts;
  }
  return out;
}

/**
 * The report in a message from a worker frame, bounded and checked field by
 * field, or null when it is not one. A field that does not read is left out.
 */
export function readDiagnosticsMessage(
  data: unknown,
): DiagnosticsReport | null {
  const message = data as Partial<DiagnosticsMessage> | null;
  if (
    message?.type !== PRODUCT_DIAGNOSTICS ||
    message.protocol !== DIAGNOSTICS_PROTOCOL ||
    typeof message.report !== "object" ||
    message.report === null ||
    !Array.isArray(message.report.conversations)
  )
    return null;
  return {
    conversations: message.report.conversations
      .slice(0, DIAGNOSTICS_LIMITS.conversations)
      .map(readConversation)
      .filter((entry): entry is ConversationDiagnostics => entry !== null),
  };
}

/** A product's end: post `report` to the host page that runs this frame. */
export function postDiagnostics(
  report: DiagnosticsReport,
  parent: Window = window.parent,
  hostOrigin: string = location.origin,
): void {
  parent.postMessage(
    {
      type: PRODUCT_DIAGNOSTICS,
      protocol: DIAGNOSTICS_PROTOCOL,
      report,
    } satisfies DiagnosticsMessage,
    hostOrigin,
  );
}
