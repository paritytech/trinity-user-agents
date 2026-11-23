import { asProductId } from "@parity/truapi";
import { type ProductId } from "@parity/truapi";
import { describe, expect, it } from "bun:test";

import type { PermissionAuthorizationStatus } from "./runtime.js";
import type { WorkerToMain } from "./worker-protocol.js";
import {
  handleGetPermissionAuthorizationStatus,
  handleGetPermissionAuthorizationStatuses,
  handleSetPermissionAuthorizationStatus,
  handleRefreshPermissionAuthorization,
  type PermissionAuthorizationRuntime,
} from "./worker-permission-authorization.js";

const PRODUCT_ID = asProductId("playground.dot");

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
  it("responds with a single permission authorization status", async () => {
    const { messages, postToMain } = recordMessages();
    const request = new Uint8Array([1, 2, 3]);
    const calls: { productId: ProductId; request: Uint8Array }[] = [];
    const runtime = makeRuntime({
      permissionAuthorizationStatus: async (productId, receivedRequest) => {
        calls.push({ productId, request: receivedRequest });
        return "Authorized";
      },
    });

    await handleGetPermissionAuthorizationStatus(
      runtime,
      postToMain,
      PRODUCT_ID,
      7,
      request,
    );

    expect(calls).toEqual([{ productId: PRODUCT_ID, request }]);
    expect(messages).toEqual([
      {
        kind: "permissionAuthorizationStatusResponse",
        requestId: 7,
        ok: true,
        status: "Authorized",
      },
    ]);
  });

  it("responds with batched permission authorization statuses", async () => {
    const { messages, postToMain } = recordMessages();
    const requests = [new Uint8Array([1]), new Uint8Array([2])];
    const statuses: PermissionAuthorizationStatus[] = ["Denied", "Authorized"];
    const calls: { productId: ProductId; requests: Uint8Array[] }[] = [];
    const runtime = makeRuntime({
      permissionAuthorizationStatuses: async (productId, receivedRequests) => {
        calls.push({ productId, requests: receivedRequests });
        return statuses;
      },
    });

    await handleGetPermissionAuthorizationStatuses(
      runtime,
      postToMain,
      PRODUCT_ID,
      8,
      requests,
    );

    expect(calls).toEqual([{ productId: PRODUCT_ID, requests }]);
    expect(messages).toEqual([
      {
        kind: "permissionAuthorizationStatusesResponse",
        requestId: 8,
        ok: true,
        statuses,
      },
    ]);
  });

  it("responds after setting a permission authorization status", async () => {
    const { messages, postToMain } = recordMessages();
    const request = new Uint8Array([9, 10]);
    const calls: {
      productId: ProductId;
      request: Uint8Array;
      status: PermissionAuthorizationStatus;
    }[] = [];
    const runtime = makeRuntime({
      setPermissionAuthorizationStatus: async (
        productId,
        receivedRequest,
        status,
      ) => {
        calls.push({ productId, request: receivedRequest, status });
      },
    });

    await handleSetPermissionAuthorizationStatus(
      runtime,
      postToMain,
      PRODUCT_ID,
      9,
      request,
      "Denied",
    );

    expect(calls).toEqual([
      { productId: PRODUCT_ID, request, status: "Denied" },
    ]);
    expect(messages).toEqual([
      {
        kind: "setPermissionAuthorizationStatusResponse",
        requestId: 9,
        ok: true,
      },
    ]);
  });

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
