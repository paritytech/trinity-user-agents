// @vitest-environment happy-dom
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { SUPPORTED_LOCALES, TranslationProvider, readLocale } from '@/shared/translation';

import { LanguageSettings } from './LanguageSettings';

const renderPage = () =>
  render(
    <TranslationProvider>
      <LanguageSettings />
    </TranslationProvider>,
  );

describe('LanguageSettings', () => {
  it('renders one option per supported locale', () => {
    renderPage();

    expect(screen.getAllByRole('radio')).toHaveLength(SUPPORTED_LOCALES.length);
  });

  it('shows the English name and the endonym for each option', () => {
    renderPage();

    expect(screen.getByRole('radio', { name: /Spanish/ })).toBeInTheDocument();
    expect(screen.getByText('Español')).toBeInTheDocument();
  });

  it('marks the persisted locale as checked', () => {
    localStorage.setItem('polkadot_locale', 'ja');

    renderPage();

    expect(screen.getByRole('radio', { name: /Japanese/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /English/ })).not.toBeChecked();
  });

  it('persists the locale the user selects', async () => {
    renderPage();

    await userEvent.click(screen.getByRole('radio', { name: /German/ }));

    expect(readLocale()).toBe('de');
    expect(screen.getByRole('radio', { name: /German/ })).toBeChecked();
  });
});
