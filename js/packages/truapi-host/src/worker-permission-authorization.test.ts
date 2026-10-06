import { describe, expect, it } from "bun:test";

import type { WorkerToMain } from "./worker-protocol.js";
import {
  handleGetPermissionAuthorizationStatus,
  handleGetPermissionAuthorizationStatuses,
  handleSetPermissionAuthorizationStatus,
  handleRefreshPermissionAuthorization,
  type PermissionAuthorizationRuntime,
} from "./worker-permission-authorization.js";

const PRODUCT_ID = "playground.dot";

function recordMessages() {
  const messages: WorkerToMain[] = [];
  return {
    messages,
    postToMain(msg: WorkerToMain): void {
      messages.push(msg);
    },
  };
}

function makeRuntime(
  overrides: Partial<PermissionAuthorizationRuntime> = {},
): PermissionAuthorizationRuntime {
  return {
    permissionAuthorizationStatus: async () => "NotDetermined",
    permissionAuthorizationStatuses: async (productId, requests) => {
      void productId;
      return requests.map(() => "NotDetermined");
    },
    setPermissionAuthorizationStatus: async () => {},
    refreshPermissionAuthorization: async () => {},
    ...overrides,
  };
}

describe("worker permission authorization handlers", () => {
  it("does not acknowledge a permission refresh before its fence rejects", async () => {
    const { messages, postToMain } = recordMessages();
    const fence = Promise.withResolvers<void>();
    const runtime = makeRuntime({
      refreshPermissionAuthorization: () => fence.promise,
    });
    const pending = handleRefreshPermissionAuthorization(
      runtime,
      postToMain,
      PRODUCT_ID,
      9,
      new Uint8Array([1]),
    );
    await Promise.resolve();
    expect(messages).toEqual([]);
    fence.reject(new Error("refresh failed"));
    await pending;
    expect(messages).toEqual([
      {
        kind: "refreshPermissionAuthorizationResponse",
        requestId: 9,
        ok: false,
        error: "refresh failed",
      },
    ]);
  });

  it("reports permission authorization requests received before runtime is ready", async () => {
    const { messages, postToMain } = recordMessages();
    const request = new Uint8Array([1]);

    await handleGetPermissionAuthorizationStatus(
      null,
      postToMain,
      PRODUCT_ID,
      1,
      request,
    );
    await handleGetPermissionAuthorizationStatuses(
      null,
      postToMain,
      PRODUCT_ID,
      2,
      [request],
    );
    await handleSetPermissionAuthorizationStatus(
      null,
      postToMain,
      PRODUCT_ID,
      3,
      request,
      "Authorized",
    );
    await handleRefreshPermissionAuthorization(
      null,
      postToMain,
      PRODUCT_ID,
      4,
      request,
    );

    expect(
      messages.map((message) => ({
        kind: message.kind,
        ok: "ok" in message ? message.ok : undefined,
      })),
    ).toEqual([
      { kind: "permissionAuthorizationStatusResponse", ok: false },
      { kind: "permissionAuthorizationStatusesResponse", ok: false },
      { kind: "setPermissionAuthorizationStatusResponse", ok: false },
      { kind: "refreshPermissionAuthorizationResponse", ok: false },
    ]);
  });
});
