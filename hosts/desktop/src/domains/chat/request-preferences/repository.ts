import * as v from 'valibot';

import { createState, persistLocalStorage } from '@/shared/rxstate';

import { HIDE_REQUESTS_BY_DEFAULT, HIDE_REQUESTS_BY_DEFAULT_STORAGE_KEY } from './constants';

const hideRequestsByDefault$ = createState<boolean>(HIDE_REQUESTS_BY_DEFAULT);

persistLocalStorage(hideRequestsByDefault$, {
  key: HIDE_REQUESTS_BY_DEFAULT_STORAGE_KEY,
  // Persisted blob read back is a trust boundary — validate it rather than
  // trust JSON.parse; fall back to the default on any malformed value.
  decode: raw => v.parse(v.fallback(v.boolean(), HIDE_REQUESTS_BY_DEFAULT), JSON.parse(raw)),
});

export const requestPreferencesRepository = {
  hideRequestsByDefault$,
  setHideRequestsByDefault(value: boolean) {
    return hideRequestsByDefault$.set(value);
  },
};
