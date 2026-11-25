import { EXECUTABLE_KINDS } from './manifest/constants';
import { type Executable, type LiveExecutable } from './manifest/types';
import { type Product } from './types';

// Stateless predicates on a loaded Product. Display-name / label formatting is
// a dotNS-name string transform and lives in `dotNsService` (toDisplayName /
// toShortLabel), not here.
function hasWidget(product: Product): boolean {
  return product.executables.widget !== undefined;
}

function hasApp(product: Product): boolean {
  return product.executables.app !== undefined;
}

// The product-search rule shared by every list that filters products by user
// input (browser new-tab search, settings apps list): substring match on the
// display name or the `.dot` address, case-insensitive.
function matchesQuery(product: Product, query: string): boolean {
  const q = query.toLowerCase();
  return product.displayName.toLowerCase().includes(q) || product.baseName.toLowerCase().includes(q);
}

// Every identifier a refresh request targets for a product: its own id plus the
// dotNS identifier of each present executable. The producer (clear-cache) fans
// out to each; the consumer (widget body) checks membership — both derive the
// set here rather than restating the loop.
function refreshTargetIdentifiers(productId: string, product: Nullable<Product>): Set<string> {
  const executableIds = product
    ? EXECUTABLE_KINDS.map(kind => product.executables[kind]?.identifier).filter((id): id is string => id !== undefined)
    : [];
  return new Set([productId, ...executableIds]);
}

// A frozen executable has drifted when its live on-chain resolution exists and
// carries a different contenthash — i.e. a newer deployment is available for that
// kind. A null `live` (kind absent on chain, or not yet resolved) is never drift.
// Type guard: on `true` the live resolution is present, so callers can read its
// fresh version without re-checking for null.
function hasExecutableDrift(frozen: Executable, live: Nullable<LiveExecutable>): live is LiveExecutable {
  return live != null && live.contenthash !== frozen.contenthash;
}

export const productService = {
  hasWidget,
  hasApp,
  matchesQuery,
  refreshTargetIdentifiers,
  hasExecutableDrift,
};
