import { firstValueFrom } from 'rxjs';

import { environmentUseCase } from '@/domains/application';
import { dotNsTldResource } from '../dotns/resource';

/**
 * The active network's dotNS TLD for non-React callers — the settled value, so a
 * caller that must not decide under a guess awaits this instead of reading
 * `useDotNsTld().data`. Rejects when the read fails; it never falls back.
 *
 * Composes the active environment with the TLD read, which is what makes it a use
 * case rather than something the resource could do on its own.
 */
async function getActiveTld(): Promise<string> {
  const environment = await environmentUseCase.getActive();

  return firstValueFrom(dotNsTldResource.read$({ environment }));
}

export const dotNsUseCase = { getActiveTld };
