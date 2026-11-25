import { type JSModuleLoaderAsync, type JSModuleNormalizerAsync } from 'quickjs-emscripten';

import { type Logger } from './logger';

// Canonical module identity as understood by the loader.
export type ModuleId = string;

// Default normalizer: POSIX-joins a relative specifier against the importer's
// directory. Bare / absolute specifiers (and relative ones with no importer)
// are returned unchanged. Pure — no I/O.
export type DefaultResolve = (specifier: string, importer: ModuleId | null) => ModuleId;

export type ResolvedModule = { filename: ModuleId; content: string | Uint8Array };

// Host hook returning ES module source on demand. `importer` is the resolved
// filename of the module issuing the import, or `null` for the entrypoint's
// own imports. Return `null` to signal "not found"; throw/reject to surface a
// load error. The third argument delegates the common path computation:
// `(specifier, importer, defaultResolve) => archive[defaultResolve(specifier, importer)]`.
export type ModuleResolver = (
  specifier: string,
  importer: ModuleId | null,
  defaultResolve: DefaultResolve,
) => Promise<ResolvedModule | null> | ResolvedModule | null;

const utf8Decoder = new TextDecoder('utf-8');

function dirname(id: ModuleId): string {
  const idx = id.lastIndexOf('/');
  return idx === -1 ? '' : id.slice(0, idx);
}

// POSIX-style path join with `.`/`..` collapsing. Bare specifiers pass through.
export const defaultResolve: DefaultResolve = (specifier, importer) => {
  const isRelative = specifier.startsWith('./') || specifier.startsWith('../');
  if (!isRelative || importer == null) return specifier;

  const base = dirname(importer);
  const segments = base === '' ? [] : base.split('/');
  for (const part of specifier.split('/')) {
    if (part === '' || part === '.') continue;
    if (part === '..') segments.pop();
    else segments.push(part);
  }
  return segments.join('/');
};

type ModuleLoaderDeps = {
  resolver: ModuleResolver;
  logger: Logger;
  // Read at the await boundary so a resolution that settles after the sandbox
  // is torn down never writes source destined for a freed VM.
  isDisposed: () => boolean;
};

// What the loader hands back for a given canonical name: either decoded source
// to evaluate, or an error to surface.
type ModuleEntry = { source: string } | { error: Error };

// Builds the QuickJS module loader/normalizer pair backing `resolveModule`.
// The resolver call lives in the normalizer because that is the stage that
// owns the (specifier, importer) pair and chooses the canonical filename; the
// loader then just hands back whatever the normalizer recorded. State is local
// to this closure — it lives and dies with the VM that captures it.
export function createModuleLoader(deps: ModuleLoaderDeps): {
  moduleLoader: JSModuleLoaderAsync;
  moduleNormalizer: JSModuleNormalizerAsync;
} {
  const { resolver, logger, isDisposed } = deps;

  // Canonical name → entry. The normalizer is the only writer; the loader is
  // the only reader.
  const entries = new Map<ModuleId, ModuleEntry>();

  // Resolver failures cannot be reported by returning `{ error }` from the
  // (async) normalizer: quickjs-emscripten does not abort the load in that
  // case — it invokes the loader with an empty name. Loader errors, in
  // contrast, propagate cleanly. So on failure the normalizer records the
  // error under a unique sentinel name and returns that name; the loader then
  // emits the error. The sentinel is plain ASCII (a `\0` prefix would be
  // truncated to "" crossing the C string FFI) and namespaced so a collision
  // with a real module name is implausible.
  let errorSeq = 0;
  const recordError = (err: Error): ModuleId => {
    const key = `host-sandbox-module-error:${errorSeq++}`;
    entries.set(key, { error: err });
    return key;
  };

  const moduleNormalizer: JSModuleNormalizerAsync = async (baseModuleName, requestedName) => {
    const importer = baseModuleName === '' ? null : baseModuleName;

    let resolved: ResolvedModule | null;
    try {
      resolved = await resolver(requestedName, importer, defaultResolve);
    } catch (e) {
      logger.error('[Sandbox] resolveModule threw', e);
      return recordError(e instanceof Error ? e : new Error(String(e)));
    }

    if (resolved == null) {
      return recordError(new Error(`Module not found: ${importer ?? '<entry>'} -> ${requestedName}`));
    }

    if (isDisposed()) {
      return recordError(new Error('Sandbox disposed during module load'));
    }

    // Only the first source wins for a given filename — QuickJS caches the
    // module by canonical name, so a later import resolving to the same
    // filename reuses the already-bootstrapped module (dedup).
    if (!entries.has(resolved.filename)) {
      const content = typeof resolved.content === 'string' ? resolved.content : utf8Decoder.decode(resolved.content);
      entries.set(resolved.filename, { source: content });
    }

    return resolved.filename;
  };

  const moduleLoader: JSModuleLoaderAsync = moduleName => {
    const entry = entries.get(moduleName);
    if (entry === undefined) {
      // Defensive: the normalizer always records an entry before returning a
      // name, so this indicates an internal invariant break.
      return { error: new Error(`Internal: source missing for ${moduleName}`) };
    }
    return 'error' in entry ? { error: entry.error } : entry.source;
  };

  return { moduleLoader, moduleNormalizer };
}
