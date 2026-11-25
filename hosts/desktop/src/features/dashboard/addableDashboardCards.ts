import { type AddableDashboardCard, addableDashboardCardsPipeline } from './di';
import { openAddToDashboardDialog } from './state/addToDashboardDialog';

// Stable seed/ctx references so the React `usePipeline` memo in
// `useAddableDashboardCards` stays referentially stable across renders.
const PIPELINE_SEED: AddableDashboardCard[] = [];
const PIPELINE_CTX = {};

export const ADDABLE_DASHBOARD_CARDS_PIPELINE_SEED = PIPELINE_SEED;
export const ADDABLE_DASHBOARD_CARDS_PIPELINE_CTX = PIPELINE_CTX;

export function buildAddableDashboardCardsByGridId(
  cards: readonly AddableDashboardCard[],
): ReadonlyMap<string, AddableDashboardCard> {
  return new Map(cards.map(card => [card.gridId, card]));
}

// Non-React readers recompute the pipeline on each call. It is a handful of DI
// handlers, so the cost is negligible — and recomputing keeps this path's
// freshness identical to the live `usePipeline` reader, with no stale module
// cache if an addable were ever registered after the first read.
export function isNativeAddableDashboardId(gridId: string): boolean {
  return addableDashboardCardsPipeline(PIPELINE_SEED, PIPELINE_CTX).some(card => card.gridId === gridId);
}

// Non-React lookup of a native addable entry by grid id, for DI handler bodies
// (e.g. the Add-to-Dashboard modal transformer) that can't call hooks. Same
// fresh-read rationale as `isNativeAddableDashboardId`. Returns `null` when no
// native entry owns the id (i.e. it's a chain product target).
export function lookupAddableDashboardCardByGridId(gridId: string): AddableDashboardCard | null {
  return addableDashboardCardsPipeline(PIPELINE_SEED, PIPELINE_CTX).find(card => card.gridId === gridId) ?? null;
}

// Cmd/Ctrl+D and the address-bar AtD button for native subjects (e.g. chat) —
// not for product menu items, which must keep the resolve → favorites-toggle
// path. Generic (native/addable) dashboard concern: it opens the host's own
// Add-to-Dashboard dialog for a native grid id and reports whether it claimed
// the id, so a product caller can fall back to the product path.
export function openNativeAddToDashboardDialog(productId: string): boolean {
  if (!isNativeAddableDashboardId(productId)) return false;
  openAddToDashboardDialog(productId);
  return true;
}
