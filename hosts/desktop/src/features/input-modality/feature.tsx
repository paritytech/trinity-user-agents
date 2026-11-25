import { createFeature } from '@/shared/feature';
import { persistentSlot } from '@/features/app-shell';
import { focusAddressBarSideEffect } from '@/features/browser';

import { inputSurfaceInitialText, inputSurfaceOpen } from './state/surface';
import { InputSpotlight } from './ui/InputSpotlight';

export const inputModalityFeature = createFeature({
  name: 'product/input-modality',
});

inputModalityFeature.inject(persistentSlot, () => <InputSpotlight />);

// The browser feature publishes "the user wants to type an address" — pressing the
// bar, Cmd/Ctrl+K, opening a new tab. All three now mean the same thing, and this
// is the one handler that decides what it is.
inputModalityFeature.inject(focusAddressBarSideEffect, ({ initialText }) => {
  // Set before the flag: the surface mounts on the flag and reads the text once.
  inputSurfaceInitialText.set(initialText ?? '');
  inputSurfaceOpen.set(true);
});

// ---------------------------------------------------------------------------
// PARKED — product querying. Restore with the rest of the `PARKED` blocks in
// this feature. The surface asks nobody while its context set is empty, so this
// would answer nothing even if it were registered; it stays commented so the
