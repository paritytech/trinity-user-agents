import { useEffect, useMemo } from 'react';

import { type AppListing, browseService, useDotNsTld, useProductIcon, usePublishedWidgetListings } from '@/domains/product';
import {
  type AddWidgetContributedEntry,
  AddWidgetSidebarIcon,
  clearAddWidgetCatalogSource,
  publishAddWidgetCatalogSource,
  useAddWidgetModalOpen,
} from '@/features/dashboard';

const PRODUCT_CATALOG_SOURCE_KEY = 'product:published';

// Kept separate so `useProductIcon` is unconditional: a published baseName can
// collide with a native id (e.g. 'chat'), and reconciling a published entry to
// a native one under the same key must not change the hook call count.
const PublishedSidebarItemIcon = ({ listing }: { listing: AppListing }) => {
  const { data: iconUrl } = useProductIcon(listing.manifest.icon);

  return <AddWidgetSidebarIcon alt={listing.manifest.displayName} imageUrl={iconUrl} />;
};

// Publishes the product catalog slice into the dashboard Add-Widget modal's
// content-agnostic `addWidgetCatalogSources` state. Mirrors `ProductFavoriteOpenBinding`:
// a hook-bound binding component that reads the published listings and
// publishes/clears a source slice, gated on modal-open so a closed modal does no
// chain work (preserving `usePublishedWidgetListings(isOpen)`).
export const ProductAddWidgetCatalogBinding = () => {
  const isOpen = useAddWidgetModalOpen();
  const { data: listings, pending, error } = usePublishedWidgetListings(isOpen);
  const { data: tld, pending: tldPending, error: tldError } = useDotNsTld();

  const entries = useMemo<AddWidgetContributedEntry[]>(() => {
    return listings.map(listing => {
      const baseName = browseService.listingBaseName(listing, tld);
      return {
        kind: 'product:widget',
        id: baseName,
        label: listing.manifest.displayName,
        searchText: listing.label,
        renderIcon: () => <PublishedSidebarItemIcon listing={listing} />,
        payload: { baseName, listing },
      };
    });
  }, [listings, tld]);

  useEffect(() => {
    // An unsettled or failed TLD is withheld as "no entries", not merely flagged
    // pending: the modal renders whatever a source contributes regardless of that
    // source's pending flag, and every `baseName` here is an id the dashboard
    // persists. The error is not published — one failed source blanks the whole
    // sidebar, native entries included, and the modal offers no retry.
    const tldUnsettled = tldPending || tldError !== null;

    publishAddWidgetCatalogSource(PRODUCT_CATALOG_SOURCE_KEY, {
      entries: tldUnsettled ? [] : entries,
      pending: pending || tldPending,
      error: error !== null,
    });

    return () => {
      clearAddWidgetCatalogSource(PRODUCT_CATALOG_SOURCE_KEY);
    };
  }, [entries, pending, error, tldPending, tldError]);

  return null;
};
