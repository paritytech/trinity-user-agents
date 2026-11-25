import { type Observable, catchError, from, map, merge, of, scan, startWith, takeUntil, timer } from 'rxjs';

import { createTransformer } from '@/shared/di';
import { dotNsService } from '@/domains/product';
import { HARD_DEADLINE_MS } from '../constants';
import { inputWorkerGateway } from '../gateway';
import { inputRoutingService } from '../service';
import {
  type Attachment,
  type AttachmentCategory,
  type AttachmentData,
  type CandidateNode,
  type InputCandidateContent,
  type InputResponse,
  type InputSource,
  type Query,
  type QueryRecipient,
  type QueryRound,
  type RoutedInput,
} from '../types';

export type AnswerQueryRequest = {
  productId: string;
  query: Query;
  source: InputSource;
};

/**
 * "Does anything want to answer this instead of the worker?"
 *
 * The only way a candidate can reach the surface while `input_request` is
 * unwired. A handler returns an `InputResponse` to answer, or nothing to pass —
 * so registering none leaves every product declining, which is the shipped
 * behaviour. Named for the question it asks, not for whoever answers it today.
 *
 * It sits here rather than beside the wire it stands in for because a handler is
 * registered from outside the domain, and the public surface may not re-export
 * `gateway.ts`. The gateway takes the answer as a parameter instead, so the wire
 * itself stays free of DI.
 */
export const answerQueryTransformer = createTransformer<AnswerQueryRequest, InputResponse>({ name: 'answerQuery' });

export type RenderCandidateRequest = {
  productId: string;
  candidateId: string;
  contentType: string;
  payload: Uint8Array;
};

/**
 * "Draw this candidate."
 *
 * RFC-0027 gives a custom candidate a `candidateId` precisely so a render call
 * and an action can both name one drawn thing — but there is no method to make
 * that call with. the product protocol exposes exactly one render
 * entry, `renderChatCustomMessage`, and nothing generic; an input candidate has
 * no equivalent. **This is an SDK gap, not a host decision.**
 *
 * The transformer is shaped like the subscription that will replace it — a
 * callback plus a teardown, because a tree is live and the product pushes a new
 * one when its state changes — so the real method drops in without moving the
 * seam. With no handler registered, nothing draws.
 */
export const renderCandidateTransformer = createTransformer<RenderCandidateRequest, CandidateNode>({
  name: 'renderCandidate',
});

/**
 * Resolves a committed string into a routed input.
 *
 * **The input syntax is not specified by RFC-0027** (Q4), so this host
 * recognizes exactly one form — the `polkadot://` deeplink it already generates
 * — and applies the normative fallthrough to everything else. No form currently
 * produces `chat` or `pocket`; those variants exist and are routable, but no
 * string resolves to them until the syntax lands.
 *
 * The fallthrough is normative: an input that resolves to no product becomes
 * `Query::Text` with the committed text **verbatim**, and is never promoted to a
 * navigation.
 */
function resolveInput(text: string, tld: string): RoutedInput {
  const trimmed = text.trim();
  const dotNsUrl = trimmed.length > 0 ? dotNsService.parseDotNsDomain(trimmed, tld) : null;

  if (dotNsUrl) {
    return { type: 'app', productId: dotNsUrl.identifier, path: dotNsUrl.pathname };
  }

  return { type: 'query', query: { type: 'text', text } };
}

/**
 * Builds an attachment from host-side file metadata, deriving the category from
 * the MIME type. A file the OS gave no type for is labelled
 * `application/octet-stream` before the category is derived — never guessed at.
 */
function toAttachment(data: AttachmentData): Attachment {
  const mimeType = inputRoutingService.normalizeMimeType(data.mimeType);

  return {
    category: inputRoutingService.attachmentCategoryOf(mimeType),
    data: { ...data, mimeType },
  };
}

/** Every product in the set, paired with what it declared. Filtering is the service's. */
function declaredBy(productIds: string[]): QueryRecipient[] {
  return productIds.map(productId => ({
    productId,
    declaration: inputWorkerGateway.declarationFor(productId),
  }));
}

/** The context set's products that declared they take text, after the context cap. */
function textRecipientsFor(productIds: string[]): QueryRecipient[] {
  return inputRoutingService.textRecipients(declaredBy(productIds));
}

/** The picker set for an attachment — products that declared its derived category. */
function attachmentRecipientsFor(productIds: string[], category: AttachmentCategory): QueryRecipient[] {
  return inputRoutingService.attachmentRecipients(declaredBy(productIds), category);
}

type RoundParams = {
  query: Query;
  source: InputSource;
  recipients: QueryRecipient[];
  signal: AbortSignal;
};

type Answer = { productId: string; response: InputResponse };

/**
 * Puts a query to its recipients and emits the merged list as answers arrive.
 *
 * Emitting incrementally satisfies the RFC's soft deadline by superset — results
 * are shown as soon as they exist, and late answers keep merging until the hard
 * deadline interrupts the round and discards the rest. Supersession is the
 * caller's `signal`: aborting it ends the round, which is what makes consecutive
 * commits interrupt each other.
 *
 * A response over the payload cap is rejected whole rather than truncated, and
 * counts as an answer so the round can still complete.
 */
function runQueryRound({ query, source, recipients, signal }: RoundParams): Observable<QueryRound> {
  const asked = recipients.length;
  const empty: QueryRound = { candidates: [], pending: asked > 0, asked };

  if (asked === 0) return of(empty);

  const notHandled = (productId: string): Answer => ({ productId, response: { type: 'notHandled' } });

  const answers$ = recipients.map(recipient =>
    from(
      inputWorkerGateway.queryWorker(recipient.productId, signal, () =>
        answerQueryTransformer({ productId: recipient.productId, query, source }),
      ),
    ).pipe(
      map<InputResponse, Answer>(response =>
        // Rejected whole — a half-decoded candidate list is worse than none.
        inputRoutingService.exceedsPayloadCap(response)
          ? notHandled(recipient.productId)
          : { productId: recipient.productId, response },
      ),
      // A worker that crashes or never answers loses its slot; the round
      // completes without it, indistinguishable from `notHandled`.
      catchError(() => of(notHandled(recipient.productId))),
    ),
  );

  return merge(...answers$).pipe(
    scan<Answer, Answer[]>((acc, answer) => [...acc, answer], []),
    map(answers => ({
      candidates: inputRoutingService.rankCandidates(answers),
      pending: answers.length < asked,
      asked,
    })),
    startWith(empty),
    takeUntil(timer(HARD_DEADLINE_MS)),
  );
}

/**
 * Delivers an attachment to the **one** product the user named in the picker.
 *
 * This is the invariant whose failure discloses user data rather than degrading
 * behaviour: an attachment carries its contents, so a host that fans it out to
 * find a taker hands the bytes to every declaring worker. Exactly one delivery,
 * to exactly the named product.
 */
async function deliverAttachment(
  productId: string,
  attachment: Attachment,
  source: InputSource,
  signal: AbortSignal,
): Promise<void> {
  await inputWorkerGateway.deliverInput({ type: 'query', query: { type: 'attachment', attachment } }, productId, source, signal);
}

/**
 * Tells the answering product its candidate was picked. The round it belonged to
 * has already closed, so this is terminal — the product's answer to being chosen
 * is whatever it does next, through the APIs it already holds.
 */
async function selectCandidate(productId: string, _content: InputCandidateContent, signal: AbortSignal): Promise<void> {
  await inputWorkerGateway.deliverSelection(productId, signal);
}

/**
 * Asks the product that answered to draw one of its custom candidates, and keeps
 * the subscription open until the caller drops it.
 *
 * Answered synchronously by whoever registered on the transformer, so there is
 * nothing to tear down yet; the real `input_*_render` subscription's teardown
 * goes where the no-op is. Everything outside reaches this domain through the
 * use case, so that day only this file changes.
 */
function renderCandidate(request: RenderCandidateRequest, onNode: (node: CandidateNode) => void): VoidFunction {
  const node = renderCandidateTransformer(request);
  if (node) onNode(node);

  return () => {};
}

export const routingUseCase = {
  resolveInput,
  toAttachment,
  textRecipientsFor,
  attachmentRecipientsFor,
  runQueryRound,
  deliverAttachment,
  selectCandidate,
  renderCandidate,
};
