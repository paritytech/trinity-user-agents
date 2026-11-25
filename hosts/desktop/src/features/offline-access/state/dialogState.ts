import { createDialogTarget } from '@/shared/rxstate';
import { type ExecutableKind } from '@/domains/product';

// `update` re-pins the whole product; `updateExecutable` re-pins a single
// modality (carries which kind). Both gate the re-download behind a confirm dialog.
type DialogState =
  | { kind: 'enable'; productId: string }
  | { kind: 'remove'; productId: string }
  | { kind: 'update'; productId: string }
  | { kind: 'updateExecutable'; productId: string; executableKind: ExecutableKind };

const dialog = createDialogTarget<DialogState>();

export const offlineAccessDialogTarget = dialog.target;
export const openOfflineAccessDialog = dialog.open;
export const closeOfflineAccessDialog = dialog.close;
