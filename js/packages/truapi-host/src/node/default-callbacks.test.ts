import { describe, expect, it } from "bun:test";

import type { HostChainSet } from "../generated/host-callbacks.js";
import { ok } from "neverthrow";

import { createHeadlessHostCallbacks } from "./default-callbacks.js";

const PEOPLE =
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";
const OTHER =
  "0x1111111111111111111111111111111111111111111111111111111111111111";

const supportedChains: HostChainSet = {
  network: "paseo",
  chains: [{ identifier: "People", genesisHash: PEOPLE }],
};

const hostCallbacks = {
  coreStorage: {
    async readCoreStorage() {
      return undefined;
    },
    async writeCoreStorage() {},
    async clearCoreStorage() {},
  },
  chain: {
    async connect(): Promise<never> {
      throw new Error("not used");
    },
  },
  auth: { authStateChanged() {} },
  productStorage: {
    async read() {
      return undefined;
    },
    async write() {},
    async clear() {},
    subscribeStorage(): AsyncIterable<never> {
      return {
        [Symbol.asyncIterator]: () => ({
          next: () => new Promise<never>(() => {}),
        }),
      };
    },
  },
  supportedChains,
  userConfirmation: {
    async confirmUserAction() {
      return true;
    },
    async confirmPermission() {
      return "AllowOnce" as const;
    },
  },
  permissions: {
    async devicePermission() {
      return "Deny" as const;
    },
    async remotePermission() {
      return "AllowOnce" as const;
    },
  },
};

describe("createHeadlessHostCallbacks", () => {
  // Reviews and permissions are the user's answers, so the host's own policy
  // must reach the core untouched rather than be replaced by a default.
  it("passes the host's review and permission policy through unchanged", () => {
    const { userConfirmation, permissions } =
      createHeadlessHostCallbacks(hostCallbacks);
    expect({ userConfirmation, permissions }).toEqual({
      userConfirmation: hostCallbacks.userConfirmation,
      permissions: hostCallbacks.permissions,
    });
  });

  it("supports exactly the chains the host can connect to", async () => {
    const { features } = createHeadlessHostCallbacks(hostCallbacks);
    const supported = async (genesisHash: `0x${string}`) =>
      (
        await features.featureSupported({
          tag: "Chain",
          value: { genesisHash },
        })
      ).supported;
    expect([
      await supported(PEOPLE),
      await supported(PEOPLE.toUpperCase() as `0x${string}`),
      await supported(OTHER),
    ]).toEqual([true, true, false]);
  });

  it("refuses navigation and notifications with a reason instead of pretending they happened", async () => {
    const callbacks = createHeadlessHostCallbacks(hostCallbacks);
    await expect(
      callbacks.navigation.navigateTo("https://example.com"),
    ).rejects.toThrow(/headless host/);
    await expect(
      callbacks.notifications.pushNotification({} as never),
    ).rejects.toThrow(/headless host/);
  });

  // The core reads the first item as the current value and keeps the stream
  // open for changes, so a stream that ended would read as "unsubscribed".
  it("emits theme once, then stays open until the consumer closes it", async () => {
    const stream = createHeadlessHostCallbacks(hostCallbacks)
      .theme.subscribeTheme()
      [Symbol.asyncIterator]();
    expect(await stream.next()).toEqual({
      value: ok({ name: { tag: "Default" }, variant: "Dark" }),
      done: false,
    });
    const pending = stream.next();
    await stream.return?.();
    expect(await pending).toEqual({ value: undefined, done: true });
  });
});
