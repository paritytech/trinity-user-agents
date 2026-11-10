import { useRead } from '@/shared/hooks';

import { deviceSyncStatusResource } from './resource';
import { type DeviceSyncStatus } from './types';

const DEFAULT_STATUS: DeviceSyncStatus = 'inactive';

export const useDeviceSyncStatus = () => {
  return useRead(deviceSyncStatusResource, {
    params: {},
    defaultValue: DEFAULT_STATUS,
  });
};
