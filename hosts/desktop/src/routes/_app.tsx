import { Outlet, createFileRoute, useNavigate, useRouterState } from '@tanstack/react-router';
import { useEffect } from 'react';

import { ConfirmationProvider, Web3SummitEndedScreen } from '@/shared/components';
import { useSideEffect } from '@/shared/di';
import { useBodyLockTracer, useResetAppData, useScrollLockGuard } from '@/shared/hooks';
import { clearAllOutboxRecords } from '@/domains/chat';
import { dotNsService, dotNsUseCase, recordRecentProduct } from '@/domains/product';
import { P2PChatBinding } from '@/aggregates/p2p-chat';
import { truapiRuntimeUseCase, useLoggingOut } from '@/aggregates/truapi-runtime';
import { AppShell } from '@/features/app-shell';
import { Browser, openDotNsUrlSideEffect } from '@/features/browser';
import { TruapiPromptsBinding } from '@/features/product-runtime';
import { UpdateCheckProvider } from '@/features/update-check';
import { RemotePermissionPromptHost } from '@/widgets/Permission';

import { PageLoadingState } from '@/PageLoadingState';
import { bootstrap } from '@/bootstrap';

/**
 * Handles deeplink navigation from the Electron main process. The main process
 * sends the raw `polkadot://` URL via `protocol-open`; parse it and navigate
 * using typed TanStack Router params. `notifyDeepLinkReady` on mount flushes any
 * cold-start deeplink stored before the SPA mounted. No-op in the web build.
 */
const useDeepLinkNavigation = () => {
  const navigate = useNavigate();

  useEffect(() => {
    const cleanup = window.App?.onProtocolOpen(url => {
      // Resolved per URL rather than captured: `notifyDeepLinkReady` below
      // flushes the cold-start deeplink the moment this effect runs, so any TLD
      // held in a ref would still be the fallback and would drop the URL on
      // every network but mainnet.
      void dotNsUseCase
        .getActiveTld()
        .then(tld => {
          const dotNsUrl = dotNsService.parseDotNsDomain(url, tld);
          if (!dotNsUrl) return;

          navigate({ to: '/product/$id/{-$route}', params: { id: dotNsUrl.identifier, route: dotNsUrl.pathname } });
        })
        .catch(() => {
          console.warn('[deeplink] could not read the network TLD; dropped', url);
        });
    });

    // Signal readiness AFTER registering the listener so the flush from the
    // main process arrives after the listener is in place.
    window.App?.notifyDeepLinkReady();

    return cleanup;
  }, [navigate]);
};

/**
 * "The user opened a product" — the one place that turns that into navigation,
 * and therefore the one place that should remember it.
 *
 * Recording lived at the single call site that raised this, so a product opened
 * any other way was never remembered. Here it costs a caller nothing: whoever
 * raises the side effect gets the history entry, which is what makes the recents
 * list true rather than true-for-one-button.
 */
const useOpenDotNsUrlNavigation = () => {
  const navigate = useNavigate();

  useSideEffect(openDotNsUrlSideEffect, ({ identifier, pathname }) => {
    recordRecentProduct(identifier);
    void navigate({ to: '/product/$id/{-$route}', params: { id: identifier, route: pathname } });
  });
};

const AppLayout = () => {
  const outcome = Route.useLoaderData();
  const { location } = useRouterState();
  const loggingOut = useLoggingOut();
  const isOnboarding = location.pathname === '/onboarding';
  const isProductRoute = location.pathname.startsWith('/product/');

  // The SDK-owned device + user identity live in `polkadot_*` localStorage,
  // which `useResetAppData`'s sweep already wipes; we only need to flip the
  // reactive handle so live consumers tear down before the reload. The chat
  // outbox records live under their own `p2p-chat-outbox:` prefix, which the
  // `polkadot_*` sweep does NOT cover — clear them here or a reset leaves
  // orphan per-peer queues behind.
  useResetAppData(() => {
    // The core owns the session now, so a reset has to tell it to drop the
    // pairing — wiping localStorage alone would leave the core still Connected.
    void truapiRuntimeUseCase.disconnectSession();
    clearAllOutboxRecords();
  });
  useScrollLockGuard();
  useBodyLockTracer();
  useDeepLinkNavigation();
  useOpenDotNsUrlNavigation();

  // Cover the whole logout teardown with the same splash the boot uses. Returning it
  // in place of the shell means the async wipe's signed-out renders and the hash-reset
  // to onboarding never reach the screen — the reload replaces this splash outright.
  if (loggingOut) {
    return <PageLoadingState />;
  }

  // `ConfirmationProvider` wraps everything — including the two loading gates —
  // because `TruapiPromptsBinding` renders the core's prompts into it, and the core
  // holds every prompt until that binding has mounted.
  return (
    <ConfirmationProvider>
      <TruapiPromptsBinding />
      {outcome.status === 'w3s-ended' ? (
        <Web3SummitEndedScreen />
      ) : (
        <>
          <UpdateCheckProvider>
            {isOnboarding ? (
              <Outlet />
            ) : (
              <>
                <P2PChatBinding />
                <AppShell>
                  <div className="h-full w-full" style={{ display: isProductRoute ? 'none' : undefined }}>
                    <Outlet />
                  </div>
                  <div className="h-full w-full" style={{ display: isProductRoute ? undefined : 'none' }}>
                    <Browser />
                  </div>
                </AppShell>
              </>
            )}
          </UpdateCheckProvider>
          <RemotePermissionPromptHost />
        </>
      )}
    </ConfirmationProvider>
  );
};

/**
 * Pathless layout (`_app`) wrapping the whole main app. Its loader runs
 * `bootstrap()` (so bootstrap is scoped to the app, not the sibling `/call`
 * route), and its component holds the main-app provider shell + the browser
 * shell. Existing URLs are unchanged — the `_app` segment is pathless.
 */
export const Route = createFileRoute('/_app')({
  loader: () => bootstrap(),
  pendingComponent: PageLoadingState,
  // bootstrap rejects only when Remote Config is unavailable (offline fresh
  // install / missing creds). The router's defaultErrorComponent renders the retry screen.
  component: AppLayout,
});
