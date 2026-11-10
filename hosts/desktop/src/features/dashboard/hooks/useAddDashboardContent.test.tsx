// @vitest-environment happy-dom

import { renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { type AddableDashboardCard, type DashboardAddRequest, dashboardCardAddTransformer } from '../di';

import { useAddDashboardContent } from './useAddDashboardContent';

// Minimal catalog entry — the handler under test only forwards the request, so
// only the fields the dispatch reads need to be present.
const entry: AddableDashboardCard = {
  kind: 'native:chat',
  gridId: 'chat',
  displayNameKey: 'feature.chat.title',
  widgetCard: { titleKey: 't', descriptionKey: 'd', previewVariant: 'small', sizeVariants: ['small'] },
  createCard: () => ({ payload: { kind: 'native:chat' }, gridSize: { w: 1, h: 4 } }),
};

// A native add request — the chat-shaped member of the union.
const nativeRequest: DashboardAddRequest = {
  kind: 'native:chat',
  size: { w: 1, h: 4 },
  entry,
};

const registerHandler = (body: (request: DashboardAddRequest) => Promise<{ ok: boolean; pageIndex?: number }> | null) => {
  const handler = { available: () => true, body };
  dashboardCardAddTransformer.registerHandler(handler);
  return () => dashboardCardAddTransformer.removeHandler(handler);
};

describe('useAddDashboardContent', () => {
  afterEach(() => {
    dashboardCardAddTransformer.resetHandlers();
  });

  it('invokes the handler registered for a kind and returns its outcome', async () => {
    const outcome = { ok: true, pageIndex: 2 };
    const body = vi.fn((request: DashboardAddRequest) => (request.kind === 'native:chat' ? Promise.resolve(outcome) : null));
    registerHandler(body);

    const { result } = renderHook(() => useAddDashboardContent());
    const resolved = await result.current.addContent(nativeRequest);

    expect(body).toHaveBeenCalledWith(nativeRequest, undefined);
    expect(resolved).toEqual(outcome);
  });

  it('returns { ok: false } when no handler resolves the request kind', async () => {
    // Registered handler resolves only the product kind, so a native request
    // falls through to no match.
    registerHandler(request => (request.kind === 'product:widget' ? Promise.resolve({ ok: true }) : null));

    const { result } = renderHook(() => useAddDashboardContent());
    const resolved = await result.current.addContent(nativeRequest);

    expect(resolved).toEqual({ ok: false });
  });

  it('returns { ok: false } when there is no handler at all', async () => {
    const { result } = renderHook(() => useAddDashboardContent());
    const resolved = await result.current.addContent(nativeRequest);

    expect(resolved).toEqual({ ok: false });
  });
});
