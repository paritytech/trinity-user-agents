import { firstValueFrom } from 'rxjs';

import { createSideEffect } from '@/shared/di';
import { environmentUseCase } from '@/domains/application';
import { dotNsService } from '../dotns/service';
import { declinedUpdatesRepository } from '../product/declined-updates/repository';
import { type ExecutableKind } from '../product/manifest/constants';
import { liveExecutableResource } from '../product/manifest/resource';
import { type LiveExecutable } from '../product/manifest/types';
import { productDb } from '../product/repository';
import { productService } from '../product/service';

import { dotNsUseCase } from './dotns';

// A product modality (app | widget | worker) became the thing the user is
// looking at. A neutral product-runtime event: the three surfaces that own an
// "open" moment (.apply) and the offline-access toaster (.inject) both reference
// this one identifier, so no feature depends on another.
export const onProductModalityOpenedSideEffect = createSideEffect<{ productId: string; kind: ExecutableKind }>({
  name: 'onProductModalityOpened',
});

// The imperative "should we nudge for this modality?" chokepoint. Non-null only
// when the product is pinned, has a frozen executable for `kind`, its live
// on-chain resolution has drifted, and that exact version is not declined.
// Reads through `liveExecutableResource` (the same cached signal the settings
// section and re-pin use), so a nudged update is always applicable and repeated
// opens don't re-hit the dotNS endpoint. `identifier` is normalized to the
// canonical base name first — opens arrive keyed by the raw webview id (any
// casing, `.dot` optional), while committed rows and declined records are keyed
// by the normalized base name.
async function checkModalityUpdate({
  baseName: identifier,
  kind,
}: {
  baseName: string;
  kind: ExecutableKind;
}): Promise<LiveExecutable | null> {
  const baseName = dotNsService.baseNameOf(identifier, await dotNsUseCase.getActiveTld());
  const stored = await productDb.getByBaseName(baseName);
  if (stored.isErr() || !stored.value || !stored.value.pinned) return null;

  const frozen = stored.value.executables[kind];
  if (!frozen) return null;

  const environment = await environmentUseCase.getActive();
  const live = await firstValueFrom(liveExecutableResource.read$({ product: stored.value, kind, environment }));
  if (!live) return null;
  if (!productService.hasExecutableDrift(frozen, live)) return null;

  if (await declinedUpdatesRepository.isDeclined(baseName, kind, live.contenthash)) return null;

  return live;
}

export const updatesUseCase = {
  onProductModalityOpenedSideEffect,
  checkModalityUpdate,
};
