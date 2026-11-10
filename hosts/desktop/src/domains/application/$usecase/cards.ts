import { dashboardLayoutDb } from '../dashboard-layout/repository';
import { dashboardLayoutService } from '../dashboard-layout/service';
import { type DashboardCard } from '../dashboard-layout/types';

// First-run seed: persist the default dashboard only when none exists yet.
// Returns whether it actually seeded, so a caller can pair it with companion
// first-run setup (e.g. committing the default product). Idempotent — a no-op
// once any dashboard page is present.
async function seedDefaultMainLayout(productId: string): Promise<boolean> {
  const main = await dashboardLayoutDb.getMain();
  if (main.isErr()) return false;
  if (dashboardLayoutService.hasPages(main.value?.pages)) return false;

  const saveResult = await dashboardLayoutDb.saveMainPages(dashboardLayoutService.defaultPages(productId), 0);
  return saveResult.isOk();
}

// Places a single top-level card: load → reject a duplicate of this id → sweep a
// legacy same-id card (a favourites entry of the same id survives, being a
// folder) → place → persist. The card's content payload is opaque — this path
// is identical for every kind. The favourites-folder add path (folders.ts) is
// deliberately NOT routed through here — it mutates folder items rather than
// placing a card.
async function addCardToLayout(card: DashboardCard): Promise<{ ok: boolean; pageIndex?: number }> {
  const main = await dashboardLayoutDb.getMain();
  if (main.isErr()) return { ok: false };
  const sourcePages = dashboardLayoutService.ensurePages(main.value?.pages ?? null);

  if (dashboardLayoutService.hasCardOnPages(sourcePages, card.i)) return { ok: false };

  const stripped = dashboardLayoutService.stripLegacyTopLevelCardFromPages(sourcePages, card.i);
  const preferred = main.value?.activePageIndex ?? 0;
  const { pages: nextPages, pageIndex } = dashboardLayoutService.placeOnPages(stripped, card, preferred);
  const saveResult = await dashboardLayoutDb.saveMainPages(nextPages, pageIndex);
  return { ok: saveResult.isOk(), pageIndex: saveResult.isOk() ? pageIndex : undefined };
}

async function removeCardFromLayout(cardId: string): Promise<boolean> {
  const main = await dashboardLayoutDb.getMain();
  if (main.isErr() || !main.value) return false;

  const next = dashboardLayoutService.removeCardFromPages(main.value.pages, main.value.activePageIndex, cardId);
  if (!next.changed) return false;

  const saveResult = await dashboardLayoutDb.saveMainPages(next.pages, next.activePageIndex);
  return saveResult.isOk();
}

async function resizeCardToGridSize(
  cardId: string,
  size: { w: number; h: number },
): Promise<{ ok: boolean; pageIndex?: number }> {
  const result = await dashboardLayoutDb.getMainPages();
  if (result.isErr() || !result.value) return { ok: false };

  const applied = dashboardLayoutService.applyCardResize(result.value, cardId, size);
  if (!applied) return { ok: false };
  if (!applied.changed) return { ok: true, pageIndex: applied.pageIndex };

  const saveResult = await dashboardLayoutDb.saveMainPages(applied.pages, applied.pageIndex);
  return { ok: saveResult.isOk(), pageIndex: saveResult.isOk() ? applied.pageIndex : undefined };
}

export const cardsUseCase = {
  addCardToLayout,
  removeCardFromLayout,
  resizeCardToGridSize,
  seedDefaultMainLayout,
};
