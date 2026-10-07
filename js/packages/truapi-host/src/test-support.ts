import type { RequiredHostCallbacks } from "./generated/host-callbacks.js";

/** `HostCallbacks` with every optional member required, for exhaustive test fixtures. */
export type CompleteHostCallbacks = RequiredHostCallbacks;

type HostCallbackOverrides = {
  [K in keyof RequiredHostCallbacks]?: Partial<RequiredHostCallbacks[K]>;
};

/** Default no-op host callbacks with optional per-test overrides. */
export function makeHostCallbacks(
  overrides: HostCallbackOverrides = {},
): CompleteHostCallbacks {
  // An operation id names one operation, so the default hands out a fresh one
  // per call rather than a constant: the core keys the worker reference it
  // holds by that id, and a shared id loses all but the first.
  let nextOperationId = 1;
  const defaults: CompleteHostCallbacks = {
    navigation: { navigateTo: async () => {} },
    notifications: {
      pushNotification: async () => ({ id: 0 }),
      cancelNotification: async () => {},
    },
    permissions: {
      devicePermission: async () => "Deny",
      remotePermission: async () => "Deny",
    },
    features: {
      featureSupported: async () => ({ supported: false }),
      supportedChains: async () => ({ network: "paseo", chains: [] }),
    },
    productStorage: {
      read: async () => undefined,
      write: async () => {},
      clear: async () => {},
      async *subscribeStorage() {},
    },
    productOperations: {
      beginOperation: async () => ({ id: nextOperationId++ }),
      endOperation: async () => {},
    },
    coreStorage: {
      readCoreStorage: async () => undefined,
      writeCoreStorage: async () => {},
      clearCoreStorage: async () => {},
    },
    auth: { authStateChanged: () => {} },
    userConfirmation: {
      confirmUserAction: async () => false,
      confirmPermission: async () => "Deny",
    },
    preimage: {
      async *lookupPreimage() {},
    },
    theme: { async *subscribeTheme() {} },
    locale: { async *subscribeLocale() {} },
    chain: {
      connect: async () => ({
        send() {},
        async *responses() {},
        close() {},
      }),
    },
  };

  return {
    navigation: { ...defaults.navigation, ...overrides.navigation },
    notifications: {
      ...defaults.notifications,
      ...overrides.notifications,
    },
    permissions: {
      ...defaults.permissions,
      ...overrides.permissions,
    },
    features: { ...defaults.features, ...overrides.features },
    productStorage: {
      ...defaults.productStorage,
      ...overrides.productStorage,
    },
    productOperations: {
      ...defaults.productOperations,
      ...overrides.productOperations,
    },
    coreStorage: {
      ...defaults.coreStorage,
      ...overrides.coreStorage,
    },
    auth: { ...defaults.auth, ...overrides.auth },
    userConfirmation: {
      ...defaults.userConfirmation,
      ...overrides.userConfirmation,
    },
    preimage: { ...defaults.preimage, ...overrides.preimage },
    theme: { ...defaults.theme, ...overrides.theme },
    locale: { ...defaults.locale, ...overrides.locale },
    chain: { ...defaults.chain, ...overrides.chain },
    // Chat is an optional capability: only fixtures that ask for it get the
    // group, so the default fixture is a host that does not serve chat.
    ...(overrides.chat
      ? {
          chat: {
            createChatRoom: async () => ({ status: "New" as const }),
            registerChatBot: async () => ({ status: "New" as const }),
            postChatMessage: async () => ({ messageId: "message" }),
            async *subscribeChatRooms() {},
            ...overrides.chat,
          },
        }
      : {}),
    // Same for the OS permission-status capability: a host that omits it
    // leaves device grants resolving from stored state alone.
    ...(overrides.permissionStatus
      ? {
          permissionStatus: {
            devicePermissionStatus: async () => "NotApplicable" as const,
            ...overrides.permissionStatus,
          },
        }
      : {}),
    // And for Pocket: the default fixture is a host that keeps no card
    // collection, so Pocket calls are answered `Unsupported`.
    ...(overrides.pocket
      ? {
          pocket: {
            async *subscribePocketCards() {},
            removePocketCard: async () => {},
            ...overrides.pocket,
          },
        }
      : {}),
    // And for Game: the default fixture is a host that holds no reminders,
    // so Game calls are answered `Unsupported`.
    ...(overrides.game
      ? {
          game: {
            scheduleGameReminder: async () => {},
            cancelGameReminder: async () => {},
            ...overrides.game,
          },
        }
      : {}),
  };
}

/** Resolve after the current microtask/immediate queue, letting pending async work run. */
export function settle(): Promise<void> {
  return new Promise<void>((resolve) => setImmediate(resolve));
}
