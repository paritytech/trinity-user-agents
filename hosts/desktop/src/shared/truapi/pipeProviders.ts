import { type WireProvider } from '@parity/truapi';

import { nonNullable } from '@/shared/utils';

/**
 * Join two TrUAPI wire providers back to back: every frame one side emits is
 * posted to the other, and a close on either side disposes its peer.
 *
 * The host sits between a product transport (a webview channel, a worker port)
 * and a core provider from the WASM runtime. Both ends speak the same SCALE
 * wire, so the host never decodes — it only forwards.
 *
 * `subscribeClose` is optional on `WireProvider`; a provider without it simply
 * contributes no close propagation. Returns an idempotent teardown that stops
 * forwarding but leaves disposal of the providers to their owners.
 */
export function pipeProviders(product: WireProvider, core: WireProvider): VoidFunction {
  const unsubscribes = [
    product.subscribe(core.postMessage.bind(core)),
    core.subscribe(product.postMessage.bind(product)),
    product.subscribeClose?.(core.dispose.bind(core)),
    core.subscribeClose?.(product.dispose.bind(product)),
  ].filter(nonNullable);

  let disposed = false;

  return () => {
    if (disposed) return;
    disposed = true;

    for (const unsubscribe of unsubscribes) {
      unsubscribe();
    }
  };
}
