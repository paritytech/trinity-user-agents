import { type AppListing } from '@parity/browse-sdk';

import { dependentRead, useRead } from '@/shared/hooks';
import { useActiveEnvironment } from '@/domains/application';

import { publishedAppListingsResource, publishedWidgetListingsResource } from './resource';

const EMPTY_LISTINGS: AppListing[] = [];

export const usePublishedWidgetListings = (enabled: boolean) => {
  // Env resolution lives in the React binding, not the resource/gateway leaf.
  const environment = useActiveEnvironment();
  const env = environment.data;
  const params = enabled && env ? { environmentId: env.id, genesisHash: env.dotnsChain.genesisHash } : null;

  const result = useRead(publishedWidgetListingsResource, {
    params,
    defaultValue: EMPTY_LISTINGS,
  });

  return dependentRead(result, environment, { enabled });
};

// Fullscreen-SPA catalog for the Add-to-Favorites picker — the `app`-modality
// sibling of `usePublishedWidgetListings`.
export const usePublishedAppListings = (enabled: boolean) => {
  const environment = useActiveEnvironment();
  const env = environment.data;
  const params = enabled && env ? { environmentId: env.id, genesisHash: env.dotnsChain.genesisHash } : null;

  const result = useRead(publishedAppListingsResource, {
    params,
    defaultValue: EMPTY_LISTINGS,
  });

  return dependentRead(result, environment, { enabled });
};
