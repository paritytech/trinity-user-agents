import { type RendererNode } from '@parity/truapi';

// Types from RFC-0027 (Input Routing), mapped to TypeScript. The Rust enums in
// the RFC become discriminated unions; field names are camelCased. Every union
// here is open in the RFC's sense — new variants arrive by protocol version, so
// consumers must handle an unknown one by its documented degrade target rather
// than by throwing.

/**
 * Where the input came from. RFC-0027 § Provenance.
 *
 * The split that matters is externally authored (`scanned`, `operating-system`)
 * versus user-originated (`user`), and satisfying a confirmation gate never moves
 * an input across it.
 */
export type InputSource = 'user' | 'scanned' | 'operating-system';

/** The bytes of an attachment and what they are. */
export type AttachmentData = {
  fileName: string;
  mimeType: string;
  content: Uint8Array;
};

/**
 * A user-supplied payload, categorized for handling.
 *
 * The category is **derived** from `mimeType`, never declared by the sender, and
 * carries no privilege: it says how the payload is labelled, not what the bytes
 * are. Dispatch on it, validate before decoding.
 */
export type AttachmentCategory = 'audio' | 'video' | 'image' | 'file';

export type Attachment = {
  category: AttachmentCategory;
  data: AttachmentData;
};

/** An input no product was named for. */
export type Query = { type: 'text'; text: string } | { type: 'attachment'; attachment: Attachment };

/**
 * What the host resolved a user input to. The first three name a product and one
 * place inside it, so they replace the screen rather than acting within it;
 * `query` names none, so it inherits the context it was entered in.
 */
export type RoutedInput =
  | { type: 'app'; productId: string; path: string }
  | { type: 'chat'; productId: string; roomId: string }
  | { type: 'pocket'; productId: string; artifactId: string }
  | { type: 'query'; query: Query };

/** One media attachment on a rich-text candidate. */
export type InputMedia = {
  url: string;
  alt?: string;
};

/** A file a candidate offers — a URL, never bytes; fetched only if taken. */
export type InputFile = {
  fileName: string;
  mimeType: string;
  sizeBytes: number;
  url: string;
};

/**
 * Richer body for a candidate. A candidate is content and nothing else: no
 * title, no URL of its own, no identity — the host frames it with the answering
 * product's manifest identity, which is what makes spoofing structurally
 * impossible.
 */
export type InputCandidateContent =
  | { type: 'text'; text: string }
  | { type: 'richText'; text?: string; media: InputMedia[] }
  | { type: 'file'; file: InputFile }
  // Product-defined bytes the host carries but never interprets. `candidateId` is
  // minted by the product and names one drawn thing, so a render call and an action
  // can both point at it.
  | { type: 'custom'; candidateId: string; contentType: string; payload: Uint8Array };

/**
 * A render tree the product drew for one custom candidate.
 *
 * The same closed vocabulary every product-rendered body is drawn from — the
 * product names layouts and design tokens, never markup or URLs — which is what
 * lets the host draw a product's own content without giving it the page.
 */
export type CandidateNode = RendererNode;

/** What a product has to say about one query. */
export type InputResponse =
  // The ordinary answer, not an error — no penalty, no telemetry failure.
  { type: 'notHandled' } | { type: 'candidates'; candidates: InputCandidateContent[] };

/**
 * A worker's declaration of the query *shapes* it accepts.
 *
 * Categories, not MIME patterns: a product declares in the same four terms a
 * delivery arrives in, so eligibility is a set-membership test. An unrecognized
 * member is ignored rather than rejecting the declaration.
 */
export type QueryDeclaration = {
  text: boolean;
  attachment: AttachmentCategory[];
};

/** An installed product's worker, as a candidate recipient for a query. */
export type QueryRecipient = {
  productId: string;
  declaration: QueryDeclaration;
};

/** A candidate merged into the host's ranked list, attributed to its answerer. */
export type RankedCandidate = {
  /** Host-local list identity. Not a protocol id. */
  id: string;
  /** The product that answered — the identity the host renders the candidate under. */
  productId: string;
  content: InputCandidateContent;
};

/** A snapshot of an in-progress query round. */
export type QueryRound = {
  candidates: RankedCandidate[];
  /** Recipients still outstanding within the hard deadline. */
  pending: boolean;
  /** How many workers were asked, after the fan-out cap. */
  asked: number;
};
