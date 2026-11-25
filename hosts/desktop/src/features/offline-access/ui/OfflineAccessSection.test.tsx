// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react';
import { of } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import {
  type ExecutableKind,
  type LiveExecutable,
  type PersistedProduct,
  type SemVer,
  liveExecutableResource,
  productsResource,
} from '@/domains/product';
import { offlineAccessDialogTarget } from '../state/dialogState';

import { OfflineAccessSection } from './OfflineAccessSection';

const FROZEN_HASH = '0xaa';
const FRESH_HASH = '0xbb';

type Drift = { kind: ExecutableKind; fromVersion: SemVer; toVersion: SemVer };

// A differing contenthash is what makes a kind drift.
const setup = (drifts: Drift[]) => {
  const executables = Object.fromEntries(
    drifts.map(({ kind, fromVersion }) => [kind, { contenthash: FROZEN_HASH, appVersion: fromVersion }]),
  );

  const record = {
    baseName: 'a.dot',
    displayName: 'Hack3m',
    description: '',
    icon: { cid: '', format: 'png' },
    executables,
    pinned: true,
    createdAt: 1000,
    updatedAt: 1000,
  } as unknown as PersistedProduct;

  const live: Partial<Record<ExecutableKind, LiveExecutable>> = Object.fromEntries(
    drifts.map(({ kind, toVersion }) => [kind, { contenthash: FRESH_HASH, version: toVersion }]),
  );

  productsResource.instead(() => of([record]));
  liveExecutableResource.instead(({ kind }) => live[kind] ?? null);

  return render(
    <TranslationProvider>
      <OfflineAccessSection productId="a.dot" />
    </TranslationProvider>,
  );
};

describe('OfflineAccessSection update rows', () => {
  it('renders one row per drifted modality with the from→to version line', async () => {
    setup([
      { kind: 'app', fromVersion: [2, 1, 0], toVersion: [2, 1, 1] },
      { kind: 'widget', fromVersion: [1, 1, 0], toVersion: [1, 1, 1] },
    ]);
    expect(await screen.findAllByTestId('offline-access-update-button')).toHaveLength(2);
    expect(screen.getByText('Update from version 2.1.0 to 2.1.1 is ready to install')).toBeTruthy();
  });

  it('omits the version line when a version is all-zero (legacy)', async () => {
    setup([{ kind: 'worker', fromVersion: [0, 0, 0], toVersion: [0, 0, 0] }]);
    await screen.findByTestId('offline-access-update-button');
    expect(screen.queryByText(/Update from version/)).toBeNull();
  });

  it('opens the per-modality confirmation dialog on Update click (no inline re-pin)', async () => {
    setup([{ kind: 'app', fromVersion: [2, 1, 0], toVersion: [2, 1, 1] }]);
    fireEvent.click(await screen.findByTestId('offline-access-update-button'));
    // The dialog is feature state, so the assertion reads the real target.
    expect(offlineAccessDialogTarget.get()).toEqual({ kind: 'updateExecutable', productId: 'a.dot', executableKind: 'app' });
  });
});
