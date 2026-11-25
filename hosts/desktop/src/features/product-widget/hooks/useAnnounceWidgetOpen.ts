import { useEffect } from 'react';

import { onProductModalityOpenedSideEffect } from '@/domains/product';

// Announce a widget-modality open when the widget card mounts (the widget's
// "open" moment). Fires once per product; the toaster de-dupes and the domain
// check gates on pinned/drifted/undeclined.
export const useAnnounceWidgetOpen = (productId: Nullable<string>) => {
  useEffect(() => {
    if (productId) void onProductModalityOpenedSideEffect.apply({ productId, kind: 'widget' });
  }, [productId]);
};
