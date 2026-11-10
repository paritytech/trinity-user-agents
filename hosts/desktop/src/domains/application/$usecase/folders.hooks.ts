import { useCallback } from 'react';

import { useAction } from '@/shared/hooks';

import { cardsUseCase } from './cards';
import { foldersUseCase } from './folders';

export const useAddToFavorites = () => {
  const { run, pending, status } = useAction(({ itemId }: { itemId: string }) => foldersUseCase.addToFavorites(itemId));
  const addToFavorites = useCallback((itemId: string) => run({ itemId }), [run]);
  return { addToFavorites, pending, status };
};

export const useRemoveItemFromFolder = () => {
  const { run, pending, status } = useAction(({ itemId }: { itemId: string }) => foldersUseCase.removeItemFromFolder(itemId));
  const removeItemFromFolder = useCallback((itemId: string) => run({ itemId }), [run]);
  return { removeItemFromFolder, pending, status };
};

export const useRemoveFolder = () => {
  // A folder is a first-class card, so removing it is removing its card.
  const { run, pending, status } = useAction(({ folderId }: { folderId: string }) => cardsUseCase.removeCardFromLayout(folderId));
  const removeFolder = useCallback((folderId: string) => run({ folderId }), [run]);
  return { removeFolder, pending, status };
};
