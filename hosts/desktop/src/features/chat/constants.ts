import { type DashboardCardLayoutRules, type WidgetSizeIconVariant } from '@/domains/application';

export const CHAT_CARD_KIND = 'native:chat';

// Max chats shown per dashboard widget size. Chat declares small/medium/large;
// `horizontal` is never used by chat but the map must be total over the union.
export const CHAT_WIDGET_VISIBLE_COUNT: Record<WidgetSizeIconVariant, number> = {
  small: 2,
  medium: 4,
  large: 8,
  horizontal: 4,
};

// Grid id of the chat dashboard *widget* card (`card.i`), distinct from the chat
// entry/favourite id (`CHAT` = 'chat'). Chat can be a widget AND a favourite at
// once; sharing one id would let the favourites add strip the widget card.
export const CHAT_WIDGET_CARD_ID = 'native:chat:widget';

export const CHAT_CARD_LAYOUT_RULES: DashboardCardLayoutRules = {
  minH: 2,
  minW: 1,
  maxW: 1,
  switchableSizes: ['small', 'medium', 'large'],
  availableSizes: ['ICON', 'HALF', 'FULL'],
  defaultSize: 'HALF',
};
