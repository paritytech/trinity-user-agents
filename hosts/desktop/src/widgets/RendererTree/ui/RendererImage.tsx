import { type ImageFit, type ImageProps, type Modifier } from '@parity/truapi';
import { type CSSProperties } from 'react';

import { useRendererImage } from '@/domains/product';

import { modifiersToStyle } from './renderNode';

const OBJECT_FIT: Record<ImageFit, NonNullable<CSSProperties['objectFit']>> = {
  None: 'none',
  Fill: 'fill',
  Cover: 'cover',
  Contain: 'contain',
  ScaleDown: 'scale-down',
};

type Props = {
  productId: string;
  modifiers: Modifier[];
  props: ImageProps;
};

/**
 * An `Image` node's bytes, drawn.
 *
 * A component rather than a branch of `renderNode` because resolving the bytes is a
 * hook, and `renderNode` is a plain function. The tree carries no URL — only a
 * Bulletin CID or an archive-relative path — so the host fetches and the product
 * never gets to name an address the host will load.
 *
 * Draws nothing until the bytes resolve, and nothing at all when they do not, which
 * is how every other unresolvable node in the tree degrades.
 */
export const RendererImage = ({ productId, modifiers, props }: Props) => {
  const { data: dataUrl } = useRendererImage(productId, props.source);

  if (!dataUrl) return null;

  return (
    <img
      src={dataUrl}
      // The wire carries no alternative text, and inventing one would describe bytes
      // the host has deliberately not interpreted.
      alt=""
      style={{ ...modifiersToStyle(modifiers), objectFit: OBJECT_FIT[props.fit ?? 'Fill'] }}
    />
  );
};
