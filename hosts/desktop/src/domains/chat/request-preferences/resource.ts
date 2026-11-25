import { type Observable, map, of, take } from 'rxjs';

import { createStreamResource } from '@/shared/resource';

import { HIDE_REQUESTS_BY_DEFAULT } from './constants';
import { requestPreferencesRepository } from './repository';

export const hideRequestsByDefaultResource = createStreamResource<object>({
  key: () => 'all',
})
  .subscribe<boolean>(() => requestPreferencesRepository.hideRequestsByDefault$.value$)
  .mock(() => of(HIDE_REQUESTS_BY_DEFAULT))
  .cache<boolean>({
    initial: HIDE_REQUESTS_BY_DEFAULT,
    map: (_, value) => value,
  })
  .build();

export function setHideRequestsByDefault({ value }: { value: boolean }): Observable<null> {
  return requestPreferencesRepository.setHideRequestsByDefault(value).pipe(
    map(() => null),
    take(1),
  );
}
