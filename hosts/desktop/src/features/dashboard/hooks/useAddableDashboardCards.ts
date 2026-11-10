import { useMemo } from 'react';

import { usePipeline } from '@/shared/di';
import {
  ADDABLE_DASHBOARD_CARDS_PIPELINE_CTX,
  ADDABLE_DASHBOARD_CARDS_PIPELINE_SEED,
  buildAddableDashboardCardsByGridId,
} from '../addableDashboardCards';
import { type AddableDashboardCard, addableDashboardCardsPipeline } from '../di';

export function useAddableDashboardCards(): {
  cards: AddableDashboardCard[];
  byGridId: ReadonlyMap<string, AddableDashboardCard>;
} {
  const cards = usePipeline(
    addableDashboardCardsPipeline,
    ADDABLE_DASHBOARD_CARDS_PIPELINE_SEED,
    ADDABLE_DASHBOARD_CARDS_PIPELINE_CTX,
  );
  const byGridId = useMemo(() => buildAddableDashboardCardsByGridId(cards), [cards]);
  return { cards, byGridId };
}
