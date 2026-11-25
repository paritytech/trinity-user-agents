// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react';
import { of } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { type PersistedProduct, commitmentUseCase, productsResource } from '@/domains/product';

import { UpdateVersionDialog } from './UpdateVersionDialog';

// The re-pin writes are use-case methods on a plain object, spied in place: the two
// hooks are `useAction` wrappers over them, so the real hooks run and the spies see the call.
const pinProductRun = vi.spyOn(commitmentUseCase, 'pinProduct').mockResolvedValue(null);
const pinExecutableRun = vi.spyOn(commitmentUseCase, 'pinExecutable').mockResolvedValue(null);

const setup = () => {
  seedProduct();
};

const seedProduct = () => {
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test fixture, not production code
  const record = {
    baseName: 'a.dot',
    displayName: 'A',
    description: '',
    icon: { cid: '', format: 'png' },
    executables: {},
    pinned: false,
    createdAt: 1000,
    updatedAt: 1000,
  } as unknown as PersistedProduct;

  productsResource.instead(() => of([record]));
};

describe('UpdateVersionDialog', () => {
  beforeEach(() => {
    pinProductRun.mockClear();
    pinExecutableRun.mockClear();
  });

  it('re-pins the WHOLE product on confirm when no executableKind is given', async () => {
    setup();
    render(
      <TranslationProvider>
        <UpdateVersionDialog productId="a.dot" onClose={() => {}} />
      </TranslationProvider>,
    );
    fireEvent.click(await screen.findByTestId(TEST_IDS.offlineAccessUpdateConfirm));

    expect(pinProductRun).toHaveBeenCalledWith('a.dot');
    expect(pinExecutableRun).not.toHaveBeenCalled();
  });

  it('re-pins ONLY the given modality on confirm when executableKind is present', async () => {
    setup();
    render(
      <TranslationProvider>
        <UpdateVersionDialog productId="a.dot" executableKind="widget" onClose={() => {}} />
      </TranslationProvider>,
    );
    fireEvent.click(await screen.findByTestId(TEST_IDS.offlineAccessUpdateConfirm));

    expect(pinExecutableRun).toHaveBeenCalledWith({ identifier: 'a.dot', kind: 'widget' });
    expect(pinProductRun).not.toHaveBeenCalled();
  });
});
