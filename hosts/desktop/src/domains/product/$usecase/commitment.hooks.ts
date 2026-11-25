import { useAction } from '@/shared/hooks';

import { commitmentUseCase } from './commitment';

export const useCommitProductByIdentifier = () => useAction(commitmentUseCase.commitProductByIdentifier);
export const usePinProduct = () => useAction(commitmentUseCase.pinProduct);
export const usePinExecutable = () => useAction(commitmentUseCase.pinExecutable);
export const useUnpinProduct = () => useAction(commitmentUseCase.unpinProduct);
