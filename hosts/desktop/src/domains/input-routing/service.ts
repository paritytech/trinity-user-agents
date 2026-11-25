import { ATTACHMENT_CAP_BYTES, CANDIDATE_CAP, CONTEXT_CAP, DEFAULT_MIME_TYPE, PAYLOAD_CAP_BYTES } from './constants';
import {
  type AttachmentCategory,
  type InputCandidateContent,
  type InputResponse,
  type QueryRecipient,
  type RankedCandidate,
} from './types';

/** Strips MIME parameters and case-folds, so `text/plain; charset=utf-8` → `text/plain`. */
function normalizeMimeType(mimeType: string): string {
  const [type] = mimeType.split(';');

  return (type ?? '').trim().toLowerCase() || DEFAULT_MIME_TYPE;
}

/**
 * The attachment category, derived from the MIME type — never declared by the
 * sender. An absent type becomes `application/octet-stream`, which lands in
 * `file` rather than being guessed at.
 */
function attachmentCategoryOf(mimeType: string): AttachmentCategory {
  const [topLevel] = normalizeMimeType(mimeType).split('/');

  switch (topLevel) {
    case 'image':
      return 'image';
    case 'audio':
      return 'audio';
    case 'video':
      return 'video';
    default:
      return 'file';
  }
}

/**
 * The context set's text recipients: the products the screen contributed that
 * declared `query.text`, truncated to the context cap.
 *
 * The declaration is an efficiency hint here, not consent — a product in context
 * is on screen regardless, and the flag only saves starting a worker to be told
 * `notHandled`.
 */
function textRecipients(recipients: QueryRecipient[]): QueryRecipient[] {
  return recipients.filter(recipient => recipient.declaration.text).slice(0, CONTEXT_CAP);
}

/**
 * The picker set for an attachment: products that declared the derived category.
 * No cap — this is a list the user chooses one name from, and listing a product
 * discloses nothing to it.
 */
function attachmentRecipients(recipients: QueryRecipient[], category: AttachmentCategory): QueryRecipient[] {
  return recipients.filter(recipient => recipient.declaration.attachment.includes(category));
}

const textEncoder = new TextEncoder();

/** Approximate encoded size of a response, for the payload cap. */
function encodedSize(response: InputResponse): number {
  if (response.type === 'notHandled') return 0;

  return response.candidates.reduce((total, content) => {
    if (content.type === 'custom') return total + content.payload.byteLength + content.contentType.length;

    return total + textEncoder.encode(JSON.stringify(content)).byteLength;
  }, 0);
}

/** An attachment over the host's cap is declined before delivery, to the user rather than the product. */
function exceedsAttachmentCap(sizeBytes: number): boolean {
  return sizeBytes > ATTACHMENT_CAP_BYTES;
}

/** A response over the payload cap is rejected whole — a half-decoded list is worse than none. */
function exceedsPayloadCap(response: InputResponse): boolean {
  return encodedSize(response) > PAYLOAD_CAP_BYTES;
}

/**
 * Merges per-product responses into one list.
 *
 * Ranking across mutually untrusted products is unresolved in the RFC (Q5). This
 * host takes the per-product quota option: each response is capped at
 * `CANDIDATE_CAP`, then the lists are interleaved round-robin, preserving each
 * product's own confidence order within its slots. A product that answers every
 * query with a plausible candidate therefore gains one row, not the list.
 *
 * Every candidate here is a context candidate — this host never queries beyond the
 * screen, so there is no outside band to order below.
 */
function rankCandidates(responses: { productId: string; response: InputResponse }[]): RankedCandidate[] {
  const lists: { productId: string; contents: InputCandidateContent[] }[] = [];
  for (const entry of responses) {
    if (entry.response.type !== 'candidates') continue;
    const contents = entry.response.candidates.slice(0, CANDIDATE_CAP);
    if (contents.length === 0) continue;
    lists.push({ productId: entry.productId, contents });
  }

  const ranked: RankedCandidate[] = [];
  const depth = Math.max(0, ...lists.map(list => list.contents.length));

  for (let index = 0; index < depth; index++) {
    for (const list of lists) {
      const content = list.contents[index];
      if (!content) continue;
      ranked.push({ id: `${list.productId}:${index}`, productId: list.productId, content });
    }
  }

  return ranked;
}

export const inputRoutingService = {
  normalizeMimeType,
  attachmentCategoryOf,
  textRecipients,
  attachmentRecipients,
  exceedsAttachmentCap,
  exceedsPayloadCap,
  rankCandidates,
};
