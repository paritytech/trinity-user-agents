import * as v from 'valibot';

import { createState, persistLocalStorage } from '@/shared/rxstate';

import { MAX_RECENT_PRODUCTS, RECENT_PRODUCTS_STORAGE_KEY } from './constants';

// Persisted user history, read back from storage on every boot, so it crosses a
// trust boundary and is parsed rather than trusted.
const recentSchema = v.array(v.string());

const decode = (raw: string): string[] => {
  const parsed: unknown = JSON.parse(raw);
  const result = v.safeParse(recentSchema, parsed);

  return result.success ? result.output.slice(0, MAX_RECENT_PRODUCTS) : [];
};

const recentProductIds$ = createState<string[]>([]);

persistLocalStorage(recentProductIds$, {
  key: RECENT_PRODUCTS_STORAGE_KEY,
  sync: true,
  decode,
});

function record(productId: string) {
  recentProductIds$.set(prev => [productId, ...prev.filter(id => id !== productId)].slice(0, MAX_RECENT_PRODUCTS));
}

function forget(productId: string) {
  recentProductIds$.set(prev => prev.filter(id => id !== productId));
}

function clear() {
  recentProductIds$.set([]);
}

// Undo for a clear — the new-tab page offers a short window to take it back.
function restore(productIds: string[]) {
  recentProductIds$.set(productIds.slice(0, MAX_RECENT_PRODUCTS));
}

export const recentProductsRepository = {
  recentProductIds$,
  record,
  forget,
  clear,
  restore,
};
