import { type AppListing } from '@parity/browse-sdk';

import { dotNsService } from '../dotns/service';
import { manifestService } from '../product/manifest/service';
import { type RootManifest } from '../product/manifest/types';
import { type Product } from '../product/types';

function listingBaseName(listing: AppListing, tld: string): string {
  return dotNsService.baseNameOf(listing.label, tld);
}

function listingToRootManifest(manifest: AppListing['manifest']): RootManifest {
  return {
    $v: 1,
    displayName: manifest.displayName,
    description: manifest.description,
    icon: manifest.icon,
  };
}

/**
 * Preview product for the add-widget modal before the user commits from chain.
 *
 * Takes the base name rather than the network TLD it is derived from:
 * `listingBaseName` is the one place that names a listing, and a caller that
 * already holds the name (an entry the catalog published) must not have it
 * re-derived under a different suffix.
 */
function productPreviewFromListing(listing: AppListing, baseName: string): Product {
  return manifestService.assembleProduct({
    baseName,
    root: listingToRootManifest(listing.manifest),
    executables: {},
  });
}

function findListingByBaseName(listings: AppListing[], baseName: string, tld: string): AppListing | undefined {
  return listings.find(listing => listingBaseName(listing, tld) === baseName);
}

/** Prefer browse catalog manifest fields when the stored product is missing them. */
function enrichProductWithListing(product: Product, listing: AppListing | undefined): Product {
  if (!listing) return product;

  const fromListing = productPreviewFromListing(listing, product.baseName);
  const description = fromListing.description.trim() || product.description;

  return {
    ...product,
    displayName: fromListing.displayName || product.displayName,
    description,
    icon: product.icon.cid ? product.icon : fromListing.icon,
  };
}

export const browseService = {
  listingBaseName,
  findListingByBaseName,
  enrichProductWithListing,
  productPreviewFromListing,
};
