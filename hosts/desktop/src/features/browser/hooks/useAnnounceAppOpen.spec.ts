// @vitest-environment happy-dom

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { onProductModalityOpenedSideEffect } from '@/domains/product';
import { browserTabs } from '@/aggregates/browser-tabs';

import { useAnnounceAppOpen } from './useAnnounceAppOpen';

// The real selection state drives the hook: what it reacts to is a tab becoming
// selected, and `selectedTab$` is the aggregate's own derivation of that. Every state
// is reset between cases by the execution environment.
const TABS = [
  { id: 'app.dot', type: 'product', deeplink: '' },
  { id: 'dashboard', type: 'dashboard', deeplink: '' },
  { id: 'new-tab-id', type: 'new-tab', deeplink: '' },
];

const select = (id: string | null) => act(() => browserTabs.selectTab(id));

// The announce is a DI side effect: a handler registered on it observes the real
// fan-out, so nothing has to stand in for the domain that declares it.
const announced = vi.fn();

beforeEach(() => {
  for (const tab of TABS) browserTabs.addTab(tab, { persistable: false });
  onProductModalityOpenedSideEffect.registerHandler({ available: () => true, body: announced });
});

afterEach(() => {
  onProductModalityOpenedSideEffect.resetHandlers();
  vi.clearAllMocks();
});

describe('useAnnounceAppOpen', () => {
  it('does not fire on the initial mount for a restored product tab', () => {
    browserTabs.selectTab('app.dot');
    renderHook(() => useAnnounceAppOpen());
    expect(announced).not.toHaveBeenCalled();
  });

  it('fires app-open when a product tab becomes selected after mount', () => {
    renderHook(() => useAnnounceAppOpen());
    expect(announced).not.toHaveBeenCalled();

    select('app.dot');
    expect(announced).toHaveBeenCalledWith({ productId: 'app.dot', kind: 'app' });
  });

  it('does not fire when a system tab becomes selected', () => {
    renderHook(() => useAnnounceAppOpen());

    select('dashboard');
    expect(announced).not.toHaveBeenCalled();
  });

  it('does not fire when a new-tab becomes selected', () => {
    renderHook(() => useAnnounceAppOpen());

    select('new-tab-id');
    expect(announced).not.toHaveBeenCalled();
  });

  it('does not fire when nothing is selected', () => {
    renderHook(() => useAnnounceAppOpen());

    select(null);
    expect(announced).not.toHaveBeenCalled();
  });
});
