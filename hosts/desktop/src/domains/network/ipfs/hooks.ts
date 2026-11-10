import { useRead } from '@/shared/hooks';
import { nonNullable } from '@/shared/utils';
import { useActiveEnvironment } from '@/domains/application';

import { ipfsRawResource } from './resource';

export const useIpfsRawData = (cid: Nullable<string>) => {
  // The resource takes the gateway URL as a parameter; resolving it is a
  // use-case concern, so it happens here in the React binding, not in the
  // resource. Stays `null` (no read) until the environment is assembled.
  const { data: environment } = useActiveEnvironment();

  return useRead(ipfsRawResource, {
    params: nonNullable(cid) && environment ? { cid, gatewayUrl: environment.ipfsGatewayUrl } : null,
    defaultValue: null,
    map: (cache, { cid }) => cache[cid] ?? null,
  });
};
