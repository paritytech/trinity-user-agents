import { firstValueFrom } from 'rxjs';
import * as v from 'valibot';

import { createQueryResource } from '@/shared/resource';
import { REMOTE_CONFIG_KEYS, remoteConfigGateway, remoteConfigReady } from '@/domains/remote-config';

import { chainRepository } from './repository';
import { remoteChainsSchema } from './schemas';
import { chainService } from './service';
import { type Chain } from './types';

// Single chain catalog from Remote Config (`chains_v2`), with the last successful
// payload kept on disk. When RC is unreachable the catalog is the first thing that
// fails, and everything downstream — the environment assembly included — never
// runs, so recovering it here is what lets the app boot at all.
//
// Still THROWS when neither source has anything, so the empty result is never
// cached under `staleAfter: Infinity` — a rejected request retries on the next read.
//
// No `.mock()`: the request performs no external I/O. Remote Config is already
// doubled at the gateway in `vitest.setup.js`, the last-known-good store is that
// file's in-memory `localStorage` shim, and the rest is a pure transform. An
// empty-catalog mock would also strand every spec that assembles an Environment,
// which resolves its role chains out of this catalog.
export const chainResource = createQueryResource<object>({
  key: () => 'chains',
})
  .request<Chain[]>(async () => {
    await remoteConfigReady;
    const raw = remoteConfigGateway.tryGetJson(REMOTE_CONFIG_KEYS.chains, remoteChainsSchema);
    if (raw) {
      chainRepository.persistLastKnownChains(raw);

      return chainService.sortChains(chainService.fromRemoteChains(raw));
    }

    const restored = v.safeParse(remoteChainsSchema, chainRepository.readLastKnownChains());
    if (!restored.success) throw new Error('[network] Remote Config "chains_v2" unavailable');

    console.error(
      '[network] Remote Config is unreachable — booting on the last-known chain catalog. ' +
        'RPC endpoints may be stale until Remote Config becomes reachable.',
    );

    return chainService.sortChains(chainService.fromRemoteChains(restored.output));
  })
  .cache<Chain[]>({
    initial: [],
    staleAfter: Number.POSITIVE_INFINITY,
    map: (_, chains) => chains,
  })
  .build();

// Non-React read for use cases / other domains; shares the resource's single
// fetch + transform with every UI chain list.
export async function getChains(): Promise<Chain[]> {
  return firstValueFrom(chainResource.read$({}));
}
