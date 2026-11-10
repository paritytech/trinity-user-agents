import { from, switchMap } from 'rxjs';

import { useAction } from '@/shared/hooks';
import { dotNsUseCase } from '../../$usecase/dotns';

import { declineUpdate } from './resource';
import { type DeclinedUpdate } from './types';

// The TLD is sourced here, from the same settled read `checkModalityUpdate` uses,
// rather than taken from the caller: a decline keyed under the render-time
// fallback would never match the lookup that suppresses the re-nag.
export const useDeclineUpdate = () =>
  useAction((entry: DeclinedUpdate) => from(dotNsUseCase.getActiveTld()).pipe(switchMap(tld => declineUpdate(entry, tld))));
