// @vitest-environment happy-dom

import { act, render, screen, waitFor } from '@testing-library/react';
import { type PropsWithChildren } from 'react';
import { describe, expect, it } from 'vitest';

import { ConfirmationProvider } from '@/shared/components';
import { TranslationProvider } from '@/shared/translation';
import { onDeviceOsPermissionBlockedSideEffect } from '@/domains/product';

import { useDeviceBlockedPrompt } from './useDeviceBlockedPrompt';

const wrapper = ({ children }: PropsWithChildren) => (
  <TranslationProvider locale="en">
    <ConfirmationProvider>{children}</ConfirmationProvider>
  </TranslationProvider>
);

const Harness = () => {
  useDeviceBlockedPrompt();
  return null;
};

describe('useDeviceBlockedPrompt', () => {
  it('shows the denied dialog when the OS-block side effect fires', async () => {
    render(<Harness />, { wrapper });

    await act(async () => {
      await onDeviceOsPermissionBlockedSideEffect.apply({ permission: 'Microphone' });
    });

    await waitFor(() => expect(screen.getByText('Microphone Access Required')).toBeInTheDocument());
  });
});
