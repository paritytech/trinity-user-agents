import { createQueryResource } from '@/shared/resource';
import { type Environment } from '@/domains/application';

import { DEFAULT_DOTNS_TLD } from './constants';
import { dotNsGateway } from './gateway';

type Params = { environment: Environment };

// Keyed by what actually determines the value: the chain the registry lives on
// and the registry address. Two channels pointing at the same deployment share
// one entry; a channel repointed at another deployment gets a new one.
function tldCacheKey({ environment }: Params): string {
  return `${environment.dotnsChain.chainId}:${environment.dotnsRegistryContract}`;
}

/**
 * Reads the network's dotNS TLD. A network that reports none — every deployment
 * predating paritytech/dotns#201 — resolves to `DEFAULT_DOTNS_TLD` rather than
 * failing, so the app stays usable there.
 */
async function readTldWithFallback({ environment }: Params): Promise<string> {
  const tld = await dotNsGateway.readTld(environment);
  if (!tld) {
    console.warn(`[dotns] "${environment.id}" reported no TLD; falling back to ${DEFAULT_DOTNS_TLD}`);
  }

  return tld ?? DEFAULT_DOTNS_TLD;
}

/** The active network's dotNS TLD, cached per deployment. */
export const dotNsTldResource = createQueryResource<Params>({
  key: tldCacheKey,
})
  .request<string>(readTldWithFallback)
  .mock(() => DEFAULT_DOTNS_TLD)
  // A hang here stalls every surface gated on the suffix, input routing included.
  // The bound is generous because timing out yields the *fallback*, which is wrong
  // rather than merely late on any other network — it exists to stop a hang, not to
  // shorten a slow read. Retries cover the other failure mode: consumers gate on
  // `error`, and a `useRead` only restarts on a key change or an explicit refresh,
  // so an unretried transient failure would be sticky for the whole session. The
  // timeout budget is shared across attempts, and the delay is short because it is
  // pure latency in front of the input-routing fallback.
  .timeout(30_000)
  .retry({ count: 2, delay: 300 })
  .cache<Record<string, string>>({
    initial: {},
    // The TLD is fixed at contract initialisation, so an entry never goes stale
    // for as long as the deployment it is keyed to.
    staleAfter: Number.POSITIVE_INFINITY,
    map: (cache, tld, params) => ({ ...cache, [tldCacheKey(params)]: tld }),
  })
  .build();
