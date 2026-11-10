import { useRead } from '@/shared/hooks';

import { recentProductIdsResource } from './resource';

const EMPTY: string[] = [];

export const useRecentProductIds = () => {
  return useRead(recentProductIdsResource, {
    params: {},
    defaultValue: EMPTY,
  });
};
