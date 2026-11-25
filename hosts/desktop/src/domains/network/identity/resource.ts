import { produce } from 'immer';

import { createQueryResource } from '@/shared/resource';
import { lazyClient } from '@/domains/application';

import { consumerIdentityGateway } from './gateway';
import { type ConsumerIdentity } from './types';

type Params = { accountId: string };

/**
 * One account's People-chain identity.
 *
 * Cached because it is read on every peer render — a chat list resolves a username
 * per row — while the record itself changes only when the user edits their identity
 * on chain. `staleAfter` is generous for that reason, not because staleness is
 * harmless: a renamed account shows its old name until the entry lapses.
 *
 * A missing pallet and an account with no record both resolve to `null`. They are
 * different facts, but neither gives the caller a name to show, and the gateway
 * already logs the difference where it can act on it.
 */
export const consumerIdentityResource = createQueryResource<Params>({
  key: ({ accountId }) => accountId,
})
  .request<ConsumerIdentity | null>(async ({ accountId }) => {
    const identities = await consumerIdentityGateway.readConsumers(lazyClient.getClient().getUnsafeApi(), [accountId]);

    return identities?.[accountId] ?? null;
  })
  .mock(() => null)
  .timeout(15_000)
  .retry({ count: 2, delay: 300 })
  .cache<Record<string, ConsumerIdentity | null>>({
    initial: {},
    staleAfter: 5 * 60 * 1000,
    map(cache, identity, { accountId }) {
      return produce(cache, draft => {
        draft[accountId] = identity;
      });
    },
  })
  .build();
