import { readFileSync } from "node:fs";

import type { RequiredHostCallbacks } from "./generated/host-callbacks.js";
import type { BundledStack } from "./runtime.js";

/** `HostCallbacks` with every optional member required, for exhaustive test fixtures. */
export type CompleteHostCallbacks = RequiredHostCallbacks;

type HostCallbackOverrides = {
  [K in keyof RequiredHostCallbacks]?: Partial<RequiredHostCallbacks[K]>;
};

/** Default no-op host callbacks with optional per-test overrides. */
export function makeHostCallbacks(
  overrides: HostCallbackOverrides = {},
): CompleteHostCallbacks {
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
    },
    extrinsicVersion: {
      transactionExtensionVersions: async () => ({ chains: [] }),
    },
    productStorage: {
      read: async () => undefined,
      write: async () => {},
      clear: async () => {},
      clearProduct: async () => {},
      async *subscribeStorage() {},
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
    extrinsicVersion: {
      ...defaults.extrinsicVersion,
      ...overrides.extrinsicVersion,
    },
    productStorage: {
      ...defaults.productStorage,
      ...overrides.productStorage,
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
            createChatRoom: async () => "New" as const,
            registerChatBot: async () => ({ status: "New" as const }),
            postChatMessage: async () => ({ messageId: "message" }),
            setChatRoomFooter: async () => {},
            async *subscribeChatRooms() {},
            // No room exists until a fixture overrides this.
            chatRoom: async () => undefined,
            setChatRoomMetadata: async () => "Applied",
            setChatRoomPreview: async () => "Applied",
            deleteChatRoom: async () => "Applied",
            updateChatFeed: async () => "Applied",
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
    // And for the scanner: the default fixture is a host with no viewfinder,
    // so scans are answered `Unsupported`, as on a web host.
    ...(overrides.scanner
      ? {
          scanner: {
            scanCode: async () => ({ tag: "Dismissed" as const }),
            ...overrides.scanner,
          },
        }
      : {}),
  };
}

/** Resolve after the current microtask/immediate queue, letting pending async work run. */
export function settle(): Promise<void> {
  return new Promise<void>((resolve) => setImmediate(resolve));
}

/**
 * The previewnet stack as a host build bundles it: the link and signed copy
 * in the repository's `stacks/`.
 */
export function bundledTestStack(): BundledStack {
  const stacks = new URL("../../../../stacks/", import.meta.url);
  return {
    pointer: readFileSync(new URL("previewnet.link", stacks), "utf8").trim(),
    descriptor: new Uint8Array(
      readFileSync(new URL("previewnet.json", stacks)),
    ),
  };
}
