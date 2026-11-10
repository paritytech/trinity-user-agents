import { fromHex } from 'polkadot-api/utils';

import { createQueryResource } from '@/shared/resource';
import { chainRegistry } from '../api/registry';
import { type Chain } from '../chain/types';

import { type DecodedCall } from './types';

/**
 * SCALE call bytes turned into a pallet, a method, and arguments.
 *
 * Only the chain's own metadata can do this, which is why it is a chain read and not
 * a decode helper: the same bytes name different calls on different runtimes.
 *
 * Keyed by genesis hash and call data together — the pair is what identifies the
 * result, and both are needed because either one alone can change the answer. The
 * chain object itself is a parameter rather than something this resource resolves:
 * the chains map comes from another resource, and a resource never reads one.
 *
 * Cached forever: a runtime upgrade can rename a call, but the desktop reconnects
 * (and re-reads metadata) far more often than a user re-opens the same review, and a
 * stale entry here would only ever mislabel a call the user already approved once.
 */
export const decodedCallResource = createQueryResource<{ chain: Chain; callData: string }>({
  key: ({ chain, callData }) => `${chain.genesisHash}:${callData}`,
})
  .request<DecodedCall>(({ chain, callData }) =>
    chainRegistry
      .requestApi(chain, api => api.api.txFromCallData(fromHex(callData)))
      .then((tx): DecodedCall => ({
        pallet: tx.decodedCall.type,
        method: tx.decodedCall.value.type,
        args: tx.decodedCall.value.value,
      })),
  )
  .mock(() => ({ pallet: 'System', method: 'remark', args: {} }))
  .cache<Record<string, DecodedCall>>({
    staleAfter: Number.POSITIVE_INFINITY,
    initial: {},
    map: (cache, decoded, { chain, callData }) => ({ ...cache, [`${chain.genesisHash}:${callData}`]: decoded }),
  })
  .build();
