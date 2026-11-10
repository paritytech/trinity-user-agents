import {
  type ExecutableKind,
  type SemVer,
  EXECUTABLE_KINDS,
  productService,
  useLiveExecutable,
  usePersistedProductById,
} from '@/domains/product';

type AvailableUpdate = { kind: ExecutableKind; fromVersion: SemVer; toVersion: SemVer };

// The drifted executables of a pinned product — one entry per present kind whose
// on-chain contenthash differs from the frozen one, carrying the frozen (pinned)
// version and the fresh on-chain version for a from→to display. Empty for an
// unpinned product (no version-check when not pinned).
export const useAvailableUpdates = (productId: string): AvailableUpdate[] => {
  const { data: record } = usePersistedProductById(productId);
  const pinned = record?.pinned === true;

  // Null skips the live read for an absent kind or an unpinned product; the three
  // hook calls stay unconditional (rules of hooks).
  const paramFor = (kind: ExecutableKind) => (pinned && record && record.executables[kind] ? { product: record, kind } : null);

  const { data: appLive } = useLiveExecutable(paramFor('app'));
  const { data: widgetLive } = useLiveExecutable(paramFor('widget'));
  const { data: workerLive } = useLiveExecutable(paramFor('worker'));

  if (!pinned || !record) return [];

  const live = { app: appLive, widget: widgetLive, worker: workerLive };

  return EXECUTABLE_KINDS.flatMap(kind => {
    const frozen = record.executables[kind];
    const fresh = live[kind];
    return frozen && productService.hasExecutableDrift(frozen, fresh)
      ? [{ kind, fromVersion: frozen.appVersion, toVersion: fresh.version }]
      : [];
  });
};

// Boolean gate kept for the whole-product "Update Version" menu item.
export const useNewerVersionAvailable = (productId: string): boolean => useAvailableUpdates(productId).length > 0;
