import { useMemo } from 'react';

import { dashboardLayoutService } from '@/domains/application';
import { useProductSessions } from '@/domains/chat';
import { useP2PSessions } from '@/aggregates/p2p-chat';
import { useTruapiUserId } from '@/aggregates/truapi-runtime';
import { type CardRenderProps, DashboardCardChrome } from '@/features/dashboard';
import { CHAT_CARD_LAYOUT_RULES, CHAT_WIDGET_VISIBLE_COUNT } from '../constants';

import { ChatWidget } from './ChatWidget';

// Chrome-owning wrapper for the chat dashboard widget. Owns the session
// subscription (single source) and lifts the loading state to the block level
// so DashboardCardChrome can pulse the whole widget while sessions resolve
// (see the pulse design doc).
export const ChatWidgetContent = (props: CardRenderProps) => {
  const userId = useTruapiUserId();
  const { data: productSessions, pending: pendingProduct } = useProductSessions(userId);
  const { data: p2pSessions, pending: pendingP2P } = useP2PSessions();
  const isLoading = pendingProduct || pendingP2P;
  const sessions = useMemo(() => [...productSessions, ...p2pSessions], [productSessions, p2pSessions]);

  const variant = dashboardLayoutService.getVariantFromGridSize(props.width, props.height);
  const visibleCount = CHAT_WIDGET_VISIBLE_COUNT[variant];

  return (
    <DashboardCardChrome
      card={props.card}
      width={props.width}
      height={props.height}
      layoutRules={CHAT_CARD_LAYOUT_RULES}
      isLoading={isLoading}
      isMenuOpen={props.isMenuOpen}
      onMenuOpenChange={open => props.onMenuOpenChange(props.menuId, open)}
      onResizeCard={props.onResizeCard}
      onRemoveCard={props.onRemoveCard}
      onCleanupCards={props.onCleanupCards}
    >
      <ChatWidget visibleCount={visibleCount} sessions={sessions} pending={isLoading} />
    </DashboardCardChrome>
  );
};
