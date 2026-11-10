import { type ResizeHandleAxis } from 'react-grid-layout';

export type WidgetSizeKey = 'ICON' | 'HALF' | 'FULL';

export type WidgetSizeIconVariant = 'small' | 'medium' | 'large' | 'horizontal';

// The size hints a widget declares it supports, as fed into the dashboard grid.
// `height` entries are size identifiers (NOT grid rows); `width` is optional and
// only gates the horizontal variant. The product manifest produces a structurally
// compatible shape, but this is the dashboard-layout-owned input contract — the
// domain interprets these hints into its own `WidgetSizeIconVariant`s.
export type WidgetSizeHints = {
  height: number[];
  width?: number;
};

// Layout rules govern resize/placement constraints for one card. Each card kind
// contributes these via DI; the dashboard treats the result as opaque from the
// layout-math side. These are semantic constraints — how a consumer surfaces
// them (a size menu, a picker, hiding an empty section) is the feature's call.
export type DashboardCardLayoutRules = {
  minH?: number;
  maxH?: number;
  minW?: number;
  maxW?: number;
  // The sizes this card can be switched to. Empty/undefined means the card is
  // not size-switchable.
  switchableSizes?: WidgetSizeIconVariant[];
  // When true, the card's size is locked to its current size and cannot be
  // switched (`switchableSizes` is ignored). Used for cards whose manifest
  // declares no supported sizes but are already placed.
  lockSizeToCurrent?: boolean;
  // Hints used by the Add Widget flow (install-time picker, not the resize
  // menu). Optional — only relevant for cards launched via that flow.
  availableSizes?: WidgetSizeKey[];
  defaultSize?: WidgetSizeKey;
};

// Folder-specific payload. Defined here (rather than inside the dashboard
// feature) because the dashboard-layout domain service has built-in folder
// operations (`addToFavorites`, `removeItemFromFolder`, etc.) that read
// and write folder items. External callers should still treat folders as
// opaque payloads — this type is the internal handshake.
// `items` order IS the placement: both the Favorites widget and the Favorites SPA
// derive icon position from this array's index, so a reorder in either surface is
// visible in the other.
export type FolderCardPayload = {
  kind: 'folder';
  items: string[];
};

// Opaque per-card content. `kind` selects the SDK handlers that render/add it;
// the dashboard domain stores and returns the rest verbatim and never reads it.
export type ContentCardPayload = { kind: string; [key: string]: unknown };
export type DashboardCardPayload = FolderCardPayload | ContentCardPayload;

// Card-kind predicates live on `dashboardLayoutService` (service.ts) — they are
// stateless runtime helpers, not types.

// A placed card on the dashboard grid. Same grid fields as the legacy
// `ExtendedLayoutItem` so layout math stays payload-agnostic.
export type DashboardCard = {
  i: string;
  x: number;
  y: number;
  w: number;
  h: number;
  minW?: number;
  maxW?: number;
  minH?: number;
  maxH?: number;
  resizeHandles?: ResizeHandleAxis[];
  isDraggable?: boolean;
  isResizable?: boolean;
  static?: boolean;
  payload: DashboardCardPayload;
};

export type DashboardLayout = {
  id: string;
  pages: DashboardCard[][];
  activePageIndex: number;
  updatedAt: number;
};

// The main dashboard layout as a reader sees it: the pages plus which one is
// active. The shape `mainDashboardLayoutResource` serves.
export type MainDashboardLayoutSnapshot = {
  pages: DashboardCard[][];
  activePageIndex: number;
};
