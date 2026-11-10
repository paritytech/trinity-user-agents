import { createDialogTarget } from '@/shared/rxstate';

const dialog = createDialogTarget<string>();

export const addToDashboardDialogTarget = dialog.target;
export const openAddToDashboardDialog = dialog.open;
export const closeAddToDashboardDialog = dialog.close;

/** The product the Add-to-Dashboard dialog is open for, or `null` when closed. */
export function getAddToDashboardDialogTarget(): string | null {
  return dialog.target.get();
}
