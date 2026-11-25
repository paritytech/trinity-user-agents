import PlaceholderGlyph from '@/shared/assets/images/app-icon-placeholder.svg?jsx';
import { cnTw } from '@/shared/utils';

type Props = {
  // Mirrors `AppIcon`'s size prop so the glyph is sized against its own tile.
  size?: '24' | '32' | '44' | '64';
};

// Figma draws the glyph at a per-tile size rather than a constant fraction of the
// tile, so map the sizes instead of deriving a percentage.
const glyphSizeClass: Record<NonNullable<Props['size']>, string> = {
  '24': cnTw('size-4'),
  '32': cnTw('size-5'),
  '44': cnTw('size-7'),
  '64': cnTw('size-12'),
};

// The empty-state glyph for an app tile, passed to `AppIcon` as its `children` so
// it replaces the library's own dashed square. Inherits the tile's text colour.
export const AppIconPlaceholder = ({ size = '32' }: Props) => {
  return <PlaceholderGlyph aria-hidden className={cnTw('shrink-0', glyphSizeClass[size])} />;
};
