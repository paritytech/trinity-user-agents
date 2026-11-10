import { type ResizeHandleAxis } from 'react-grid-layout';

import { type WidgetSizeIconVariant } from './types';

// Layout grid constants
export const ALLOWED_WIDGET_HEIGHTS = [2, 4, 8];
export const DEFAULT_RESIZE_HANDLES: ResizeHandleAxis[] = ['s'];
export const MAX_WIDGET_WIDTH = 2;
export const MAX_WIDGET_HEIGHT = 8;
export const MAX_GRID_ROWS = 8;

// Canonical grid footprint for each size variant — the single source of truth
// shared by the resize menu, the add-widget modal, and the size-hint-derived
// layout bounds in the service. These are real dashboard grid cells (cols × rows).
export const WIDGET_VARIANT_GRID_SIZE: Record<WidgetSizeIconVariant, { w: number; h: number }> = {
  small: { w: 1, h: 2 },
  medium: { w: 1, h: 4 },
  large: { w: 1, h: 8 },
  horizontal: { w: 2, h: 4 },
};

// A widget's declared `height` hints are size identifiers, NOT grid rows: the
// widget lists which sizes it supports. `0` opts into horizontal (only valid
// together with `width === 2`); the rest map to the vertical variants.
// The favourites folder grid is always 3 columns. Visible-icon caps per size
// variant come straight from the no-scroll mockup (rows × 3): small 1 row,
// medium 3 rows, large 5 rows. A favourite card is 92-123px tall (icon tile plus
// its own label row), so a taller cap would divide the widget body into rows the
// card cannot fit and the last row would clip. Extra favourites stay saved but are
// not rendered until the widget is resized larger. `horizontal` is impossible for
// a folder (maxW=1) but carries a value to keep the Record total.
export const FAVORITES_GRID_COLS = 3;

export const MAX_VISIBLE_FAVORITES_BY_VARIANT: Record<WidgetSizeIconVariant, number> = {
  small: 3,
  medium: 9,
  large: 15,
  horizontal: 9,
};

export const HEIGHT_HINT_TO_VARIANT: Record<number, WidgetSizeIconVariant> = {
  1: 'small',
  2: 'medium',
  4: 'large',
};

export const HORIZONTAL_HEIGHT_MARKER = 0;

export const SIZE_VARIANT_ORDER: WidgetSizeIconVariant[] = ['small', 'medium', 'large', 'horizontal'];

// Bare label, not a full name: the dotNS suffix is the network's TLD and is only
// known at runtime, so callers derive `<label><tld>` through `baseNameOf`.
export const DEFAULT_DASHBOARD_WIDGET_PRODUCT_LABEL = 'browse';

export const FAVORITES_FOLDER_ID = 'folder-favorites';
export const FOLDER_MIN_HEIGHT = 2;
export const FOLDER_DEFAULT_HEIGHT = 4;
