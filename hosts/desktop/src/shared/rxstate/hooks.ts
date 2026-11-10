import { useObservable } from 'react-rx';

import { type RxState } from './state';

export function useRxState<T>(state: RxState<T>) {
  const value = useObservable(state.value$, state.get);

  return [value, state.set] as const;
}
