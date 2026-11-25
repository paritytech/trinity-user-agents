// @vitest-environment happy-dom

import { render, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import { onProductModalityOpenedSideEffect, updatesUseCase } from '@/domains/product';
// eslint-disable-next-line boundaries/dependencies -- the test observes the write where it lands; the barrel exposes no seam for it
import { declinedUpdatesRepository } from '@/domains/product/product/declined-updates/repository';
import { offlineAccessDialogTarget } from '../state/dialogState';

import { ModalityUpdateToaster } from './ModalityUpdateToaster';

const { toastMock } = vi.hoisted(() => ({ toastMock: vi.fn() }));

vi.mock('@novasamatech/tr-ui', () => ({ toast: toastMock }));

// `checkModalityUpdate` reaches `productDb` and the declined-updates repository directly,
// so the use case is spied in place; the decline is observed where it lands, on the
// repository, so the real `useDeclineUpdate` and its TLD normalisation run.
const checkModalityUpdateMock = vi.spyOn(updatesUseCase, 'checkModalityUpdate');
const recordDecline = vi.spyOn(declinedUpdatesRepository, 'record').mockResolvedValue();

const renderToaster = () =>
  render(
    <TranslationProvider>
      <ModalityUpdateToaster />
    </TranslationProvider>,
  );

afterEach(() => {
  vi.clearAllMocks();
});

describe('ModalityUpdateToaster', () => {
  it('raises a persistent Update toast when the opened modality has an undeclined update', async () => {
    checkModalityUpdateMock.mockResolvedValue({ contenthash: '0xnew', version: [1, 0, 1] });
    renderToaster();

    await onProductModalityOpenedSideEffect.apply({ productId: 'app.dot', kind: 'app' });

    expect(checkModalityUpdateMock).toHaveBeenCalledWith({ baseName: 'app.dot', kind: 'app' });
    expect(toastMock).toHaveBeenCalledTimes(1);
    const [, options] = toastMock.mock.calls[0]!;
    expect(options.id).toBe('modality-update:app.dot#app');
    expect(options.duration).toBe(Infinity);
  });

  it('Update action opens the per-modality confirm dialog', async () => {
    checkModalityUpdateMock.mockResolvedValue({ contenthash: '0xnew', version: [1, 0, 1] });
    renderToaster();
    await onProductModalityOpenedSideEffect.apply({ productId: 'app.dot', kind: 'widget' });

    const [, options] = toastMock.mock.calls[0]!;
    options.action.onClick();
    // The dialog is feature state, so the assertion reads the real target.
    expect(offlineAccessDialogTarget.get()).toEqual({ kind: 'updateExecutable', productId: 'app.dot', executableKind: 'widget' });
  });

  it('dismiss records a decline for that exact version', async () => {
    checkModalityUpdateMock.mockResolvedValue({ contenthash: '0xnew', version: [1, 0, 1] });
    renderToaster();
    await onProductModalityOpenedSideEffect.apply({ productId: 'app.dot', kind: 'worker' });

    const [, options] = toastMock.mock.calls[0]!;
    options.onDismiss();
    // The decline resolves the active TLD first, so the write lands a tick later.
    await waitFor(() =>
      expect(recordDecline).toHaveBeenCalledWith({
        baseName: 'app.dot',
        kind: 'worker',
        contenthash: '0xnew',
        version: [1, 0, 1],
      }),
    );
  });

  it('raises no toast when the modality has no update', async () => {
    checkModalityUpdateMock.mockResolvedValue(null);
    renderToaster();
    await onProductModalityOpenedSideEffect.apply({ productId: 'app.dot', kind: 'app' });
    expect(toastMock).not.toHaveBeenCalled();
  });
});
