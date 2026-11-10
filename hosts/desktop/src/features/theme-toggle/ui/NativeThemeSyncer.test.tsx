// @vitest-environment happy-dom

import { render, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { type ThemePreference, themeResource } from '@/domains/application';

import { NativeThemeSyncer } from './NativeThemeSyncer';

const storePreference = (preference: ThemePreference) => {
  themeResource.instead(() => of({ preference, variant: 'light' as const, name: 'berlin' }));
};

afterEach(() => {
  vi.clearAllMocks();
  // @ts-expect-error test cleanup of the optional bridge
  delete window.App;
});

describe('NativeThemeSyncer', () => {
  it('pushes the current preference to the native theme bridge on mount', async () => {
    const setNativeTheme = vi.fn();
    // @ts-expect-error partial App bridge for the test
    window.App = { setNativeTheme };
    storePreference('dark');

    render(<NativeThemeSyncer />);

    await waitFor(() => expect(setNativeTheme).toHaveBeenCalledWith('dark'));
  });

  it('does not throw when the bridge is absent (web build)', () => {
    storePreference('system');
    expect(() => render(<NativeThemeSyncer />)).not.toThrow();
  });
});
