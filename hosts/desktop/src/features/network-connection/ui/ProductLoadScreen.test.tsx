// @vitest-environment happy-dom

import { act, render, screen } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { CONNECTION_TIMEOUT_THRESHOLD_MS, SLOW_CONNECTION_THRESHOLD_MS, connectionSettingsResource } from '@/domains/network';

// The spinner animates through requestAnimationFrame and leaves nothing in the DOM that
// says whether it is running, so the screen stands in only to expose that one flag.
vi.mock(import('@/shared/components'), async importOriginal => ({
  ...(await importOriginal()),
  ProductLoadingScreen: ({ spinnerAnimated, message }: { spinnerAnimated?: boolean; message?: unknown }) => (
    <div data-testid="screen" data-animated={String(spinnerAnimated)}>
      {message as never}
    </div>
  ),
}));

import { ProductLoadScreen } from './ProductLoadScreen';

const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
const animated = () => screen.getByTestId('screen').getAttribute('data-animated');

// The real notice renders each phase differently: the plain phrase while within budget,
// the switch once slow, the reload once failed.
const phase = () => {
  if (screen.queryByTestId(TEST_IDS.productLoadReloadButton)) return 'timeout';
  if (screen.queryByTestId(TEST_IDS.productLoadSwitchButton)) return 'slow';

  return 'loading';
};

const renderScreen = () =>
  render(
    <TranslationProvider>
      <ProductLoadScreen identifier="app.dot" />
    </TranslationProvider>,
  );

describe('ProductLoadScreen', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // The slow phase only has something to show while the light client is preferred.
    connectionSettingsResource.instead(() => of({ preference: 'light-client', overrides: {} }));
  });
  afterEach(() => vi.useRealTimers());

  it('walks the notice through the phases as the load drags on', () => {
    renderScreen();

    expect(phase()).toBe('loading');

    advance(SLOW_CONNECTION_THRESHOLD_MS);

    expect(phase()).toBe('slow');

    advance(CONNECTION_TIMEOUT_THRESHOLD_MS - SLOW_CONNECTION_THRESHOLD_MS);

    expect(phase()).toBe('timeout');
  });

  it('stops the spinner once the load has timed out, and not before', () => {
    renderScreen();

    expect(animated()).toBe('true');

    advance(SLOW_CONNECTION_THRESHOLD_MS);

    expect(animated()).toBe('true');

    advance(CONNECTION_TIMEOUT_THRESHOLD_MS - SLOW_CONNECTION_THRESHOLD_MS);

    expect(animated()).toBe('false');
  });
});
