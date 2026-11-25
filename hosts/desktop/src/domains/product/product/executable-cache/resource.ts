import { of } from 'rxjs';

import { createStreamResource } from '@/shared/resource';

import { executableCacheRepository } from './repository';
import { type ExecutableCacheEntry } from './types';

export const executableCacheResource = createStreamResource({ key: () => 'executable-cache' })
  .subscribe<ExecutableCacheEntry[]>(() => executableCacheRepository.subscribeToAll())
  .mock(() => of([]))
  .cache<ExecutableCacheEntry[]>({
    initial: [],
    map(_, entries) {
      return entries;
    },
  })
  .build();
