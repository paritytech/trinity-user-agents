import { useEffect } from 'react';

import { dotNsService, dotNsUseCase } from '@/domains/product';
import { openDotNsUrlSideEffect } from '../di';

/**
 * Answers the main process when a product webview calls `window.open`.
 *
 * Main cannot tell a product URL from any other: that depends on the active
 * network's dotNS TLD and its `.li` gateway aliases, neither of which the main
 * process tracks. So it asks here, and a URL that resolves to a product opens as a
 * tab instead of leaving for the system browser. Answering `false` hands the URL
 * back to main's external path, which keeps the per-product permission gate.
 *
 * The TLD is read per call, not captured: it follows the active network, and a
 * stale one would send every in-app target to the browser.
 */
export const useSandboxWindowOpenRouting = () => {
  useEffect(() => {
    return window.App.onSandboxWindowOpen(async ({ url }) => {
      const dotNsUrl = dotNsService.parseDotNsDomain(url, await dotNsUseCase.getActiveTld());
      if (!dotNsUrl) return false;

      void openDotNsUrlSideEffect.apply(dotNsUrl);

      return true;
    });
  }, []);
};
