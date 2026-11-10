import { type ImageSource } from '@parity/truapi';

import { useRead } from '@/shared/hooks';
import { useActiveEnvironment } from '@/domains/application';

import { rendererUseCase } from './renderer';

// Typed so `useRead` infers `string | null` from the default rather than `null`.
const NO_IMAGE: string | null = null;

/**
 * A render tree `Image`'s bytes as a data URL, or `null` while it loads and for
 * anything the host cannot draw.
 *
 * The IPFS gateway is resolved here rather than inside the use case: the active
 * environment is a binding concern, and the read takes it as a parameter.
 */
export function useRendererImage(productId: string, source: Nullable<ImageSource>) {
  const { data: environment } = useActiveEnvironment();

  return useRead(rendererUseCase.resolveImage, {
    params: source && environment ? { productId, source, ipfsGatewayUrl: environment.ipfsGatewayUrl } : null,
    defaultValue: NO_IMAGE,
  });
}
