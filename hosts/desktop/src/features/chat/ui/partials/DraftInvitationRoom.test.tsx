// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';

import { DraftInvitationRoom } from './DraftInvitationRoom';

describe('DraftInvitationRoom', () => {
  it('shows the invite empty state and one-message hint', () => {
    render(
      <TranslationProvider>
        <DraftInvitationRoom name="mysticRiver.88" onSend={vi.fn()} onCancel={vi.fn()} />
      </TranslationProvider>,
    );

    expect(screen.getByText('Invite mysticRiver.88 to chat')).toBeInTheDocument();
    expect(screen.getByText('You can only send one message in this invitation')).toBeInTheDocument();
  });
});
