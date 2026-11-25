import { useRead } from '@/shared/hooks';
import { accountService } from '../account/service';
import { type AccountId } from '../account/types';

import { consumerIdentityResource } from './resource';
import { type ConsumerIdentity } from './types';

// Typed so `useRead` infers `ConsumerIdentity | null` from the default rather than `null`.
const NO_IDENTITY: ConsumerIdentity | null = null;

/**
 * One account's People-chain identity, or `null` while it loads and when the account
 * has no record. Idle until an account id is given.
 *
 * The React read of the same source the P2P peer resolver reads imperatively through
 * `consumerIdentityUseCase.getIdentity` — so the signed-in user's name comes from the
 * exact place a peer's does.
 *
 * Converted to SS58 first because `Resources.Consumers` is keyed by that form; a hex
 * `accountId` fails the encode and reads back empty.
 */
export const useConsumerIdentity = (accountId: Nullable<AccountId>) => {
  const address = accountId ? accountService.toAddress(accountId) : null;
  const ss58Address = address?.type === 'ss58' ? address.value : null;

  return useRead(consumerIdentityResource, {
    params: ss58Address ? { accountId: ss58Address } : null,
    defaultValue: NO_IDENTITY,
    map: (cache, { accountId: id }) => cache[id] ?? null,
  });
};
