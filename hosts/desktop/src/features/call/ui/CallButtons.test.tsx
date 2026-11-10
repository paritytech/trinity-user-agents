// @vitest-environment happy-dom

import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type ChatSession } from '@/domains/chat';
import { setCallActive, setCallIdle } from '../state/callActivity';

import { CallButtons } from './CallButtons';

const openCallWindow = vi.fn().mockResolvedValue(undefined);

const session = { sessionId: 's1', name: of('mysticRiver.88') } as unknown as ChatSession;

const renderButtons = () =>
  render(
    <TranslationProvider>
      <CallButtons session={session} />
    </TranslationProvider>,
  );

const audioButton = () => screen.getByTestId(TEST_IDS.callAudioButton);
const videoButton = () => screen.getByTestId(TEST_IDS.callVideoButton);

describe('CallButtons', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // isElectron() only checks `typeof window.App === 'object'`, so a stub is enough.
    window.App = { openCallWindow } as unknown as typeof window.App;
  });

  afterEach(() => {
    // Reset runs while the component is still mounted (unmount is a later afterEach),
    // so the resulting re-render must be wrapped in act.
    act(() => {
      setCallIdle();
    });
  });

  it('enables both buttons when no call is active', () => {
    setCallIdle();
    renderButtons();
    expect(audioButton()).toBeEnabled();
    expect(videoButton()).toBeEnabled();
  });

  it('opens a call window when clicked with no call active', async () => {
    setCallIdle();
    renderButtons();
    await userEvent.click(audioButton());
    expect(openCallWindow).toHaveBeenCalledTimes(1);
    expect(openCallWindow.mock.calls[0]?.[0]).toMatchObject({ direction: 'outgoing', purpose: 'audio' });
  });

  it('disables both buttons while a call is active', () => {
    setCallActive('mysticRiver.88', 's1');
    renderButtons();
    expect(audioButton()).toBeDisabled();
    expect(videoButton()).toBeDisabled();
  });

  // The bar is scoped to the call peer's chat, but the buttons are not.
  it('disables the buttons in a chat other than the active call peer', () => {
    setCallActive('mysticRiver.88', 'other-session');
    renderButtons();
    expect(audioButton()).toBeDisabled();
    expect(videoButton()).toBeDisabled();
  });

  it('does not open a second call window while a call is active', async () => {
    setCallActive('mysticRiver.88', 's1');
    renderButtons();
    await userEvent.click(videoButton());
    expect(openCallWindow).not.toHaveBeenCalled();
  });
});
