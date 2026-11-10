import { useCallback } from 'react';

import { type DashboardAddOutcome, type DashboardAddRequest, dashboardCardAddTransformer } from '../di';

// Single entry point for "add this content to the dashboard". Dispatches the
// request through `dashboardCardAddTransformer`, which resolves to the one
// handler registered for the request's `kind` (each provider returns `null` for
// every other kind). The matched handler returns a `Promise<DashboardAddOutcome>`;
// when no handler matches, the transformer yields `null` and we report failure.
export function useAddDashboardContent() {
  const addContent = useCallback(async (request: DashboardAddRequest): Promise<DashboardAddOutcome> => {
    const pending = dashboardCardAddTransformer(request);
    if (!pending) return { ok: false };
    return pending;
  }, []);

  return { addContent };
}
