import { useCallback, useMemo } from 'react';

import { dependentRead, useRead } from '@/shared/hooks';
import { useActiveEnvironment } from '@/domains/application';

import { DEFAULT_DOTNS_TLD } from './constants';
import { dotNsTldResource } from './resource';
import { dotNsService } from './service';

/**
 * The active network's dotNS TLD, for the name helpers.
 *
 * A caller feeding the suffix into a **resource parameter**, a **persisted
 * write**, or a **security predicate** MUST gate on `pending || error`. `data`
 * is always a usable suffix — `DEFAULT_DOTNS_TLD` until the read settles and
 * still that if it fails — which makes it a *guess* rather than an answer: a
 * resolve fired under the default namehashes the wrong root, an id written under
 * it names a product that exists on no network, and a check made under it both
 * rejects this network's names and accepts another network's.
 *
 * Display labels are the one safe consumer, and they take the suffix from
 * {@link useDotNsLabels} rather than from here.
 */
export function useDotNsTld() {
  // Env resolution lives in the React binding, not the resource/gateway leaf, and
  // its wait and its failure are this read's own — an unstarted `useRead` reports
  // `pending: false, error: null`, which a caller cannot tell apart from settled.
  const environment = useActiveEnvironment();
  const env = environment.data;

  const result = useRead(dotNsTldResource, {
    params: env ? { environment: env } : null,
    defaultValue: DEFAULT_DOTNS_TLD,
  });

  // Without an environment the TLD read never started, so its own refresh is a
  // no-op — recovery has to go through the read that actually failed.
  const refresh = useCallback(() => {
    environment.refresh();
    result.refresh();
  }, [environment.refresh, result.refresh]);

  return { ...dependentRead(result, environment, { enabled: true }), refresh };
}

/**
 * The name helpers that only ever *strip* the TLD, bound to the active network's.
 *
 * Needs no gate, unlike {@link useDotNsTld}: stripping is the one use of the
 * suffix a guess cannot get wrong — a name that does not end in the fallback is
 * returned whole, which is what the caller would render anyway.
 */
export function useDotNsLabels() {
  const { data: tld } = useDotNsTld();

  return useMemo(
    () => ({
      displayName: (name: string) => dotNsService.toDisplayName(name, tld),
      shortLabel: (name: string, maxLen?: number) => dotNsService.toShortLabel(name, tld, maxLen),
    }),
    [tld],
  );
}

/**
 * Identifier validation for the SDK gates, which decide whether a caller-supplied
 * string reaches the paired device and the consent dialog.
 *
 * Returns `false` until the TLD read settles and if it fails: no answer is better
 * than the fallback's, which would admit a name from another network's namespace.
 */
export function useIsProductIdentifier() {
  const { data: tld, pending, error } = useDotNsTld();

  return useCallback(
    (identifier: string) => !pending && error === null && dotNsService.isProductIdentifier(identifier, tld),
    [tld, pending, error],
  );
}
