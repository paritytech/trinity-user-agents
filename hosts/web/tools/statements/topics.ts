import {
  inboxTopic,
  periodOf,
} from "../../src/invitations/statement-delivery.js";
import { toHex } from "../../bridges/primitives/bytes.js";
import {
  short,
  type StatementLedger,
  type SubscriptionRecord,
  type TopicRecord,
} from "./ledger.js";

/**
 * The Topics tab's model: each topic the ledger met, with what this tab knows
 * of who listens on it and who publishes on it, each from its own source.
 *
 * - Listening: a subscription the core or the host holds open on it, from
 *   the traffic; or a product's report that it reads it, unchecked.
 * - Publishing: a submission on it the light client answered `new` or
 *   `known` (broadcast to a peer, not shown stored); or a product's report
 *   that it posts there, unchecked.
 * - Tried: submissions on it were asked for, and none is known to have gone
 *   out: refused here, an RPC error, refused, or no answer seen.
 *
 * A name is given only where its meaning is known: the day inbox of the
 * host's draft delivery, worked out from the network and the UTC day as the
 * delivery does, or a topic a product's report names. Every other topic stays
 * hex. No topic is reversed and no payload is read.
 */

export type TopicRole =
  | "Both"
  | "Listening"
  | "Publishing"
  | "Tried"
  | "Other observed";

/** A product's report naming the topic. */
export interface TopicReport {
  product: string;
  conversation: string;
  direction: "listen" | "post";
  /** The worker stopped: the report no longer holds. */
  stale: boolean;
}

export interface TopicName {
  text: string;
  source: "derived" | "product";
  detail: string;
}

export interface TopicView {
  record: TopicRecord;
  name: TopicName | null;
  /** Subscriptions whose filter covers it, newest first. */
  subscriptions: SubscriptionRecord[];
  /** Of those, the ones open or opening. */
  open: SubscriptionRecord[];
  reports: TopicReport[];
  role: TopicRole;
  /** The role rests, for listening or publishing, only on a product's report. */
  reportedOnly: { listening: boolean; publishing: boolean };
}

const ROLE_ORDER: TopicRole[] = [
  "Both",
  "Listening",
  "Publishing",
  "Tried",
  "Other observed",
];

/** The day inbox topics the host's delivery uses, by topic, around `now`. */
const inboxCache = new Map<string, string>();

function inboxName(
  topic: string,
  network: string,
  now: number,
): TopicName | null {
  const today = periodOf(now);
  for (let period = today - 2; period <= today + 1; period += 1) {
    const key = `${network}:${period}`;
    let hex = inboxCache.get(key);
    if (hex === undefined) {
      hex = toHex(inboxTopic(network, period));
      if (inboxCache.size > 64) inboxCache.clear();
      inboxCache.set(key, hex);
    }
    if (hex !== topic) continue;
    const day =
      period === today
        ? "today UTC"
        : period === today - 1
          ? "yesterday UTC"
          : `UTC day ${period}`;
    return {
      text: `Shared setup inbox · ${day}`,
      source: "derived",
      detail: `The draft delivery's day inbox on ${network} for UTC day ${period}, worked out here from the network and the day. Every host using this draft delivery posts its sealed chat requests and replies there; this host reads today's and yesterday's.`,
    };
  }
  return null;
}

/** Whether `subscription`'s filter takes statements on `topic`. */
function covers(subscription: SubscriptionRecord, topic: string): boolean {
  const { filter } = subscription;
  return (
    filter.kind === "any" ||
    ("topics" in filter && filter.topics.includes(topic))
  );
}

/** What the conversations products report say of each topic. */
function reportsByTopic(ledger: StatementLedger): Map<string, TopicReport[]> {
  const out = new Map<string, TopicReport[]>();
  for (const record of ledger.conversationList()) {
    const { report } = record;
    const add = (topic: string, direction: TopicReport["direction"]) => {
      const list = out.get(topic) ?? [];
      list.push({
        product: record.product,
        conversation: report.id,
        direction,
        stale: record.stale,
      });
      out.set(topic, list);
    };
    for (const topic of report.listen ?? []) add(topic, "listen");
    for (const topic of report.post?.topics ?? []) add(topic, "post");
  }
  return out;
}

function reportName(
  reports: TopicReport[],
  conversations: Map<string, number>,
): TopicName | null {
  const live = reports.filter((report) => !report.stale);
  const [first] = live.length > 0 ? live : reports;
  if (first === undefined) return null;
  const several = (conversations.get(first.product) ?? 0) > 1;
  const direction = first.direction === "listen" ? "incoming" : "outgoing";
  return {
    text: `${first.product} · ${direction}${several ? ` · ${short(first.conversation, 2)}` : ""}${live.length === 0 ? " · worker stopped" : ""}`,
    source: "product",
    detail: `${first.product} reports that its conversation ${first.conversation} ${first.direction === "listen" ? "reads" : "posts on"} this topic${reports.length > 1 ? `, and ${reports.length - 1} more report${reports.length === 2 ? "" : "s"} name it` : ""}. The host did not check it.`,
  };
}

/** Every topic the ledger holds, as the Topics tab lists them: active roles first, then the latest seen. */
export function topicViews(
  ledger: StatementLedger,
  network: string,
  now: number,
): TopicView[] {
  const subscriptions = ledger.subscriptionList().reverse();
  const reports = reportsByTopic(ledger);
  const conversations = new Map<string, number>();
  for (const record of ledger.conversationList())
    if (!record.stale)
      conversations.set(
        record.product,
        (conversations.get(record.product) ?? 0) + 1,
      );
  const views = ledger.topicList().map((record): TopicView => {
    const named = reports.get(record.topic) ?? [];
    const live = named.filter((report) => !report.stale);
    const covering = subscriptions.filter((entry) =>
      covers(entry, record.topic),
    );
    const open = covering.filter(
      (entry) => entry.state === "open" || entry.state === "opening",
    );
    const reportsListening = live.some(
      (report) => report.direction === "listen",
    );
    const reportsPosting = live.some((report) => report.direction === "post");
    const listening = open.length > 0 || reportsListening;
    const publishing = record.wentOut > 0 || reportsPosting;
    return {
      record,
      name:
        inboxName(record.topic, network, now) ??
        reportName(named, conversations),
      subscriptions: covering,
      open,
      reports: named,
      role:
        listening && publishing
          ? "Both"
          : listening
            ? "Listening"
            : publishing
              ? "Publishing"
              : record.submits > 0
                ? "Tried"
                : "Other observed",
      reportedOnly: {
        listening: listening && open.length === 0,
        publishing: publishing && record.wentOut === 0,
      },
    };
  });
  return views.sort(
    (a, b) =>
      ROLE_ORDER.indexOf(a.role) - ROLE_ORDER.indexOf(b.role) ||
      (b.record.lastAt ?? 0) - (a.record.lastAt ?? 0),
  );
}
