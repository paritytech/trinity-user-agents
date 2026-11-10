import { environmentUseCase } from '@/domains/application';
import { type IpfsFetchOptions, ipfsGateway } from '../ipfs/gateway';

/**
 * Uncached IPFS fetch: resolves the active environment's IPFS gateway URL
 * (owned by the `application` domain) and performs the wire fetch via
 * `ipfsGateway`. Composing the env source with the gateway I/O is what keeps
 * `ipfsGateway` a pure wire adapter — the cross-domain resolution lives here.
 *
 * `ipfsRawResource` performs the same read behind a cache for immutable blobs;
 * callers that must not cache a transient miss (e.g. preimage polling) use this.
 */
async function fetchRaw(cid: string, options?: IpfsFetchOptions): Promise<Uint8Array | null> {
  const { ipfsGatewayUrl } = await environmentUseCase.getActive();
  return ipfsGateway.fetchRaw(ipfsGatewayUrl, cid, options);
}

export const ipfsUseCase = {
  fetchRaw,
};
