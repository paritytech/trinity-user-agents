import type {
  AuthState,
  ChatPlatform,
  ContactsPlatform,
  GamePlatform,
  PermissionDecision,
  PocketPlatform,
  PreimageHost,
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
import type { ActivitySink } from "../platform/activity.js";
import type { HostStorage } from "../platform/storage.js";
import type { Network } from "../platform/network.js";
import { constant, subscription } from "../platform/subscription.js";

export interface HostCallbackOptions {
  network: Network;
  storage: HostStorage;
  activity: ActivitySink;
  onAuthState: (state: AuthState) => void;
  /**
   * Handle a product's request to open `url`: inside the host for a product
   * name, in the browser for a web page. Rejects when the host will not open it.
   */
  navigate: (url: string) => Promise<void>;
  /**
   * Called when a network permission names domains, so the host can read the
   * core's saved answer for each. It only reports what was asked.
   */
  onNetworkAsked?: (domains: string[]) => void;
  /** Called once a permission prompt, or an automatic answer, has been given. */
  onPermissionDecided?: () => void;
  /** Where preimage lookups are read from. */
  preimage: Required<PreimageHost>;
  /** Where product workers' chat rooms and messages go. Without it, Chat is `Unsupported`. */
  chat?: Required<ChatPlatform>;
  /** The Pocket cards each product sees. Without it, Pocket is `Unsupported`. */
  pocket?: Required<PocketPlatform>;
  /** The contact picker and handle lookup. Without it, Contacts is `Unsupported`. */
  contacts?: Required<ContactsPlatform>;
}

/** How the log names each answer to a permission. */
const DECISION_LABELS: Record<PermissionDecision, string> = {
  Deny: "Permission denied",
  AllowOnce: "Permission allowed once",
  AllowAlways: "Permission always allowed",
};

const DECISIONS: PromptChoice<PermissionDecision>[] = [
  { label: "Deny", value: "Deny", tone: "cancel" },
  { label: "Allow once", value: "AllowOnce" },
  { label: "Always allow", value: "AllowAlways", tone: "primary" },
];

/**
 * The host side of the core: storage, chain access, and the prompts through
 * which the user answers what a product asks for.
 *
 * Live OS permission status is left out, so the core answers it
 * `Unsupported`. Chat, Pocket and Contacts are served when `chat`, `pocket`
 * and `contacts` are given. The game reminder is always served, best effort.
 */
export function createHostCallbacks(
  options: HostCallbackOptions,
): RequiredHostCallbacks {
  const {
    network,
    storage,
    activity,
    onAuthState,
    navigate,
    onNetworkAsked,
    onPermissionDecided,
    preimage,
    chat,
    pocket,
    contacts,
  } = options;
  const knownGenesis = new Set(
    network.supportedChains.chains.map((chain) =>
      chain.genesisHash.toLowerCase(),
    ),
  );
  let nextNotificationId = 1;
  let nextOperationId = 1;

  const decide = async (
    title: string,
    product: ProductContext,
    detail: string,
    category: "products" | "network",
  ) => {
    const decision = await ask({
      title,
      from: product.productId,
      fields: [{ label: "Permission", value: detail, mono: true }],
      choices: DECISIONS,
      dismissed: "Deny" as PermissionDecision,
    });
    activity({
      category,
      action: DECISION_LABELS[decision],
      subject: product.productId,
      details: [{ label: "Permission", value: detail }],
    });
    return decision;
  };

  return {
    navigation: {
      navigateTo: (url) => navigate(url),
    },

    notifications: {
      pushNotification(_request) {
        const id = nextNotificationId++;
        // The text is the product's message to the user, so it is not logged.
        activity({
          category: "products",
          action: "Notification posted",
          details: [{ label: "Id", value: String(id) }],
        });
        return Promise.resolve({ id });
      },
      cancelNotification(id) {
        activity({
          category: "products",
          action: "Notification cancelled",
          details: [{ label: "Id", value: String(id) }],
        });
        return Promise.resolve();
      },
    },

    permissions: {
      devicePermission: (product, request) =>
        decide("Device permission", product, request, "products").finally(
          onPermissionDecided,
        ),
      remotePermission: (product, request) => {
        const detail = describePermission(request.permission);
        if (request.permission.tag === "Remote")
          onNetworkAsked?.(request.permission.value.domains);
        return decide(
          "Remote permission",
          product,
          detail,
          request.permission.tag === "Remote" ? "network" : "products",
        ).finally(onPermissionDecided);
      },
    },

    // The core builds every extrinsic this host signs, as V4.
    extrinsicVersion: {
      transactionExtensionVersions: () => Promise.resolve({ chains: [] }),
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
        activity({
          category: "session",
          action: "Core session",
          subject: state.tag,
          ...(state.tag === "LoginFailed"
            ? { note: state.value.reason, failed: true }
            : {}),
        });
        onAuthState(state);
      },
    },

    userConfirmation: {
      confirmPermission(review) {
        const { title, product, fields } = describeReview(review);
        return ask({
          title,
          from: product,
          fields,
          choices: DECISIONS,
          dismissed: "Deny" as PermissionDecision,
        });
      },
      confirmUserAction(review) {
        const { title, product, fields } = describeReview(review);
        return ask({
          title,
          from: product,
          fields,
          choices: [
            { label: "Reject", value: false, tone: "cancel" },
            { label: "Approve", value: true, tone: "primary" },
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

    // The core submits preimages to Bulletin itself; the host only reads them.
    preimage,

    // A worker this host runs stays up while its wallet is signed in, so no
    // operation needs to keep it alive. Ids stay unique across products,
    // which satisfies the core's per-product uniqueness requirement.
    productOperations: {
      beginOperation: () => Promise.resolve({ id: nextOperationId++ }),
      endOperation: () => Promise.resolve(),
    },
    game: createGameReminders(activity),
    ...(chat ? { chat } : {}),
    ...(pocket ? { pocket } : {}),
    ...(contacts ? { contacts } : {}),
  };
}

/** The longest delay `setTimeout` keeps; a longer one fires at once. */
const MAX_TIMER_DELAY_MS = 2_147_483_647;

/**
 * The next-game reminder of each product. The core calls it for the game
 * product only, and only for a start in the future.
 *
 * Best effort: a reminder lives in this tab and is lost when the tab closes or
 * reloads. The host keeps one per product, logs it, and logs it again and
 * drops it when the game starts.
 */
export function createGameReminders(
  activity: ActivitySink,
): Required<GamePlatform> {
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  const drop = (productId: string) => {
    clearTimeout(timers.get(productId));
    return timers.delete(productId);
  };

  const arm = (productId: string, startsAt: number) => {
    const delay = startsAt - Date.now();
    if (delay <= 0) {
      timers.delete(productId);
      activity({
        category: "products",
        action: "Game starting",
        subject: productId,
      });
      return;
    }
    timers.set(
      productId,
      setTimeout(
        () => arm(productId, startsAt),
        Math.min(delay, MAX_TIMER_DELAY_MS),
      ),
    );
  };

  return {
    scheduleGameReminder(product, startsAt) {
      const start = Number(startsAt);
      drop(product.productId);
      activity({
        category: "products",
        action: "Game reminder set",
        subject: product.productId,
        details: [{ label: "Starts", value: new Date(start).toISOString() }],
      });
      arm(product.productId, start);
      return Promise.resolve();
    },
    cancelGameReminder(product) {
      if (drop(product.productId))
        activity({
          category: "products",
          action: "Game reminder cancelled",
          subject: product.productId,
        });
      return Promise.resolve();
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
