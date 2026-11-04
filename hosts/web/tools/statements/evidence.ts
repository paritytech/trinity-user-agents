import type { StatementLedger } from "./ledger.js";

/**
 * The Statements view's evidence: what the ledger holds, as JSON a developer
 * can copy and attach to a report. Metadata only: it never holds statement
 * data, captured or not, and the ledger holds no key, seed or secret to put
 * in it. Accounts, topics, channels, hashes, request handles and derivation
 * indexes are public or local identifiers and stay, since they are what the
 * evidence is for.
 */

/** The version of the evidence's shape. */
export const EVIDENCE_FORMAT = "truapi-web-host/statements-evidence/3";

export interface EvidenceContext {
  network: string;
  exportedAt: number;
}

export function evidence(ledger: StatementLedger, context: EvidenceContext) {
  const statements = ledger.statementList().map((statement) => ({
    meta: statement.meta,
    firstAt: statement.firstAt,
    lastAt: statement.lastAt,
    sightings: statement.sightings,
    via: statement.via,
    subscriptions: statement.subscriptions,
    submits: statement.submits,
  }));
  return {
    format: EVIDENCE_FORMAT,
    network: context.network,
    exportedAt: new Date(context.exportedAt).toISOString(),
    sessionStartedAt: new Date(ledger.sessionStart()).toISOString(),
    peopleBackend: ledger.transport.backend,
    transport: ledger.transport.note,
    notes: [
      "Metadata only: statement data is never exported, captured or not.",
      "Counters count since the session started in this tab; lists keep their newest entries within bounds, and `dropped` says how many went.",
      "`latestObserved` per account counts the statements this tab last saw (latest per channel, not known expired, its own broadcasts included), with when it last saw them. It is an observation, not what the network holds now and not a bound on it: peers refuse and evict without saying so, and send only some of what they hold.",
      "A statement's `firstAt` and `lastAt` say when this tab observed it, not that any peer still holds it.",
      ...ledger.transport.evidenceNotes,
    ],
    counters: ledger.counts(),
    dropped: ledger.dropped(),
    connections: ledger.connectionList(),
    submits: ledger.submitList(),
    subscriptions: ledger.subscriptionList(),
    statements,
    channels: ledger.channelList(),
    accounts: ledger.accountList().map((account) => ({
      ...account,
      latestObserved: ledger.latestObserved(account.account),
    })),
    topics: ledger.topicList(),
    requests: ledger.requestList(),
    conversations: ledger.conversationList(),
    events: ledger.eventList(),
  };
}

/** {@link evidence} as indented JSON. */
export const evidenceText = (
  ledger: StatementLedger,
  context: EvidenceContext,
): string => JSON.stringify(evidence(ledger, context), null, 2);
