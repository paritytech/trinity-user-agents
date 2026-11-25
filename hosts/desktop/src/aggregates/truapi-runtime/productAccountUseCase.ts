import { type ProductAccountId, DerivationIndex } from '@parity/truapi';
import init, { deriveProductAccountPublicKey, productAccountAddress } from '@parity/truapi-host/wasm/web';

import { truapiRuntime } from './state/runtime';

// The derivation wasm is loaded once, lazily. `deriveProductAccountPublicKey` /
// `productAccountAddress` are pure — they need no runtime or session, only the module
// loaded via its default init — so the host can derive an address off the main thread's
// own copy without touching the worker runtime.
let wasmReady: Promise<unknown> | null = null;
function ensureWasm(): Promise<unknown> {
  wasmReady ??= init();

  return wasmReady;
}

/**
 * The SS58 address of a product account, or `null` when it cannot be derived yet.
 *
 * The core keeps the wallet's product subtree private key and exposes only its public
 * half (`getProductSubtreePublicKey`), cached once the account holder authorizes the
 * product. From that public key plus the account's derivation index the host derives
 * the account's public key and its address, seeing no secret. `null` while the runtime
 * is absent or the core has not cached the subtree key.
 */
async function deriveAddress(account: ProductAccountId): Promise<string | null> {
  const runtime = truapiRuntime.get().runtime;
  if (!runtime) return null;

  const subtreeKey = await runtime.getProductSubtreePublicKey(account.dotNsIdentifier);
  if (!subtreeKey) return null;

  await ensureWasm();
  const publicKey = deriveProductAccountPublicKey(subtreeKey, DerivationIndex.enc(account.derivationIndex));

  return productAccountAddress(publicKey);
}

export const productAccountUseCase = {
  deriveAddress,
};
