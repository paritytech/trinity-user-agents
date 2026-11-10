import { useCallback } from 'react';

import { useAction } from '@/shared/hooks';
import { type DashboardCard } from '../dashboard-layout/types';

import { cardsUseCase } from './cards';

export const useAddCard = () => {
  const { run, pending, status } = useAction(({ card }: { card: DashboardCard }) => cardsUseCase.addCardToLayout(card));
  const addCard = useCallback((card: DashboardCard) => run({ card }), [run]);
  return { addCard, pending, status };
};

export const useRemoveCard = () => {
  const { run, pending, status } = useAction(({ cardId }: { cardId: string }) => cardsUseCase.removeCardFromLayout(cardId));
  const removeCard = useCallback((cardId: string) => run({ cardId }), [run]);
  return { removeCard, pending, status };
};

export const useResizeCard = () => {
  const { run, pending, status } = useAction(({ cardId, size }: { cardId: string; size: { w: number; h: number } }) =>
    cardsUseCase.resizeCardToGridSize(cardId, size),
  );
  const resizeCard = useCallback((cardId: string, size: { w: number; h: number }) => run({ cardId, size }), [run]);
  return { resizeCard, pending, status };
};
