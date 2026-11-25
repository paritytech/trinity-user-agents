// @vitest-environment happy-dom

import { render, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { type ResolvedTheme, themeResource } from '@/domains/application';

import { ThemeSyncer } from './ThemeSyncer';

const setMode = vi.fn();
vi.mock('@novasamatech/tr-ui', () => ({
  useTheme: () => ({ setMode }),
}));

const showVariant = (variant: ResolvedTheme) => {
  themeResource.instead(() => of({ preference: variant, variant, name: 'berlin' }));
};

afterEach(() => {
  vi.clearAllMocks();
  document.documentElement.style.colorScheme = '';
});

describe('ThemeSyncer', () => {
  it('binds the host color-scheme to the resolved app theme', async () => {
    showVariant('dark');

    render(<ThemeSyncer />);

    await waitFor(() => expect(document.documentElement.style.colorScheme).toBe('dark'));
    expect(setMode).toHaveBeenCalledWith('dark');
  });

  it('follows a light resolved theme too', async () => {
    showVariant('light');

    render(<ThemeSyncer />);

    await waitFor(() => expect(document.documentElement.style.colorScheme).toBe('light'));
  });
});
