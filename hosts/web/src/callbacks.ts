import type {
  AuthState,
  PermissionDecision,
  ProductContext,
  RequiredHostCallbacks,
} from "@parity/truapi-host";
import type {
  HostLocaleSubscribeItem,
  HostThemeSubscribeItem,
  RemotePermission,
} from "@parity/truapi";
import { ask, type PromptChoice } from "./prompt.js";
import { describeReview, formatValue } from "./reviews.js";
import type { HostStorage } from "./storage.js";
import type { Network } from "./network.js";
import { constant, subscription } from "./subscription.js";

export interface HostCallbackOptions {
  network: Network;
  storage: HostStorage;
  log: (text: string) => void;
  onAuthState: (state: AuthState) => void;
  /** A `polkadot://` destination, which has no `https://` form to open. */
  onProductNavigation: (url: string) => void;
  /**
   * Called when a network permission names domains, so the host can read the
   * core's saved answer for each. It only reports what was asked.
   */
  onNetworkAsked?: (domains: string[]) => void;
  /** Called once a permission prompt, or an automatic answer, has been given. */
  onPermissionDecided?: () => void;
}

const DECISIONS: PromptChoice<PermissionDecision>[] = [
  { label: "Deny", value: "Deny" },
  { label: "Allow once", value: "AllowOnce" },
  { label: "Always allow", value: "AllowAlways", primary: true },
];

/**
 * The host side of the core: storage, chain access, and the prompts through
 * which the user answers what a product asks for.
 *
 * Chat, contacts, Pocket and live OS permission status are left out, so the
 * core answers those product calls `Unsupported`.
 */
export function createHostCallbacks(
  options: HostCallbackOptions,
): RequiredHostCallbacks {
  const {
    network,
    storage,
    log,
    onAuthState,
    onProductNavigation,
    onNetworkAsked,
    onPermissionDecided,
  } = options;
  const knownGenesis = new Set(
    network.supportedChains.chains.map((chain) =>
      chain.genesisHash.toLowerCase(),
    ),
  );
  let nextNotificationId = 1;
  let nextOperationId = 1;

  const decide = (title: string, product: ProductContext, detail: string) =>
    ask({
      title,
      fields: [
        { label: "Requested by", value: product.productId },
        { label: "Permission", value: detail, mono: true },
      ],
      choices: DECISIONS,
      dismissed: "Deny" as PermissionDecision,
    });

  return {
    navigation: {
      navigateTo(url) {
        log(`navigate to ${url}`);
        if (url.startsWith("polkadot://")) onProductNavigation(url);
        else window.open(url, "_blank", "noopener,noreferrer");
        return Promise.resolve();
      },
    },

    notifications: {
      pushNotification(request) {
        const id = nextNotificationId++;
        log(`notification ${id}: ${request.text}`);
        return Promise.resolve({ id });
      },
      cancelNotification(id) {
        log(`notification ${id} cancelled`);
        return Promise.resolve();
      },
    },

    permissions: {
      devicePermission: (product, request) =>
        decide("Device permission", product, request).finally(
          onPermissionDecided,
        ),
      remotePermission: (product, request) => {
        const detail = describePermission(request.permission);
        if (request.permission.tag === "Remote")
          onNetworkAsked?.(request.permission.value.domains);
        return decide("Remote permission", product, detail).finally(
          onPermissionDecided,
        );
      },
    },

    features: {
      featureSupported: (request) =>
        Promise.resolve({
          supported: knownGenesis.has(request.value.genesisHash.toLowerCase()),
        }),
      supportedChains: () => Promise.resolve(network.supportedChains),
    },

    productStorage: storage.product,
    coreStorage: storage.core,
    chain: network.chain,

    auth: {
      authStateChanged(state) {
        log(`auth state ${state.tag}`);
        onAuthState(state);
      },
    },

    userConfirmation: {
      confirmPermission(review) {
        const { title, fields } = describeReview(review);
        return ask({
          title,
          fields,
          choices: DECISIONS,
          dismissed: "Deny" as PermissionDecision,
        });
      },
      confirmUserAction(review) {
        const { title, fields } = describeReview(review);
        return ask({
          title,
          subtitle: "The product waits until you decide.",
          fields,
          choices: [
            { label: "Reject", value: false },
            { label: "Approve", value: true, primary: true },
          ],
          dismissed: false,
        });
      },
    },

    theme: {
      subscribeTheme: () =>
        subscription<HostThemeSubscribeItem>((emit) => {
          const dark = window.matchMedia("(prefers-color-scheme: dark)");
          const current = () =>
            emit({
              name: { tag: "Default" },
              variant: dark.matches ? "Dark" : "Light",
            });
          current();
          dark.addEventListener("change", current);
          return () => dark.removeEventListener("change", current);
        }),
    },

    locale: {
      subscribeLocale: () =>
        constant<HostLocaleSubscribeItem>({ languageTag: navigator.language }),
    },

    // No preimage network is reachable from this host, so every lookup is an
    // honest miss. The core submits preimages to Bulletin itself.
    preimage: {
      lookupPreimage: () => constant<Uint8Array | undefined>(undefined),
    },

    // Worker products are not served here, so no operation keeps anything
    // alive. Ids stay unique across products, which satisfies the core's
    // per-product uniqueness requirement.
    productOperations: {
      beginOperation: () => Promise.resolve({ id: nextOperationId++ }),
      endOperation: () => Promise.resolve(),
    },
  };
}

function describePermission(permission: RemotePermission): string {
  if (permission.tag === "Remote")
    return `Network access to ${permission.value.domains.join(", ")}`;
  return permission.value === undefined
    ? permission.tag
    : formatValue(permission);
}
