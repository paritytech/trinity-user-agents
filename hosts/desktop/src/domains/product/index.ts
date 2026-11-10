// Product container — canonical entity, persistence, and the nested manifest
// sub-module (product manifest wire schemas + per-executable archive loader). Top-
// level product discovery lives in `$usecase/resolve.ts`, not here.
export type {
  AppExecutable,
  Executable,
  ExecutableContent,
  ExecutableKind,
  Icon,
  LiveExecutable,
  PersistedProduct,
  Product,
  ProductArchive,
  ProductExecutables,
  RootManifest,
  SemVer,
  WidgetExecutable,
  WorkerExecutable,
} from './product';
export type { ExecutableCacheStatus } from './product/executable-cache/types';
// Pure derivation over the core's storage keys — a service, not a use case.
export { coreSlotService } from './core-storage/service';
export {
  EXECUTABLE_KINDS,
  executableArchiveResource,
  manifestService,
  productDb,
  productService,
  productsResource,
  useDeclineUpdate,
  useDisplayedProduct,
  useExecutableArchive,
  useIsPinned,
  useIsProductInstalled,
  useLiveExecutable,
  usePersistedProductById,
  usePersistedProducts,
  useProductIcon,
} from './product';

// Host-environment wiring — call once from the app bootstrap (never at import time).
export { bootstrapProduct } from './bootstrap';

// Use cases — imperative product flows (multi-step writes / cross-module read
// composition). Each group is exported separately per project structure.
export { coreStorageUseCase } from './$usecase/coreStorage';
export { lifecycleUseCase } from './$usecase/lifecycle';
export { offlineCacheUseCase } from './$usecase/offlineCache';
export { productStorageUseCase } from './$usecase/productStorage';
export { remoteAccessUseCase } from './$usecase/remoteAccess';
export { useRendererImage } from './$usecase/renderer.hooks';
export { resolveProductUseCase } from './$usecase/resolve';
export { useCommitProductByIdentifier, usePinExecutable, usePinProduct, useUnpinProduct } from './$usecase/commitment.hooks';
export { useInteractedProducts } from './$usecase/interaction.hooks';
export { useOfflineCacheSize, useOfflineCacheStatus } from './product/executable-cache/hooks';
export { commitmentUseCase } from './$usecase/commitment';
export { onProductModalityOpenedSideEffect, updatesUseCase } from './$usecase/updates';

export type { AppListing } from '@parity/browse-sdk';
export { browseService } from './browse/service';
export { usePublishedAppListings, usePublishedWidgetListings } from './browse/hooks';

export { dotNsService } from './dotns/service';
export { isLocalhostUrl, normalizeLocalhostUrl } from './dotns/service';
export { DEFAULT_DOTNS_TLD } from './dotns/constants';
export { useDotNsLabels, useDotNsTld, useIsProductIdentifier } from './dotns/hooks';
export { dotNsUseCase } from './$usecase/dotns';
export type { DotNsUrl } from './dotns/types';

export type { FetchResolver, ProductWorkerInstance, Sandbox } from './worker/types';
export { createProductWorker } from './worker/instance';

export type {
  AggregatedPermission,
  AppPermissionEntry,
  DevicePermissionType,
  OsDevicePermissionStatus,
  PermissionStatus,
  RemotePermissionIpcRequest,
} from './permissions/types';
export { type PermissionId, PERMISSION_IDS } from './permissions/constants';
export { clearTransientDevicePermissionGrants, grantTransientDevicePermission } from './permissions/resource';
export { permissionsService } from './permissions/service';
export { _resetRemotePermissionBroker, pendingRemotePermissionRequests$, requestExternalUrlAccess } from './permissions/broker';
export type { PendingRemotePermissionRequest } from './permissions/broker';

// Recently opened products — persisted visit history, read back on every boot.
export { MAX_RECENT_PRODUCTS } from './recents/constants';
export {
  clearRecentProducts,
  forgetRecentProduct,
  recentProductIdsResource,
  recordRecentProduct,
  restoreRecentProducts,
} from './recents/resource';
export { useRecentProductIds } from './recents/hooks';
export type { CoreSlotDescriptor } from './core-storage/types';
export { onDeviceOsPermissionBlockedSideEffect, permissionsUseCase } from './$usecase/permissions';
export {
  useSetPermissionStatus,
  useWatchAggregatedPermissions,
  useWatchGrantedAccountAccess,
  useWatchGrantedPatterns,
  useWatchProductPermissions,
} from './$usecase/permissions.hooks';
export type { GrantedAccountAccess, GrantedPattern, PermissionsAdapter, ProductPermissionEntry } from './permissions/types';
export { publishedAppListingsResource, publishedWidgetListingsResource } from './browse/resource';
export { dotNsTldResource } from './dotns/resource';
export { executableCacheResource } from './product/executable-cache/resource';
export { liveExecutableResource } from './product/manifest/resource';
export { chainResolveResource } from './product/resource';
