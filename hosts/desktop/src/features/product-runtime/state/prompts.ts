import { filter, firstValueFrom } from 'rxjs';

import { createState } from '@/shared/rxstate';
import { nonNullable } from '@/shared/utils';
import { type HostPrompts } from '../createHostCallbacks';

// The core starts at app bootstrap, before anything renders, and holds one host
// surface for its whole life. Asking the user needs the mounted UI, so the core is
// handed the forwarding surface below and the UI registers the real prompts here.
const mountedPrompts = createState<HostPrompts | null>(null);

function whenMounted(): Promise<HostPrompts> {
  return firstValueFrom(mountedPrompts.value$.pipe(filter(nonNullable)));
}

/**
 * The prompts the core is started with. Each call waits for the UI to register its
 * prompts, then asks through them: a request raised before the UI mounts (a product
 * worker starting with the core) is held, not refused, because only the UI can ask.
 */
export const deferredHostPrompts: HostPrompts = {
  permissions: {
    devicePermission: async (product, request) => (await whenMounted()).permissions.devicePermission(product, request),
    remotePermission: async (product, request) => (await whenMounted()).permissions.remotePermission(product, request),
  },
  userConfirmation: {
    confirmUserAction: async review => (await whenMounted()).userConfirmation.confirmUserAction(review),
    confirmPermission: async review => (await whenMounted()).userConfirmation.confirmPermission(review),
  },
};

/** Makes `prompts` the ones the core asks through. Returns the unregister call. */
export function registerHostPrompts(prompts: HostPrompts): VoidFunction {
  mountedPrompts.set(prompts);

  return () => {
    mountedPrompts.set(current => (current === prompts ? null : current));
  };
}
