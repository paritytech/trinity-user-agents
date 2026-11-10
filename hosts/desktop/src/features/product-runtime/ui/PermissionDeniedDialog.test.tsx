// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { type PropsWithChildren } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';

import { PermissionDeniedDialog } from './PermissionDeniedDialog';

const wrapper = ({ children }: PropsWithChildren) => <TranslationProvider locale="en">{children}</TranslationProvider>;

describe('PermissionDeniedDialog', () => {
  it('renders the Camera copy and routes to settings', async () => {
    const onOpenSettings = vi.fn();
    render(<PermissionDeniedDialog permission="Camera" onOpenSettings={onOpenSettings} onClose={vi.fn()} />, { wrapper });

    expect(screen.getByText('Camera Access Required')).toBeInTheDocument();
    await userEvent.click(screen.getByText('Device settings'));
    expect(onOpenSettings).toHaveBeenCalled();
  });
});
