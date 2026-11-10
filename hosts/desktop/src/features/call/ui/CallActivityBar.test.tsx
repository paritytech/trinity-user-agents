// @vitest-environment happy-dom

import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import { type ChatSession } from '@/domains/chat';
import { setCallActive, setCallIdle } from '../state/callActivity';

import { CallActivityBar } from './CallActivityBar';

const focusCallWindow = vi.fn();

const session = { sessionId: 's1' } as unknown as ChatSession;

const renderBar = () =>
  render(
    <TranslationProvider>
      <CallActivityBar session={session} />
    </TranslationProvider>,
  );

describe('CallActivityBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // isElectron() only checks `typeof window.App === 'object'`, so a stub is enough.
    window.App = { focusCallWindow } as unknown as typeof window.App;
  });

  afterEach(() => {
    // Reset runs while the component is still mounted (unmount is a later afterEach),
    // so the resulting re-render must be wrapped in act.
    act(() => {
      setCallIdle();
    });
  });

  it('renders nothing when no call is active', () => {
    setCallIdle();
    const { container } = renderBar();
    expect(container.firstChild).toBeNull();
  });

  it('shows the peer name while a call is active in this chat', () => {
    setCallActive('mysticRiver.88', 's1');
    renderBar();
    expect(screen.getByText('mysticRiver.88')).toBeDefined();
  });

  it('stays hidden on a different chat than the active call', () => {
    setCallActive('mysticRiver.88', 'other-session');
    const { container } = renderBar();
    expect(container.firstChild).toBeNull();
  });

  it('shows on any chat when the call has no known session', () => {
    setCallActive('mysticRiver.88', null);
    renderBar();
    expect(screen.getByText('mysticRiver.88')).toBeDefined();
  });

  it('focuses the call window when clicked', async () => {
    setCallActive('mysticRiver.88', 's1');
    renderBar();
    await userEvent.click(screen.getByRole('button'));
    expect(focusCallWindow).toHaveBeenCalledTimes(1);
  });
});
