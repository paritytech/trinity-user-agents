// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
// eslint-disable-next-line boundaries/dependencies -- the test observes the write where it lands; the barrel exposes no seam for it
import { requestPreferencesRepository } from '@/domains/chat/request-preferences/repository';

import { ChatSettingsPage } from './ChatSettingsPage';

// `useHideRequestsByDefault` needs no stub: `hideRequestsByDefaultResource`'s mock
// already answers HIDE_REQUESTS_BY_DEFAULT. The write lands in the repository, a plain
// object spied in place, so the real hook and mutation run up to that point.
const setHideRequestsByDefault = vi.spyOn(requestPreferencesRepository, 'setHideRequestsByDefault');

describe('ChatSettingsPage', () => {
  it('reflects the preference and toggles it off', async () => {
    render(
      <TranslationProvider>
        <ChatSettingsPage />
      </TranslationProvider>,
    );

    const toggle = screen.getByRole('switch');
    expect(toggle).toBeChecked();

    await userEvent.click(toggle);
    expect(setHideRequestsByDefault).toHaveBeenCalledWith(false);
  });
});
