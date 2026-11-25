import { firstValueFrom } from 'rxjs';
import { afterEach, describe, expect, it } from 'vitest';

import { openNativeAddToDashboardDialog } from './addableDashboardCards';
import { type AddableDashboardCard, addableDashboardCardsPipeline } from './di';
import { addToDashboardDialogTarget } from './state/addToDashboardDialog';

// The addable set is a DI pipeline, so a case states it by registering a handler —
// the same seam the features use — instead of standing in for the pipeline itself.
const registerAddables = (...cards: AddableDashboardCard[]) => {
  addableDashboardCardsPipeline.registerHandler({ available: () => true, body: value => value.concat(cards) });
};

afterEach(() => {
  addableDashboardCardsPipeline.resetHandlers();
});

describe('openNativeAddToDashboardDialog', () => {
  it('opens the dialog for a native addable id without resolving a chain product', async () => {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- only gridId is read
    registerAddables({ gridId: 'chat' } as AddableDashboardCard);

    expect(openNativeAddToDashboardDialog('chat')).toBe(true);

    await expect(firstValueFrom(addToDashboardDialogTarget.value$)).resolves.toBe('chat');
  });

  it('returns false for ids that are not native addable entries', async () => {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- only gridId is read
    registerAddables({ gridId: 'chat' } as AddableDashboardCard);

    expect(openNativeAddToDashboardDialog('app.dot')).toBe(false);

    await expect(firstValueFrom(addToDashboardDialogTarget.value$)).resolves.toBeNull();
  });
});
