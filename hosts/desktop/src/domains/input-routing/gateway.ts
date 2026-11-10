import { type InputResponse, type InputSource, type QueryDeclaration, type RoutedInput } from './types';

/**
 * The product-worker boundary — RFC-0027's `input_request` method, seen from the
 * host side.
 *
 * **Every implementation here is a mock.** Nothing crosses a real TrUAPI
 * connection: no worker is launched, no subscription is opened, no bytes leave
 * the renderer. This is the single file that the real wiring replaces, and the
 * only place in the domain that knows the transport is fake.
 *
 * The mock is deliberately faithful in the ways that matter for the host logic
 * being demonstrated: it honours the abort signal, it takes a variable amount of
 * time to answer (so deadlines and incremental merge are exercised), and most
 * workers answer `notHandled` for most queries — the RFC's expected steady state.
 */

// Every product is assumed to take text and any attachment. The real read is
// `query` off the worker executable manifest (`workerManifestSchema.query`), which
// is absent until the manifest RFC lands; until then a permissive default is what
// lets a screen's real products answer at all.
const MOCK_DECLARATION: QueryDeclaration = {
  text: true,
  attachment: ['audio', 'video', 'image', 'file'],
};

function declarationFor(_productId: string): QueryDeclaration {
  return MOCK_DECLARATION;
}

function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener(
      'abort',
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });
}

// Deterministic per (product, query) so a demo repeats identically.
function hashOf(value: string): number {
  let hash = 0;
  for (const char of value) {
    hash = (hash * 31 + char.charCodeAt(0)) | 0;
  }

  return Math.abs(hash);
}

/**
 * Put one query to one worker. Rejects with the signal's reason when the round
 * is superseded or the hard deadline interrupts it.
 *
 * **Nothing here fabricates an answer.** `answer` is what the worker replied;
 * with nothing to say it declines, which is what an unwired host should report —
 * an invented candidate that looks like a real action is worse than no answer,
 * because the only way to find out it does nothing is to press it. The parameter
 * exists only while the wire is mocked: the real `input_request` returns its own
 * response and the caller stops supplying one.
 *
 * Real candidates arrive when `input_request` is implemented and this file is
 * replaced; the host-side merge, ranking, caps and deadlines are exercised by
 * `service.spec.ts` and `state/round.spec.ts` in the meantime.
 *
 * The delay stays: it is what makes the round's pending state, the incremental
 * merge and both deadlines observable.
 */
async function queryWorker(productId: string, signal: AbortSignal, answer: () => InputResponse | null): Promise<InputResponse> {
  await delay(120 + (hashOf(productId) % 900), signal);

  return answer() ?? { type: 'notHandled' };
}

/**
 * Deliver `CandidateSelected` back to the product that answered. The product
 * then acts through the APIs it already holds — nothing it returned is
 * interpreted by the host as an instruction.
 */
async function deliverSelection(productId: string, signal: AbortSignal): Promise<void> {
  await delay(50, signal);
  console.info('[input-routing] selection delivered', { productId });
}

/** Deliver a routed input to exactly one named product. */
async function deliverInput(input: RoutedInput, productId: string, source: InputSource, signal: AbortSignal): Promise<void> {
  await delay(50, signal);
  console.info('[input-routing] delivered to exactly one product', { productId, input, source });
}

export const inputWorkerGateway = {
  declarationFor,
  queryWorker,
  deliverSelection,
  deliverInput,
};
