import {
  type Observable,
  Subject,
  catchError,
  debounce,
  distinctUntilChanged,
  finalize,
  from,
  map,
  of,
  switchMap,
  timer,
} from 'rxjs';

import { createState } from '@/shared/rxstate';
import {
  type InputSource,
  type QueryRound,
  type RankedCandidate,
  type RoutedInput,
  DEBOUNCE_MS,
  routingUseCase,
} from '@/domains/input-routing';
import { DEFAULT_DOTNS_TLD, dotNsUseCase } from '@/domains/product';

/**
 * What the surface is currently showing. There is no transcript: a round is a
 * live view of the string in the field, superseded by the next one.
 */
export type RoundView =
  | { kind: 'idle' }
  | { kind: 'navigation'; routed: RoutedInput }
  | { kind: 'query'; candidates: RankedCandidate[]; pending: boolean; asked: number };

const IDLE: RoundView = { kind: 'idle' };

type RoundRequest = { text: string; productIds: string[]; source: InputSource };

export const inputRound = createState<RoundView>(IDLE);

const requests$ = new Subject<RoundRequest>();

function runRequest({ text, productIds, source }: RoundRequest): Observable<RoundView> {
  if (text.trim().length === 0) return of(IDLE);

  // The TLD is sourced here rather than carried on the request: `resetRound`
  // pushes a request too and has no TLD to give. `catchError` is not optional —
  // the subscription below has no error handler, so a rejected read would kill
  // input routing for the rest of the session.
  return from(dotNsUseCase.getActiveTld()).pipe(
    // Only reached if the TLD read itself fails. `.dot` is right on mainnet and
    // wrong everywhere else, but the input still routes instead of being dropped.
    catchError(() => of(DEFAULT_DOTNS_TLD)),
    switchMap(tld => runResolvedRequest({ text, productIds, source }, tld)),
  );
}

function runResolvedRequest({ text, productIds, source }: RoundRequest, tld: string): Observable<RoundView> {
  // Resolution comes before the context check, and the order is load-bearing. A
  // navigation names its own product and does not consult the context set, so a
  // deeplink typed on a screen with no products still resolves.
  const routed = routingUseCase.resolveInput(text, tld);
  if (routed.type !== 'query') return of<RoundView>({ kind: 'navigation', routed });

  // A screen with nothing on it asks nobody, so no product learns the string.
  if (productIds.length === 0) return of(IDLE);

  const recipients = routingUseCase.textRecipientsFor(productIds);
  const controller = new AbortController();

  return routingUseCase.runQueryRound({ query: routed.query, source, recipients, signal: controller.signal }).pipe(
    map<QueryRound, RoundView>(round => ({ kind: 'query', ...round })),
    finalize(() => controller.abort(new DOMException('Round ended', 'AbortError'))),
  );
}

// `debounce`, not `debounceTime`: a reset must not wait. A round left running for
// the debounce window keeps emitting and overwrites the IDLE that `resetRound`
// just set — the user closes mid-round, reopens, and sees the previous screen's
// candidates. `switchMap` is the supersession itself: subscribing to the next
// round tears down the previous one, which aborts its signal through `finalize`.
requests$
  .pipe(
    debounce(request => timer(request.text.trim() ? DEBOUNCE_MS : 0)),
    distinctUntilChanged((a, b) => a.text === b.text && a.productIds.join() === b.productIds.join()),
    switchMap(runRequest),
  )
  .subscribe(view => inputRound.set(view));

/** Ask for a round. Nothing is routed until the debounce elapses. */
export function requestRound(text: string, productIds: string[], source: InputSource) {
  requests$.next({ text, productIds, source });
}

/**
 * Ends the open round and clears the view.
 *
 * The push through `requests$` is what tears the round down — setting the state
 * alone would leave the inner subscription alive to overwrite IDLE. RFC-0027
 * § Corner cases: "Candidates from a context the user has left are never merged
 * into a new one."
 */
export function resetRound() {
  requests$.next({ text: '', productIds: [], source: 'user' });
  inputRound.set(IDLE);
}
