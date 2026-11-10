import { type ReactNode } from 'react';

import { createPipeline, createSDK, createSideEffect, createSlot, createTransformer } from '@/shared/di';
import {
  type DashboardCard,
  type DashboardCardLayoutRules,
  type DashboardCardPayload,
  type WidgetSizeIconVariant,
} from '@/domains/application';
// The dashboard host is otherwise content-free. This is the one intentional
// product reference: a TYPE-ONLY import for the typed `product:widget` variant of
// `DashboardAddRequest` below. A feature importing a domain *type* is allowed;
// genericising the variant to `unknown` would only force casts for no gain.
import { type Product } from '@/domains/product';

import { type CardRenderProps, type DashboardCardMetadata, type FolderItemIconSize } from './types';
import { type AddWidgetSidebarEntry } from './ui/add-widget/types';

// Opening a favourites-folder icon that is a native entry (e.g. chat) has no
// chain product to resolve — the owning feature handles the open by item id.
export const openFavoriteItemSideEffect = createSideEffect<{ itemId: string }>({ name: 'openFavoriteItem' });

// Opens the fullscreen Favorites SPA (the folder's "View more" tile). The dashboard
// host owns the seam; the favourites feature registers the navigation handler, so
// the folder card doesn't import the favourites route/tab machinery.
export const openFavoritesSideEffect = createSideEffect<void>({ name: 'openFavorites' });

// Opens the Add-to-Favorites dialog (the folder card's empty-state action). The
// dashboard host owns the seam; the favourites feature registers the handler, so
// the folder card doesn't reach into the favourites dialog itself.
export const openAddToFavoritesSideEffect = createSideEffect<void>({ name: 'openAddToFavorites' });

export const dashboardCardContentTransformer = createTransformer<CardRenderProps, ReactNode>({
  name: 'dashboardCardContent',
});

// Resolves the icon+label cell rendered for one favourites-folder item. Each
// content provider registers a handler that claims its own ids (returns a
// hook-bound node) and returns `null` for everyone else, so the folder renders
// "the one cell for this item" without the host importing product/native code.
// The folder still owns the grid layout, positions, drag, and remove menu —
// only the per-item icon+label CONTENT is provider-supplied.
export const folderItemContentTransformer = createTransformer<{ itemId: string; iconSize: FolderItemIconSize }, ReactNode>({
  name: 'folderItemContent',
});

// Resolves the Add-to-Dashboard modal content for a dialog target. Each content
// provider registers a handler that claims its own targets (returns the modal
// node) and returns `null` for everyone else, so the host renders "the one
// modal for this target" without branching on product-vs-native inline.
export const addToDashboardModalTransformer = createTransformer<{ targetId: string; onClose: () => void }, ReactNode>({
  name: 'addToDashboardModal',
});

export const dashboardCardMetadataTransformer = createTransformer<DashboardCardPayload, DashboardCardMetadata>({
  name: 'dashboardCardMetadata',
});

// One catalog entry the Add-Widget modal lists in its sidebar. Native entries are
// derived from `addableDashboardCardsPipeline`; everything else is *contributed* by
// a content provider through `addWidgetCatalogSources` (see `state/addWidgetCatalog`).
// The entry is content-agnostic: it carries an already-resolved `label` (for the
// modal's central search/sort), an opaque `renderIcon`, and an opaque `payload` the
// provider's own panel handler narrows. `kind` routes `addWidgetPanelTransformer`.
export type AddWidgetContributedEntry = {
  kind: string;
  // Stable id; also doubles as a search token. Distinct from `kind`: e.g. a
  // product entry's id is its baseName while `kind` is `product:widget`.
  id: string;
  // Already-resolved display name. Contributed entries hold a concrete string
  // (external manifest, etc.); the modal does not translate them.
  label: string;
  // Extra haystack tokens for the modal's central search, beyond `label`/`id`.
  searchText?: string;
  renderIcon: () => ReactNode;
  // Opaque per-entry data the provider's `addWidgetPanelTransformer` handler reads.
  payload: unknown;
};

// Context the Add-Widget modal threads into a resolved panel: the same callbacks
// and dashboard state every panel needs, regardless of content kind.
export type AddWidgetPanelContext = {
  dashboardPages: DashboardCard[][];
  favoriteProductIds: ReadonlySet<string>;
  onNavigateToDashboardPage: (pageIndex: number) => void;
};

// Resolves the Add-Widget panel for the selected sidebar entry. Each content
// provider registers a handler that claims its own entries (native by
// `entry.source === 'native'`, contributed by `entry.entry.kind`) and returns
// `null` otherwise, so the modal renders "the one panel for this entry" without
// branching on product-vs-native inline (mirrors `addToDashboardModalTransformer`).
// The dashboard host registers the native handler; content providers register the
// contributed ones. Panels needing hooks return a hook-bound node.
export const addWidgetPanelTransformer = createTransformer<
  { entry: AddWidgetSidebarEntry; context: AddWidgetPanelContext },
  ReactNode
>({
  name: 'addWidgetPanel',
});

export const dashboardCardActionsSlot = createSlot<{ payload: DashboardCardPayload }>({
  name: 'dashboardCardActions',
});

// Extra entries a card feature contributes to the widget "…" menu (e.g. Reload),
// rendered above the destructive Remove item. Named after the place of use — the
// menu — not after any single contributor.
export const dashboardCardMenuItemsSlot = createSlot<{ payload: DashboardCardPayload }>({
  name: 'dashboardCardMenuItems',
});

// Self-describing copy/preview the Add Widget modal renders for a native card.
// Carried by the entry so the modal renders exactly what the feature declares —
// no per-kind branching inside the dashboard feature.
type AddableWidgetCardDefinition = {
  titleKey: string;
  descriptionKey: string;
  previewVariant: WidgetSizeIconVariant;
  sizeVariants: WidgetSizeIconVariant[];
};

export type AddableDashboardCard = {
  kind: string;
  // Entry **identity** id — the favourites-folder item id and the lookup key
  // (`byGridId`, `isNativeAddableDashboardId`). Distinct from `kind` (the payload
  // type): e.g. Favorites has kind `folder:favorites` but grid id `folder-favorites`,
  // and Chat has kind `native:chat` / grid id `chat`.
  gridId: string;
  // Grid id of the **placed widget card** (`card.i`). Defaults to `gridId`. Set
  // distinct only when an entry can coexist as a widget AND a favourite at once
  // (e.g. chat): the favourites add strips the top-level card sharing the
  // favourite id, so the widget needs its own id to survive favouriting.
  widgetGridId?: string;
  // i18n keys (not literals): entries are declared at module load, outside React,
  // so the sidebar label and panel header are translated at render time. Keeping
  // them as keys also avoids a second source of truth drifting from `en.json`.
  displayNameKey: string;
  descriptionKey?: string;
  icon?: ReactNode;
  defaultLayoutRules?: DashboardCardLayoutRules;
  widgetCard: AddableWidgetCardDefinition;
  // When true, the entry can also be added as a 1×1 favourites icon — the Add
  // Widget / AtD panels render an "Add to Favorites" button. The Favorites folder
  // entry itself omits it (you can't favourite the favourites folder).
  supportsFavorites?: boolean;
  // When true, tapping this entry's favourites-folder icon dispatches
  // `openFavoriteItemSideEffect`. Set only when the owning feature registers an
  // open handler — omit for widget-only addables (e.g. the Favorites folder).
  openFromFavorites?: boolean;
  createCard: () => { payload: DashboardCardPayload; gridSize: { w: number; h: number } };
};

// Catalog of native widgets the user can add to the dashboard. Product
// widgets aren't here — the modal lists them from `useProducts()` directly.
export const addableDashboardCardsPipeline = createPipeline<AddableDashboardCard[], object>({
  name: 'addableDashboardCards',
});

// Per-kind add request — the single payload the uniform add dispatch carries.
// Native/chat members carry the static catalog `entry` they were built from;
// the product member carries the already-resolved chain `Product`. The two
// members are discriminated structurally (`'product' in request`) so each
// provider's handler narrows to its own shape without an `as` cast.
// `size` is the only signal: a 1×1 add is a favourite, anything larger is a
// widget — there is no separate `target` flag to keep in sync with it.
export type DashboardAddRequest =
  | { kind: string; size: { w: number; h: number }; entry: AddableDashboardCard }
  | { kind: 'product:widget'; size: { w: number; h: number }; product: Product };

export type DashboardAddOutcome = { ok: boolean; pageIndex?: number };

// Uniform add dispatch keyed by content `kind`. Each content provider registers
// exactly one handler that resolves its own kind (returns a Promise) and returns
// `null` for every other kind, so the transformer resolves "the one handler for
// this request". The Promise is non-nullable, so the transformer carries it.
export const dashboardCardAddTransformer = createTransformer<DashboardAddRequest, Promise<DashboardAddOutcome>>({
  name: 'dashboardCardAdd',
});

export const dashboardCardSDK = createSDK({
  required: {
    content: dashboardCardContentTransformer,
  },
  optional: {
    metadata: dashboardCardMetadataTransformer,
    actions: dashboardCardActionsSlot,
    menuItems: dashboardCardMenuItemsSlot,
    addable: addableDashboardCardsPipeline,
    add: dashboardCardAddTransformer,
    open: openFavoriteItemSideEffect,
    folderItemContent: folderItemContentTransformer,
    addWidgetPanel: addWidgetPanelTransformer,
    addToDashboardModal: addToDashboardModalTransformer,
  },
});
