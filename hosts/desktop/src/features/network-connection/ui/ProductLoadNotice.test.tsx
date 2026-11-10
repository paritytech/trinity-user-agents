// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type ConnectionPreference, connectionSettingsResource } from '@/domains/network';
// eslint-disable-next-line boundaries/dependencies -- the test observes the write where it lands; the barrel exposes no seam for it
import { connectionRepository } from '@/domains/network/connection/repository';
import { onProductRefreshRequestedSideEffect } from '@/aggregates/product-loading';
import { type ProductLoadPhase } from '../hooks/useProductLoadPhase';

// The refresh request is a DI side effect: a handler registered on it observes the
// real fan-out, so nothing has to stand in for the aggregate that declares it.
const refreshApplyMock = vi.fn();

// The preference write lands in the repository, a plain object spied in place, so the
// real hook and mutation run up to that point and the action is observed there.
const setPreferenceMock = vi.spyOn(connectionRepository, 'setPreference').mockReturnValue(of(undefined) as never);

import { ProductLoadNotice } from './ProductLoadNotice';

const renderNotice = (phase: ProductLoadPhase) =>
  render(
    <TranslationProvider>
      <ProductLoadNotice identifier="app.dot" phase={phase} />
    </TranslationProvider>,
  );

const PHRASE = 'Reaching out app.dot';

function preference(value: ConnectionPreference) {
  connectionSettingsResource.instead(() => of({ preference: value, overrides: {} }));
}

describe('ProductLoadNotice', () => {
  beforeEach(() => {
    preference('light-client');
    onProductRefreshRequestedSideEffect.registerHandler({ available: () => true, body: refreshApplyMock });
  });

  afterEach(() => {
    onProductRefreshRequestedSideEffect.resetHandlers();
    vi.clearAllMocks();
  });

  it('shows the plain loading phrase while the load is still within budget', () => {
    renderNotice('loading');

    expect(screen.getByText(PHRASE)).toBeTruthy();
    expect(screen.queryByTestId(TEST_IDS.productLoadNotice)).toBeNull();
  });

  it('keeps the phrase on a slow load when the switch has nothing to offer', () => {
    preference('rpc');
    renderNotice('slow');

    expect(screen.getByText(PHRASE)).toBeTruthy();
  });

  it('offers only the switch while slow', () => {
    renderNotice('slow');

    expect(screen.getByText('Loading is taking longer than usual')).toBeTruthy();
    expect(screen.getByTestId(TEST_IDS.productLoadSwitchButton)).toBeTruthy();
    expect(screen.queryByTestId(TEST_IDS.productLoadReloadButton)).toBeNull();
  });

  it('adds the reload once the load has failed', () => {
    renderNotice('timeout');

    expect(screen.getByText('Couldn’t load this page')).toBeTruthy();
    expect(screen.getByTestId(TEST_IDS.productLoadReloadButton)).toBeTruthy();
    expect(screen.getByTestId(TEST_IDS.productLoadSwitchButton)).toBeTruthy();
  });

  it('reloads the product without restarting the app', () => {
    renderNotice('timeout');
    fireEvent.click(screen.getByTestId(TEST_IDS.productLoadReloadButton));

    expect(refreshApplyMock).toHaveBeenCalledWith({ identifier: 'app.dot' });
    expect(setPreferenceMock).not.toHaveBeenCalled();
  });

  it('offers the reload alone when the switch has nothing to offer', () => {
    preference('rpc');
    renderNotice('timeout');

    expect(screen.getByText('The connection took too long. Try reloading the page.')).toBeTruthy();
    expect(screen.getByTestId(TEST_IDS.productLoadReloadButton)).toBeTruthy();
    expect(screen.queryByTestId(TEST_IDS.productLoadSwitchButton)).toBeNull();
  });

  // The open connections move onto the RPC nodes in place, so the write is the
  // whole action — nothing reloads the app out from under the pending load.
  it('switches to trusted providers on the spot', () => {
    renderNotice('slow');
    fireEvent.click(screen.getByTestId(TEST_IDS.productLoadSwitchButton));

    expect(setPreferenceMock).toHaveBeenCalledWith('rpc');
  });
});
