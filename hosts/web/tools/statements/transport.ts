import type { ChainBackend } from "../../src/platform/network-config.js";

/**
 * What this tab's People connection is, by the backend it runs on, and what
 * that means for every Statement Store fact the inspector shows. The backend
 * is chosen before the core starts and holds for the page, so one transport
 * describes the whole session.
 *
 * - Light client (smoldot-light 2.1.0, `src/json_rpc_service/statement.rs`
 *   and `background.rs`): no statement store. `statement_submit` checks
 *   expiry, size and proof, then broadcasts; `new` means at least one peer
 *   took the broadcast, and peers' own refusals never reach it. A
 *   subscription's first page is always empty with `remaining: 0`; later
 *   pages carry what peers send. Every People connection shares one topic
 *   filter that is sent to peers.
 * - RPC node: a People node with its own statement store (polkadot-sdk
 *   `substrate/primitives/statement-store/src/store_api.rs`, `SubmitResult`,
 *   read 2026-11-04). `new` means the node accepted the statement into its
 *   store, which then sends it on; `rejected` and `invalid` carry this
 *   node's reason. Other nodes check it again and do not tell this tab. A
 *   subscription's first pages replay what the node holds, with `remaining`.
 *   The host trusts the node's answers.
 */
export interface PeopleTransport {
  backend: ChainBackend;
  /** The source tag of what the connection answered or sent. */
  source: "light-client" | "node";
  /** The whole note, kept in the evidence. */
  note: string;
  /** The first lines of "About this data": source, answers, and what observed means. */
  about: string[];
  /** How a submission answered `new` or `known` is counted: broadcast or accepted. */
  wentOut: string;
  /** What a `new` answer to a submission means, for its timeline line. */
  newAnswer: string;
  /** The label of the refusals this tab cannot see. */
  refusalsLabel: string;
  /** What this tab cannot see of refusals. */
  peerRefusals: string;
  /** The note under a submission answered `new` or `known`. */
  wentOutNote: string;
  /** What an account's asked submissions do not show. */
  postsNote: string;
  /** Why a submission the no-submit guard refused reached no one. */
  refusedHereNote: string;
  /** What a topic's submissions do not show. */
  topicPostsNote: string;
  /** What the statements observed on a topic do not say. */
  topicObservedNote: string;
  /** What the latest observed statements of an account do not say. */
  latestObservedNote: string;
  /** Why the view opens no subscription of its own. */
  noProbeNote: string;
  /** The topics list's Role column, explained. */
  roleColumn: string;
  /** What an open subscription does not show. */
  openSubscription: string;
  /** Where a page after a subscription's initial batch comes from. */
  laterPages: string;
  /** What an empty, complete initial batch means. */
  emptyInitialBatch: string;
  /** The evidence's notes on refusals and on sources. */
  evidenceNotes: string[];
  /** What a chat worker's per-message cost counts. */
  chatCost: string;
}

/** The People connection over `backend`. */
export function peopleTransport(backend: ChainBackend): PeopleTransport {
  return backend === "rpc" ? NODE : LIGHT_CLIENT;
}

const LIGHT_CLIENT: PeopleTransport = {
  backend: "light-client",
  source: "light-client",
  note: "Source: this tab's embedded light client. It holds no statements. A submission answered new went to at least one peer, which may still refuse it; peers' refusals never reach this tab. A subscription's first page is always empty; later pages carry what peers send, new or long stored.",
  about: [
    "Source: this tab's light client. It holds no statements.",
    "new = broadcast to a peer, not stored. Peers' refusals never reach this tab.",
    "Observed ≠ current: a first page is always empty; later pages carry what peers send.",
  ],
  wentOut: "broadcast",
  newAnswer: "broadcast (new): at least one peer took it; not shown stored",
  refusalsLabel: "Peer refusals",
  peerRefusals:
    "Peers check allowance, account room and channel priority themselves and do not tell the light client. No refusal shown does not mean none happened.",
  wentOutNote:
    "Peers check allowance, account room and channel priority themselves, and their refusals do not reach this tab.",
  postsNote:
    "Submissions signed by this account that the core or the host asked for. Asking does not mean the light client got it.",
  refusedHereNote:
    "This tab submits nothing: its no-submit guard answered these before the light client, so no peer got them.",
  topicPostsNote:
    "Tried: submissions on this topic the core or the host asked for, whatever the answer. Broadcast: those the light client answered new: a peer took it, not that a peer stored it.",
  topicObservedNote:
    "Statements this tab saw on it: its own broadcasts and what peers sent its subscriptions, replays included. Not what the network holds now.",
  latestObservedNote:
    'Latest observed per channel, not known expired, this tab\'s own broadcasts included. Not what the network holds now: peers may have refused or evicted them without saying so, or hold others no one sent here. "Dropped" counts channels whose latest statement left the list.',
  noProbeNote:
    "In this host's light client every People connection shares one topic filter that is sent to peers, and each connection's subscriptions replace it, so a subscription made to look would take the chat's topics off it. An open subscription here is one the core or the host made; it does not show that peers send to it.",
  roleColumn:
    "Role: Listening = an open subscription here, or a product says it reads. Publishing = a submission here was broadcast (not shown stored), or a product says it posts. Tried = submissions asked, none known to have gone out. Reported: only a product said it.",
  openSubscription: "Open does not mean peers send to it.",
  laterPages: "after the initial batch: what peers sent, new or long stored",
  emptyInitialBatch:
    "the initial batch, empty: the light client holds no statements",
  evidenceNotes: [
    "No refusal recorded does not mean none happened: peers' allowance, account-room and priority refusals do not reach the light client.",
    "Each fact keeps its source: light-client answers and pages, core claim results, host request facts, chain reads at a block, and product reports as the product gave them.",
  ],
  chatCost:
    "submissions answered new (broadcast, not shown stored) signed by its account",
};

const NODE: PeopleTransport = {
  backend: "rpc",
  source: "node",
  note: "Source: People's RPC node, which keeps a statement store; the host trusts its answers. A submission answered new was accepted into that node's store, which sends it on; other nodes check it again and may refuse or evict it without telling this tab. This node's own refusals reach this tab with their reason. A subscription's first pages replay what the node holds on its filter; later pages carry what reaches the node, new or long stored.",
  about: [
    "Source: People's RPC node. It keeps a statement store; the host trusts its answers.",
    "new = accepted into that node's store. Its refusals reach this tab; other nodes' do not.",
    "Observed ≠ current: first pages replay what the node holds; later pages carry what reaches it.",
  ],
  wentOut: "accepted",
  newAnswer:
    "accepted (new): in the node's store; other nodes may still refuse it",
  refusalsLabel: "Other nodes' refusals",
  peerRefusals:
    "The node's own refusals show here, with their reason. Other nodes check allowance, account room and channel priority again and do not tell this tab. No refusal shown does not mean none happened there.",
  wentOutNote:
    "The node took it into its store. Other nodes check allowance, account room and channel priority again, and their refusals do not reach this tab.",
  postsNote:
    "Submissions signed by this account that the core or the host asked for. Asking does not mean the node got it.",
  refusedHereNote:
    "This tab submits nothing: its no-submit guard answered these before the People node, so no node got them.",
  topicPostsNote:
    "Tried: submissions on this topic the core or the host asked for, whatever the answer. Accepted: those the node answered new or known: in that node's store when it answered, not that other nodes keep it.",
  topicObservedNote:
    "Statements this tab saw on it: its own accepted submissions and what the node sent its subscriptions, replays included. Not what the network holds now.",
  latestObservedNote:
    'Latest observed per channel, not known expired, this tab\'s own accepted submissions included. Not what the network holds now: nodes may evict them without saying so, or hold others no one sent here. "Dropped" counts channels whose latest statement left the list.',
  noProbeNote:
    "The view opens no subscription of its own. An open subscription here is one the core or the host made, each with its own filter on the People node.",
  roleColumn:
    "Role: Listening = an open subscription here, or a product says it reads. Publishing = the node accepted a submission here (not shown kept by other nodes), or a product says it posts. Tried = submissions asked, none known to have been accepted. Reported: only a product said it.",
  openSubscription: "Open does not mean anything is posted to it.",
  laterPages:
    "after the initial batch: what reached the node, new or long stored",
  emptyInitialBatch:
    "the initial batch, empty: the node holds none on this filter",
  evidenceNotes: [
    "Refusals recorded are the People node's own. No refusal recorded does not mean none happened: other nodes' allowance, account-room and priority refusals do not reach this tab.",
    "Each fact keeps its source: People node answers and pages, trusted from that node; core claim results, host request facts, chain reads at a block, and product reports as the product gave them.",
  ],
  chatCost:
    "submissions answered new (accepted into the People node's store) signed by its account",
};
