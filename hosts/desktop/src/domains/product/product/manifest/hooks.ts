import { useMemo } from 'react';

import { dependentRead, useRead } from '@/shared/hooks';
import { nonNullable } from '@/shared/utils';
import { useActiveEnvironment } from '@/domains/application';
import { ipfsService, useIpfsRawData } from '@/domains/network';
import { type Product } from '../types';

import { type ExecutableKind } from './constants';
import {
  archiveCacheKey,
  executableArchiveResource,
  liveExecutableCacheKey,
  liveExecutableResource,
  missingArchiveCacheKey,
} from './resource';
import { manifestService } from './service';
import { type Icon, type LiveExecutable } from './types';

// Fetches the IPFS archive for a product's executable of the given kind and
// registers it with the Electron sandbox so `polkadot://<identifier>` URLs
// resolve to that archive's files. The resource reads the executable's
// `identifier` off the product — no subname derivation at the call site.
export const useExecutableArchive = (params: Nullable<{ product: Product; kind: ExecutableKind }>) => {
  const environment = useActiveEnvironment();

  const result = useRead(executableArchiveResource, {
    params: params && environment.data ? { ...params, ipfsGatewayUrl: environment.data.ipfsGatewayUrl } : null,
    defaultValue: null,
    map(cache, { product, kind }) {
      const executable = product.executables[kind];
      if (!executable) return cache[missingArchiveCacheKey(product.baseName, kind)];
      return cache[archiveCacheKey(product.baseName, kind, executable.contenthash)];
    },
  });

  return dependentRead(result, environment, { enabled: nonNullable(params) });
};

// Resolves a manifest `Icon` (CID + format) to a base64 data URL via the IPFS
// raw resource. Returns `null` while bytes are loading or when the icon has
// no CID (e.g. native products).
export const useProductIcon = (icon: Nullable<Icon>) => {
  const cid = icon?.cid && icon.cid.length > 0 ? icon.cid : null;
  const { data: bytes, pending, error } = useIpfsRawData(cid);

  const dataUrl = useMemo(() => {
    // Unknown format → render a placeholder (null): the product stays
    // launchable, the host never sniffs or auto-corrects the bytes.
    if (!bytes || !icon || !manifestService.isRenderableIconFormat(icon.format)) return null;
    const format = icon.format.toLowerCase();
    if (format !== 'png' && format !== 'jpeg') return null;
    return ipfsService.toDataUrl(bytes, format);
  }, [bytes, icon]);

  return { data: dataUrl, pending, error };
};

// The environment is resolved here and passed into the resource as a parameter —
// the resource reads gateways only, never a use case.
export const useLiveExecutable = (params: Nullable<{ product: Product; kind: ExecutableKind }>) => {
  const environment = useActiveEnvironment();

  const result = useRead(liveExecutableResource, {
    params: params && environment.data ? { ...params, environment: environment.data } : null,
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- narrowing null to typed defaultValue
    defaultValue: null as LiveExecutable | null,
    map(cache, { product, kind, environment: env }) {
      const executable = product.executables[kind];
      if (!executable) return null;
      return cache[liveExecutableCacheKey(env, product.baseName, kind)] ?? null;
    },
  });

  return dependentRead(result, environment, { enabled: nonNullable(params) });
};
