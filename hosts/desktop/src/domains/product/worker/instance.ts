import { type TrUApiProductProvider } from '@parity/truapi-host';

import {
  type FetchResolver,
  type ModuleResolver,
  type SubtleResolver,
  createSandbox as defaultCreateSandbox,
} from '@/shared/sandbox';
import { pipeProviders } from '@/shared/truapi';
import { manifestService } from '../product/manifest/service';

import { type ProductWorkerInstance } from './types';

type CreateProductWorkerArgs = {
  productId: string;
  contenthash: string;
  /** Product archive files keyed by path; the entrypoint and its imports are resolved from here. */
  files: Record<string, Uint8Array>;
  /** Entrypoint module id (the manifest entrypoint). */
  entrypoint: string;
  /**
   * The core's end of the worker's wire. Created by the caller — `instance.ts` lives in
   * `domains/` and may not reach the runtime aggregate that owns the core.
   */
  coreProvider: TrUApiProductProvider;
  /**
   * Permission-gated network executor for the worker's `fetch`. Built in the renderer (see
   * createWorkerFetchResolver) and passed in so the host-environment wiring stays out of the domain.
   * Omitted ⇒ in-VM `fetch` is unavailable.
   */
  fetchResolver?: FetchResolver;
  /** Injectable for tests. Defaults to host-worker-sandbox.createSandbox. */
  createSandbox?: typeof defaultCreateSandbox;
};

// `crypto.subtle` needs no permission gate, so it is delegated straight to the host WebCrypto
// in-place. The discriminated union keeps each method's args correlated to its overload.
const subtleResolver: SubtleResolver = call => {
  switch (call.method) {
    case 'digest':
      return crypto.subtle.digest(...call.args);
    case 'sign':
      return crypto.subtle.sign(...call.args);
    case 'verify':
      return crypto.subtle.verify(...call.args);
    case 'encrypt':
      return crypto.subtle.encrypt(...call.args);
    case 'decrypt':
      return crypto.subtle.decrypt(...call.args);
    case 'generateKey':
      return crypto.subtle.generateKey(...call.args);
    case 'deriveBits':
      return crypto.subtle.deriveBits(...call.args);
    case 'deriveKey':
      return crypto.subtle.deriveKey(...call.args);
    case 'importKey':
      return crypto.subtle.importKey(...call.args);
    case 'exportKey':
      return crypto.subtle.exportKey(...call.args);
    case 'wrapKey':
      return crypto.subtle.wrapKey(...call.args);
    case 'unwrapKey':
      return crypto.subtle.unwrapKey(...call.args);
  }
};

export async function createProductWorker(args: CreateProductWorkerArgs): Promise<ProductWorkerInstance> {
  const { productId, contenthash, files, entrypoint, coreProvider, fetchResolver, createSandbox = defaultCreateSandbox } = args;

  // The entrypoint is just another archive module; resolve it through the same lookup that backs
  // ES-module imports so the run target and its `import`s share one resolution path.
  const code = manifestService.lookupArchiveFile(files, entrypoint);
  if (code === undefined) {
    throw new Error(`[ProductWorker] entrypoint "${entrypoint}" not found in archive for ${productId}`);
  }

  // The sandbox calls the resolver for every import the worker encounters; an unknown path
  // resolves to null, which surfaces as a "Module not found" error inside the VM.
  const resolveModule: ModuleResolver = (specifier, importer, defaultResolve) => {
    const filename = defaultResolve(specifier, importer);
    const content = manifestService.lookupArchiveFile(files, filename);
    return content === undefined ? null : { filename, content };
  };

  const sandbox = await createSandbox(productId, { resolveModule, subtleResolver, ...(fetchResolver ? { fetchResolver } : {}) });

  // The worker speaks the same SCALE wire as any other product surface; joining its
  // sandbox channel to the core provider is the whole host integration.
  const unpipe = pipeProviders(sandbox.provider, coreProvider);

  let disposed = false;

  const instance: ProductWorkerInstance = {
    productId,
    contenthash,
    sandbox,
    coreProvider,
    get disposed() {
      return disposed;
    },
    dispose: () => {
      if (disposed) return;
      disposed = true;
      // Stop forwarding before tearing either end down, or a frame in flight lands
      // on a disposed transport.
      unpipe();
      try {
        coreProvider.dispose();
      } catch (e) {
        console.warn('[ProductWorker] coreProvider.dispose threw', e);
      }
      try {
        sandbox.dispose();
      } catch (e) {
        console.warn('[ProductWorker] sandbox.dispose threw', e);
      }
    },
  };

  // Fire-and-forget: a worker subscribes to events and never resolves.
  // Late rejection after dispose is expected — swallow it.
  sandbox.run(code, { name: entrypoint }).catch(err => {
    if (disposed) return;
    console.error(`[ProductWorker] Worker error for ${productId}:`, err);
  });

  return instance;
}
