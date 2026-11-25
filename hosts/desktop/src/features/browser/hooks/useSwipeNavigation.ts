import { useCallback, useEffect, useRef, useState } from 'react';

import { useLooseRef } from '@/shared/hooks';
import { type TabRef } from '@/aggregates/browser-tabs';
import { SWIPE_GESTURE_END_MS, SWIPE_IDLE } from '../constants';
import { browserService } from '../service';
import { type SwipeDirection, type SwipeState, type SwipeWheel } from '../types';

type SwipeNavigation = {
  backEntry: TabRef | null;
  forwardEntry: TabRef | null;
  goBack: VoidFunction;
  goForward: VoidFunction;
};

// `enabled`: the product route is shown. `tabId`: the selected tab — `goBack` / `goForward` act on it, so a gesture
// started on one tab must never finish on another.
type Params = { enabled: boolean; tabId: string | null; navigation: SwipeNavigation };

export const useSwipeNavigation = ({ enabled, tabId, navigation }: Params) => {
  const [state, setState] = useState<SwipeState>(SWIPE_IDLE);
  const stateRef = useRef<SwipeState>(SWIPE_IDLE);
  const endTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const gestureTabIdRef = useRef<string | null>(null);
  const enabledRef = useLooseRef(enabled);
  const tabIdRef = useLooseRef(tabId);
  const navigationRef = useLooseRef(navigation);

  const update = useCallback((next: SwipeState) => {
    stateRef.current = next;
    setState(next);
  }, []);

  const cancel = useCallback(() => {
    if (endTimerRef.current !== null) clearTimeout(endTimerRef.current);
    endTimerRef.current = null;
    update(SWIPE_IDLE);
  }, [update]);

  const navigate = useCallback(
    (direction: SwipeDirection) => {
      // The cancelling effects below run after render; a timer or event in between must not navigate either.
      if (!enabledRef() || gestureTabIdRef.current !== tabIdRef()) return;
      if (direction === 'back') navigationRef().goBack();
      else navigationRef().goForward();
    },
    [enabledRef, tabIdRef, navigationRef],
  );

  // Fallback release (fingers held still, then lifted): the gesture ends on silence.
  const end = useCallback(() => {
    endTimerRef.current = null;
    const indicator = browserService.toSwipeIndicator(stateRef.current);
    update(SWIPE_IDLE);
    if (indicator?.progress === 1) navigate(indicator.direction);
  }, [update, navigate]);

  const onGuestWheel = useCallback(
    (wheel: SwipeWheel) => {
      if (!enabledRef()) return;
      if (stateRef.current.status === 'idle') gestureTabIdRef.current = tabIdRef();
      const { backEntry, forwardEntry } = navigationRef();
      const availability = {
        canGoBack: browserService.canSwipeTo(backEntry),
        canGoForward: browserService.canSwipeTo(forwardEntry),
      };
      const next = browserService.reduceSwipe(stateRef.current, wheel, availability);
      const liftedNow = next.status === 'committed' && stateRef.current.status !== 'committed';
      update(next);
      if (endTimerRef.current !== null) clearTimeout(endTimerRef.current);
      endTimerRef.current = setTimeout(end, SWIPE_GESTURE_END_MS);
      if (liftedNow) navigate(next.direction);
    },
    [update, end, navigate, enabledRef, tabIdRef, navigationRef],
  );

  useEffect(() => {
    if (!enabled) cancel();
  }, [enabled, cancel]);

  // Runs its cleanup when the selected tab changes and on unmount. A committed gesture already navigated, and a
  // cross-product entry renames the tab: its momentum must drain into `committed`, not start a second gesture.
  useEffect(
    () => () => {
      if (stateRef.current.status !== 'committed') cancel();
    },
    [tabId, cancel],
  );

  useEffect(
    () => () => {
      if (endTimerRef.current !== null) clearTimeout(endTimerRef.current);
    },
    [],
  );

  return { onGuestWheel, indicator: enabled ? browserService.toSwipeIndicator(state) : null };
};
