import { type DashboardCard, type DashboardCardPayload } from '@/domains/application';

import { type AddableDashboardCard } from './di';

export const buildNativeDashboardCard = (entry: AddableDashboardCard, size: { w: number; h: number }): DashboardCard => {
  const { payload, gridSize } = entry.createCard();
  const rules = entry.defaultLayoutRules;

  return {
    i: entry.widgetGridId ?? entry.gridId,
    x: 0,
    y: 0,
    w: size.w,
    h: size.h,
    minW: rules?.minW ?? gridSize.w,
    maxW: rules?.maxW ?? 1,
    minH: rules?.minH ?? gridSize.h,
    maxH: rules?.maxH ?? 8,
    payload: payload satisfies DashboardCardPayload,
  };
};
