import { pathnameMatchesSegment } from '@/shared/utils';
import { type RankedCandidate } from '@/domains/input-routing';
import { type PersistedProduct, productService } from '@/domains/product';

import { type CandidateGroup, type DraftAttachment } from './types';

/** How many recents the suggestion list shows before the saved section takes over. */
const VISIBLE_RECENTS = 6;

type ContextInput = {
  pathname: string;
  /** The product whose app view is on screen, or null if the screen shows none. */
  screenProductId: string | null;
  dashboardProductIds: string[];
  roomProductIds: string[];
};

// Whether the chip shows a thumbnail rather than a kind icon. A presentation
// decision, so it stays in the feature — the domain owns the category, not how
// it is drawn.
function hasPreview(attachment: DraftAttachment): boolean {
  const { category } = attachment.routed;

  return (category === 'image' || category === 'video') && Boolean(attachment.previewUrl);
}

/**
 * The dashboard's contribution to the context set.
 *
 * A favourites folder's items are a bare string list mixing product base names
 * with native surface ids (`chat`), and the dashboard domain cannot tell them
 * apart. Intersecting with the installed products is what the row actually means
 * — placed on the dashboard AND a product — and a native id was never installed.
 */
function installedAmong(placedIds: string[], installedBaseNames: string[]): string[] {
  const names = new Set(installedBaseNames);

  return placedIds.filter(id => names.has(id));
}

/** The distinct products the user holds a room with. */
function distinctProductIds(rooms: { productId: string }[]): string[] {
  return [...new Set(rooms.map(room => room.productId))];
}

/**
 * The products the screen underneath contributes — RFC-0027 § The context set.
 *
 * This is a floor with no ceiling above it: this host never queries a product
 * outside the set, so what this returns is exactly what learns the input. An
 * empty result means no product is asked at all, which is why an unrecognized
 * route falls through to `[]` rather than to anything installed.
 *
 * Takes a resolved product id rather than a tab: reading the tab is the hook's
 * job, which keeps this file free of the aggregate's import graph.
 */
function contextProductIds({ pathname, screenProductId, dashboardProductIds, roomProductIds }: ContextInput): string[] {
  if (pathnameMatchesSegment(pathname, '/product')) {
    return screenProductId ? [screenProductId] : [];
  }

  if (pathnameMatchesSegment(pathname, '/chat')) return roomProductIds;
  if (pathnameMatchesSegment(pathname, '/dashboard')) return dashboardProductIds;

  return [];
}

/**
 * The host's own answers to what the user typed: products they have opened
 * before, and products they have saved.
 *
 * Navigation suggestions, not RFC-0027 candidates — the host owns them outright
 * and no product is asked to produce them.
 *
 * **Both sections narrow as the user types.** A real query ("send 5 DOT to
 * alice") matches no product name, so the suggestions empty out and the
 * candidates below have the surface to themselves. Showing a static list under
 * every query put host rows that answer nothing above the rows that do.
 */
function suggestProducts(query: string, allProducts: PersistedProduct[], recentIds: string[]) {
  const matching = query ? allProducts.filter(product => productService.matchesQuery(product, query)) : allProducts;
  const byBaseName = new Map(matching.map(product => [product.baseName, product]));

  const recentProducts = recentIds
    .map(id => byBaseName.get(id))
    .filter((product): product is PersistedProduct => product !== undefined)
    .slice(0, VISIBLE_RECENTS);

  // A product already shown as recent is not repeated below — one row per
  // product, so an arrow-key press moves to a different product every time.
  const shown = new Set(recentProducts.map(product => product.baseName));
  const installed = matching.filter(product => !shown.has(product.baseName));

  return { recentProducts, installed, allItems: [...recentProducts, ...installed] };
}

/**
 * Collapses the merged list into one group per answering product.
 *
 * Ranking interleaves products round-robin, so a product with three answers
 * appears three times in three different places — its identity repeated, its
 * answers scattered among strangers'. Grouping restores "these came from the same
 * product" without disturbing the ranking: groups appear in the order their
 * products first placed, and each keeps its own answers in rank order. So the
 * top-ranked candidate is still in the first group, first position.
 */
function groupByProduct(candidates: RankedCandidate[]): CandidateGroup[] {
  const groups = new Map<string, CandidateGroup>();

  for (const candidate of candidates) {
    const group = groups.get(candidate.productId);
    if (group) {
      group.candidates.push(candidate);
    } else {
      groups.set(candidate.productId, { productId: candidate.productId, candidates: [candidate] });
    }
  }

  return [...groups.values()];
}

/**
 * The network TLD the field will complete to if the user presses Tab, or empty when
 * the input already names something addressable.
 */
function ghostSuffix(input: string, tld: string): string {
  const trimmed = input.trim();
  if (!trimmed) return '';
  if (/^https?:\/\//i.test(trimmed)) return '';
  if (/^(localhost|127\.0\.0\.1)([:/]|$)/i.test(trimmed)) return '';
  // The completion is drawn — and committed — at the end of the text, but the
  // suffix belongs to the host, so `browse/apps` would complete to
  // `browse/apps${tld}`, which names nothing. No offer rather than a wrong one.
  if (/[/?#]/.test(trimmed)) return '';
  // The leading dot stays escaped: an unescaped `.` would match any character,
  // so `xpaseo` would count as already suffixed. The rest of `tld` is a single
  // DNS label (validated at the chain boundary), so it needs no escaping.
  if (new RegExp(`\\${tld}(\\.li)?([/?#]|$)`, 'i').test(trimmed)) return '';

  return tld;
}

export const inputModalityService = {
  hasPreview,
  installedAmong,
  suggestProducts,
  groupByProduct,
  ghostSuffix,
  distinctProductIds,
  contextProductIds,
};
