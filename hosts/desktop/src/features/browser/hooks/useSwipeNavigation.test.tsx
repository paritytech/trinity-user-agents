// @vitest-environment happy-dom
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type TabRef } from '@/aggregates/browser-tabs';
import { SWIPE_GESTURE_END_MS, SWIPE_THRESHOLD_PX } from '../constants';
import { NEW_TAB, PRODUCT } from '../tabs/helpers';

import { useSwipeNavigation } from './useSwipeNavigation';

const productEntry: TabRef = { id: 'app.dot', type: PRODUCT, deeplink: '' };

type Overrides = Partial<{ backEntry: TabRef | null; forwardEntry: TabRef | null }>;

function setup(overrides: Overrides = {}) {
  const navigation = { backEntry: productEntry, forwardEntry: productEntry, goBack: vi.fn(), goForward: vi.fn(), ...overrides };
  const hook = renderHook(props => useSwipeNavigation(props), {
    initialProps: { enabled: true, tabId: 'tab-a' as string | null, navigation },
  });
  return { navigation, hook };
}

const wheel = (hook: ReturnType<typeof setup>['hook'], deltaX: number, deltaY = 0, consumed = false) =>
  act(() => hook.result.current.onGuestWheel({ deltaX, deltaY, consumed }));
const idle = () => act(() => vi.advanceTimersByTime(SWIPE_GESTURE_END_MS));

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('useSwipeNavigation', () => {
  it('navigates back when the gesture ends past the threshold', () => {
    const { navigation, hook } = setup();
    wheel(hook, -SWIPE_THRESHOLD_PX);
    expect(hook.result.current.indicator).toEqual({ direction: 'back', progress: 1 });
    idle();
    expect(navigation.goBack).toHaveBeenCalledTimes(1);
    expect(navigation.goForward).not.toHaveBeenCalled();
    expect(hook.result.current.indicator).toBeNull();
  });

  it('navigates forward on a rightward gesture past the threshold', () => {
    const { navigation, hook } = setup();
    wheel(hook, SWIPE_THRESHOLD_PX);
    idle();
    expect(navigation.goForward).toHaveBeenCalledTimes(1);
  });

  it('keeps the gesture alive while events keep arriving', () => {
    const { navigation, hook } = setup();
    for (const deltaX of [-40, -45, -55, -60]) {
      wheel(hook, deltaX);
      act(() => vi.advanceTimersByTime(SWIPE_GESTURE_END_MS - 1));
    }
    expect(navigation.goBack).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1));
    expect(navigation.goBack).toHaveBeenCalledTimes(1);
  });

  it('cancels below the threshold', () => {
    const { navigation, hook } = setup();
    wheel(hook, -SWIPE_THRESHOLD_PX / 2);
    expect(hook.result.current.indicator).toEqual({ direction: 'back', progress: 0.5 });
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
    expect(hook.result.current.indicator).toBeNull();
  });

  it('does not track towards a missing entry', () => {
    const { navigation, hook } = setup({ backEntry: null });
    wheel(hook, -SWIPE_THRESHOLD_PX);
    expect(hook.result.current.indicator).toBeNull();
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('does not track towards a new-tab entry', () => {
    const { navigation, hook } = setup({ backEntry: { id: 'nt-1', type: NEW_TAB, deeplink: '' } });
    wheel(hook, -SWIPE_THRESHOLD_PX);
    expect(hook.result.current.indicator).toBeNull();
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('starts a fresh gesture after a rejected one ends', () => {
    const { navigation, hook } = setup();
    wheel(hook, 0, 20);
    idle();
    wheel(hook, -SWIPE_THRESHOLD_PX);
    idle();
    expect(navigation.goBack).toHaveBeenCalledTimes(1);
  });

  it('ignores wheels while disabled', () => {
    const { navigation, hook } = setup();
    hook.rerender({ enabled: false, tabId: 'tab-a', navigation });
    wheel(hook, -SWIPE_THRESHOLD_PX);
    expect(hook.result.current.indicator).toBeNull();
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('cancels a pending gesture when disabled', () => {
    const { navigation, hook } = setup();
    wheel(hook, -SWIPE_THRESHOLD_PX);
    hook.rerender({ enabled: false, tabId: 'tab-a', navigation });
    expect(hook.result.current.indicator).toBeNull();
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('cancels a pending gesture when the tab changes', () => {
    const { navigation, hook } = setup();
    wheel(hook, -SWIPE_THRESHOLD_PX);
    hook.rerender({ enabled: true, tabId: 'tab-b', navigation });
    expect(hook.result.current.indicator).toBeNull();
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('cancels a pending gesture on unmount', () => {
    const { navigation, hook } = setup();
    wheel(hook, -SWIPE_THRESHOLD_PX);
    hook.unmount();
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('navigates on finger lift without waiting for the gesture to end', () => {
    const { navigation, hook } = setup();
    for (const deltaX of [-100, -60, -50, -40]) wheel(hook, deltaX);
    expect(navigation.goBack).toHaveBeenCalledTimes(1);
    expect(hook.result.current.indicator).toBeNull();
  });

  it('navigates once while the momentum after a lift keeps arriving', () => {
    const { navigation, hook } = setup();
    for (const deltaX of [-100, -60, -50, -40, -30, -20, -10]) wheel(hook, deltaX);
    idle();
    expect(navigation.goBack).toHaveBeenCalledTimes(1);
  });

  it('navigates once when the tab id changes after a commit', () => {
    const { navigation, hook } = setup();
    for (const deltaX of [-100, -60, -50, -40]) wheel(hook, deltaX);
    hook.rerender({ enabled: true, tabId: 'tab-b', navigation });
    for (const deltaX of [-90, -80, -70, -60, -50]) wheel(hook, deltaX);
    idle();
    expect(navigation.goBack).toHaveBeenCalledTimes(1);
  });

  it('does not navigate on a lift below the threshold', () => {
    const { navigation, hook } = setup();
    for (const deltaX of [-50, -40, -30, -20]) wheel(hook, deltaX);
    idle();
    expect(navigation.goBack).not.toHaveBeenCalled();
  });

  it('keeps onGuestWheel referentially stable across renders', () => {
    const { navigation, hook } = setup();
    const first = hook.result.current.onGuestWheel;
    hook.rerender({ enabled: true, tabId: 'tab-a', navigation: { ...navigation } });
    expect(hook.result.current.onGuestWheel).toBe(first);
  });
});
