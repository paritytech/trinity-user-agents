import { firstValueFrom } from 'rxjs';

import { consumerIdentityResource } from '../identity/resource';
import { type ConsumerIdentity } from '../identity/types';

/**
 * One account's People-chain identity, or `null` when it has none.
 *
 * Thin by design, and the domain's only public read: `resource.ts` may not sit on
 * the barrel (`local-rules/enforce-import-restrictions`), and callers outside React
 * — the P2P peer resolver in particular — need the value imperatively rather than
 * through `useRead`.
 */
function getIdentity(accountId: string): Promise<ConsumerIdentity | null> {
  return firstValueFrom(consumerIdentityResource.read$({ accountId }));
}

export const consumerIdentityUseCase = {
  getIdentity,
};
