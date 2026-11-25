// @vitest-environment happy-dom

import { renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { onProductModalityOpenedSideEffect } from '@/domains/product';

import { useAnnounceWidgetOpen } from './useAnnounceWidgetOpen';

// The announce is a DI side effect: a handler registered on it observes the real
// fan-out, so nothing has to stand in for the domain that declares it.
const announced = vi.fn();

beforeEach(() => {
  onProductModalityOpenedSideEffect.registerHandler({ available: () => true, body: announced });
});

afterEach(() => {
  onProductModalityOpenedSideEffect.resetHandlers();
  vi.clearAllMocks();
});

describe('useAnnounceWidgetOpen', () => {
  it('fires widget-open on mount for a product', () => {
    renderHook(() => useAnnounceWidgetOpen('app.dot'));
    expect(announced).toHaveBeenCalledWith({ productId: 'app.dot', kind: 'widget' });
  });

  it('does not fire when productId is null', () => {
    renderHook(() => useAnnounceWidgetOpen(null));
    expect(announced).not.toHaveBeenCalled();
  });
});
