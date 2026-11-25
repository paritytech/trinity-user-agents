import { createState, useRxState } from '@/shared/rxstate';
import { type AddWidgetContributedEntry } from '../di';

// One content provider's slice of the Add-Widget catalog. `pending`/`error` mirror
// the read state of whatever the provider loads (e.g. published listings) so the
// modal keeps its catalog-loading spinner and load-failed message — aggregated
// across all sources.
export type AddWidgetCatalogContribution = {
  entries: AddWidgetContributedEntry[];
  pending: boolean;
  error: boolean;
};

const EMPTY_SOURCES: ReadonlyMap<string, AddWidgetCatalogContribution> = new Map();

// Contributed catalog entries keyed by a provider-chosen source key. Providers
// publish their slice from a binding component (mirrors `ProductFavoriteOpenBinding`) and
// clear it on unmount; the modal reads the merged map reactively.
export const addWidgetCatalogSources = createState<ReadonlyMap<string, AddWidgetCatalogContribution>>(EMPTY_SOURCES);

export function publishAddWidgetCatalogSource(key: string, contribution: AddWidgetCatalogContribution): void {
  addWidgetCatalogSources.set(prev => {
    const next = new Map(prev);
    next.set(key, contribution);
    return next;
  });
}

export function clearAddWidgetCatalogSource(key: string): void {
  addWidgetCatalogSources.set(prev => {
    if (!prev.has(key)) return prev;
    const next = new Map(prev);
    next.delete(key);
    return next;
  });
}

// Whether the Add-Widget modal is open. Catalog providers gate their (sometimes
// network-backed) reads on this so a closed modal does no work — preserving the
// prior `usePublishedWidgetListings(isOpen)` behavior now that the modal no longer
// owns the product hook directly.
export const addWidgetModalOpen = createState<boolean>(false);

export function useAddWidgetModalOpen(): boolean {
  const [isOpen] = useRxState(addWidgetModalOpen);
  return isOpen;
}

export function useAddWidgetCatalogSources(): ReadonlyMap<string, AddWidgetCatalogContribution> {
  const [sources] = useRxState(addWidgetCatalogSources);
  return sources;
}
