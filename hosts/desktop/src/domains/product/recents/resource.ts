import { of } from 'rxjs';

import { createStreamResource } from '@/shared/resource';

import { recentProductsRepository } from './repository';

const EMPTY: string[] = [];

/** Ids of the products the user most recently opened, most recent first. */
export const recentProductIdsResource = createStreamResource<object>({
  key: () => 'all',
})
  .subscribe<string[]>(() => recentProductsRepository.recentProductIds$.value$)
  .mock(() => of(EMPTY))
  .cache<string[]>({
    initial: EMPTY,
    map: (_, value) => value,
  })
  .build();

export function recordRecentProduct(productId: string) {
  recentProductsRepository.record(productId);
}

export function forgetRecentProduct(productId: string) {
  recentProductsRepository.forget(productId);
}

export function clearRecentProducts() {
  recentProductsRepository.clear();
}

export function restoreRecentProducts(productIds: string[]) {
  recentProductsRepository.restore(productIds);
}
