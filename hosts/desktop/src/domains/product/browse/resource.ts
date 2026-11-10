import { type AppListing } from '@parity/browse-sdk';

import { createQueryResource } from '@/shared/resource';

import { browseGateway } from './gateway';

type PublishedListingsParams = {
  environmentId: string;
  genesisHash: string;
};

export const publishedWidgetListingsResource = createQueryResource<PublishedListingsParams>({
  key: ({ environmentId }) => `published-widgets:${environmentId}`,
})
  .request<AppListing[]>(({ genesisHash }) => browseGateway.listPublishedWidgets(genesisHash))
  .mock(() => [])
  .timeout(30_000)
  .cache<AppListing[]>({
    initial: [],
    map: (_cache, response) => response,
    staleAfter: 60_000,
  })
  .build();

export const publishedAppListingsResource = createQueryResource<PublishedListingsParams>({
  key: ({ environmentId }) => `published-apps:${environmentId}`,
})
  .request<AppListing[]>(({ genesisHash }) => browseGateway.listPublishedApps(genesisHash))
  .mock(() => [])
  .timeout(30_000)
  .cache<AppListing[]>({
    initial: [],
    map: (_cache, response) => response,
    staleAfter: 60_000,
  })
  .build();
