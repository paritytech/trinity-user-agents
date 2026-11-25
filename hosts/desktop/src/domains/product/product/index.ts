// Product entity — canonical struct + persistence + service helpers.
export type { Product } from './types';
export { type PersistedProduct, productDb } from './repository';
export { productsResource } from './resource';
export { useDisplayedProduct, useIsPinned, useIsProductInstalled, usePersistedProductById, usePersistedProducts } from './hooks';
export { productService } from './service';
export { useDeclineUpdate } from './declined-updates/hooks';

// Manifest sub-module — product manifest wire schemas + per-executable archive loader.
export { manifestService } from './manifest/service';
export { type ExecutableKind, EXECUTABLE_KINDS } from './manifest/constants';
export { executableArchiveResource, invalidateExecutableArchive } from './manifest/resource';
export { useExecutableArchive, useLiveExecutable, useProductIcon } from './manifest/hooks';
export type {
  AppExecutable,
  Executable,
  ExecutableContent,
  Icon,
  LiveExecutable,
  ProductArchive,
  ProductExecutables,
  RootManifest,
  WidgetExecutable,
  WorkerExecutable,
} from './manifest/types';
export type { SemVer } from './manifest/schemas';
