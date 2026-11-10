import { type Observable } from 'rxjs';

import { themeResource } from '../theme/resource';
import { type Theme } from '../theme/types';

/**
 * The live theme for callers that can't use a hook — the TrUAPI theme callback is a
 * generator the core drives. Reads through the resource so it shares the single cached,
 * `shareReplay`-deduped subscription with the React consumers.
 */
function watchTheme(): Observable<Theme> {
  return themeResource.read$();
}

export const themeUseCase = {
  watchTheme,
};
