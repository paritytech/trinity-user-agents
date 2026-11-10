import { dotNsService } from '../../dotns/service';

import { declinedUpdatesRepository } from './repository';
import { type DeclinedUpdate } from './types';

// Record that the user declined a specific modality version. Single-source write
// (no invariant) → a plain mutation, bound via `useAction`, not a use case. The
// base name is normalized so the recorded key matches the normalized `isDeclined`
// read in `checkModalityUpdate` — dismissals arrive keyed by the raw open id.
export function declineUpdate(entry: DeclinedUpdate, tld: string): Promise<void> {
  return declinedUpdatesRepository.record({ ...entry, baseName: dotNsService.baseNameOf(entry.baseName, tld) });
}
