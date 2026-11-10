import { type Observable, map, take } from 'rxjs';

import { createStreamResource } from '@/shared/resource';
import { type GenesisHash } from '../chain/types';

import { connectionRepository } from './repository';
import { type ConnectionMode, type ConnectionPreference, type ConnectionSettings } from './types';

export const INITIAL_CONNECTION_SETTINGS: ConnectionSettings = connectionRepository.initialSettings;

// No `.mock()`: the subscription reads an in-memory `RxState`, no external I/O,
// and a mock would hide the preference each spec sets.
export const connectionSettingsResource = createStreamResource<object>({
  key: () => 'all',
})
  .subscribe<ConnectionSettings>(() => connectionRepository.settings$.value$)
  .cache<ConnectionSettings>({
    initial: INITIAL_CONNECTION_SETTINGS,
    map: (_, value) => value,
  })
  .build();

export function setConnectionPreference({ preference }: { preference: ConnectionPreference }): Observable<null> {
  return connectionRepository.setPreference(preference).pipe(
    map(() => null),
    take(1),
  );
}

export function setChainConnectionMode({
  chainId,
  mode,
}: {
  chainId: GenesisHash;
  mode: ConnectionMode | null;
}): Observable<null> {
  return connectionRepository.setChainMode(chainId, mode).pipe(
    map(() => null),
    take(1),
  );
}
