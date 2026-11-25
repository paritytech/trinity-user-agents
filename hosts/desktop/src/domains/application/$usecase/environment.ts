import { firstValueFrom } from 'rxjs';

import { getChains } from '@/domains/network';
import { environmentRepository } from '../environment/repository';
import { environmentResource } from '../environment/resource';
import { environmentService } from '../environment/service';
import { type DigitalDollarAsset, type Environment, type EnvironmentId } from '../environment/types';

// Composes the chain catalog with the environment assembly — two resources, which
// is what makes this a use case rather than part of either one. The assembly
// itself lives in `environment/resource.ts`; do not fold it back in, and do not
// let the resource fetch the catalog itself (a resource reads leaves and
// parameters only).
async function getById(id: EnvironmentId): Promise<Environment> {
  const chains = await getChains();

  return firstValueFrom(environmentResource.read$({ id, chains }));
}

// The active channel id: the persisted value if it's a known channel, else the
// catalog default. Sync (reads localStorage + the compile-time catalog) so it
// can run at module init, before React / the storage adapter.
function getActiveId(): EnvironmentId {
  return environmentService.toEnvironmentId(environmentRepository.readPersistedEnvironmentId());
}

function getActiveDigitalDollarAsset(): DigitalDollarAsset {
  return environmentService.activeDigitalDollarAsset(getActiveId());
}

function getActive(): Promise<Environment> {
  return getById(getActiveId());
}

export const environmentUseCase = { getActive, getById, getActiveId, getActiveDigitalDollarAsset };
