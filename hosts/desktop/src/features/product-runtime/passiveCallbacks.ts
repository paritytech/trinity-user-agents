import { type HostThemeSubscribeItem, scale } from '@parity/truapi';
import { type RequiredHostCallbacks } from '@parity/truapi-host';
import { err, ok } from 'neverthrow';
import { distinctUntilChanged, map } from 'rxjs';

import { isElectron } from '@/shared/env';
import { observableToAsyncIterable } from '@/shared/rxstate';
import { watchLocale } from '@/shared/translation';
import { themeUseCase } from '@/domains/application';
import { ipfsService, ipfsUseCase } from '@/domains/network';
import { dotNsService, dotNsUseCase, permissionsService } from '@/domains/product';

type PassiveCallbacks = Pick<
  RequiredHostCallbacks,
  'navigation' | 'notifications' | 'theme' | 'locale' | 'permissionStatus' | 'preimage'
>;

const NOTIFICATION_TEXT_LIMIT = 200;
const LOOKUP_POLL_INTERVAL_MS = 10_000;
const LOOKUP_FETCH_TIMEOUT_MS = 8_000;
const LOOKUP_FAILURES_BEFORE_GIVING_UP = 3;

// The core's notification callbacks carry no `ProductContext` — unlike `ChatPlatform`,
// whose methods do — so the host cannot tell which product raised a notification.
// Everything is filed under one host-level id, which costs the per-product title and
// per-product activation routing the hand-written binding had. See
// `docs/_plans/truapi-upstream-issues.md` § 6.
const HOST_NOTIFICATION_SCOPE = 'polkadot-desktop';

function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Opens an in-app route.
 *
 * Injected rather than imported: the app router carries the whole route tree, and a
 * module that reaches for it cannot be built or tested without it. The composition
 * point supplies the one method this needs.
 */
export type NavigateToProduct = (params: { identifier: string; route: string | undefined }) => void;

/**
 * The six host callbacks that answer without asking the user anything: where a URL
 * opens, how a notification is delivered, what the theme and locale are, whether the
 * OS still allows a device capability the user already granted, and where preimage
 * bytes come from.
 */
export function createPassiveCallbacks(navigate: NavigateToProduct): PassiveCallbacks {
  return {
    navigation: {
      // The core has already categorised the URL and taken the remote-access
      // decision through `permissions.remotePermission`, which is product-scoped;
      // by contract this callback only hands the URL onward. Re-checking here is
      // not possible anyway — `navigateTo` carries no product, so the host has
      // nothing to check the grant against.
      navigateTo: async url => {
        // Resolved per call rather than captured: the TLD follows the active
        // network, and a stale one silently sends every in-app target to the
        // browser. A non-null parse is already a validated navigation target
        // (`asSafeNavigationTarget`), so no second dotNS check is needed.
        const tld = await dotNsUseCase.getActiveTld();
        const dotNsUrl = dotNsService.parseDotNsDomain(url, tld);

        if (dotNsUrl) {
          navigate({ identifier: dotNsUrl.identifier, route: dotNsUrl.pathname || undefined });

          return;
        }

        window.open(url, '_blank');
      },
    },

    notifications: {
      pushNotification: async notification => {
        const result = await window.App.scheduleNotification({
          productId: HOST_NOTIFICATION_SCOPE,
          title: 'Polkadot Desktop',
          text: notification.text.trim().slice(0, NOTIFICATION_TEXT_LIMIT),
          deeplink: notification.deeplink ?? null,
          scheduledAt: notification.scheduledAt === undefined ? null : Number(notification.scheduledAt),
        });

        if (!result.ok) throw new Error(result.reason ?? 'Notification could not be scheduled');

        return { id: result.id };
      },

      // Idempotent by contract: cancelling an already-fired or unknown id succeeds.
      cancelNotification: async id => {
        await window.App.cancelNotification(HOST_NOTIFICATION_SCOPE, id).catch(() => undefined);
      },
    },

    theme: {
      // The core drives this as a generator, so it can't use a hook. `theme$` re-emits
      // on every theme write; `distinctUntilChanged` keeps byte-identical items off the
      // wire, the job the old poll's `last` compare did.
      subscribeTheme: () =>
        observableToAsyncIterable(
          themeUseCase.watchTheme().pipe(
            map((theme): HostThemeSubscribeItem => ({
              name: { tag: 'Custom', value: theme.name },
              variant: theme.variant === 'dark' ? 'Dark' : 'Light',
            })),
            distinctUntilChanged((a, b) => a.name.value === b.name.value && a.variant === b.variant),
            map(item => ok(item)),
          ),
        ),
    },

    locale: {
      // `Locale` ids are already BCP 47 tags, so the id is the wire value.
      subscribeLocale: () => observableToAsyncIterable(watchLocale().pipe(map(locale => ok({ languageTag: locale })))),
    },

    permissionStatus: {
      // Must not prompt: the core only wants to know whether the OS currently refuses a
      // capability the user already granted the product. `isOsGatedDevicePermission` is
      // the same gate the Electron permission-request path applies, so the two paths
      // cannot disagree about which permissions the OS decides.
      devicePermissionStatus: async request => {
        if (!permissionsService.isOsGatedDevicePermission(request) || !isElectron()) return 'NotApplicable';

        return permissionsService.toDevicePermissionStatus(await window.App.getSystemDevicePermissionStatus(request));
      },
    },

    preimage: {
      lookupPreimage: async function* (key) {
        let consecutiveFailures = 0;
        let delivered = false;

        while (consecutiveFailures < LOOKUP_FAILURES_BEFORE_GIVING_UP) {
          try {
            const data = await ipfsUseCase.fetchRaw(ipfsService.toIpfsCid(scale.bytesToHex(key)), {
              timeoutMs: LOOKUP_FETCH_TIMEOUT_MS,
            });

            if (data) {
              yield ok(data);

              return;
            }

            // Report the miss once so the core stops waiting on a first value.
            if (!delivered) {
              delivered = true;
              yield ok(undefined);
            }
            consecutiveFailures++;
          } catch {
            if (!delivered) {
              delivered = true;
              yield ok(undefined);
            }
            consecutiveFailures++;
          }

          await delay(LOOKUP_POLL_INTERVAL_MS);
        }

        yield err({ reason: 'Preimage lookup failed repeatedly' });
      },
    },
  };
}
