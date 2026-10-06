import { describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";

import {
  HostChatCreateRoomRequest,
  HostChatCreateRoomResponse,
  HostDevicePermissionRequest,
  HostFeatureSupportedRequest,
  HostFeatureSupportedResponse,
  HostPushNotificationRequest,
  HostPushNotificationResponse,
  HostThemeSubscribeItem,
  RemotePermissionRequest,
} from "@parity/truapi";
import type {
  GenericError,
  HostSignPayloadData,
  HostThemeSubscribeItem as HostThemeSubscribeItemValue,
  ThemeVariant,
} from "@parity/truapi";

import { createWasmRawCallbacks } from "./generated/host-callbacks-adapter.js";
import {
  AuthState,
  CoreStorageKey,
  SecretCoreStorageKey,
  PermissionDecision,
  ProductContext,
  ProductExecutionKind,
  UserConfirmationReview,
} from "./generated/host-callbacks.js";
import { makeHostCallbacks, settle } from "./test-support.js";

// The generated `createWasmRawCallbacks` adapter speaks the symmetric SCALE
// byte boundary: codec-typed requests arrive as `Uint8Array` and are decoded
// for the typed host callback; codec-typed responses are SCALE-encoded back to
// `Uint8Array`. Primitives, strings and byte blobs pass through unchanged.

const GENESIS = `0x${"11".repeat(32)}` as `0x${string}`;

it("preserves one-use permission decisions across the WASM callback", async () => {
  const review = {
    tag: "IdentityDisclosure" as const,
    value: { productId: "playground.dot" },
  };
  for (const decision of ["AllowOnce", "AllowAlways", "Deny"] as const) {
    const reviews: UserConfirmationReview[] = [];
    const raw = createWasmRawCallbacks(makeHostCallbacks({
      userConfirmation: {
        confirmPermission: async (request) => {
          reviews.push(request);
          return decision;
        },
      },
    }));
    const encoded = await raw.confirmPermission(UserConfirmationReview.enc(review));
    expect({ decision: PermissionDecision.dec(encoded), reviews }).toEqual({
      decision,
      reviews: [review],
    });
  }
});

it("keeps public storage separate from the required secret callback group", async () => {
  const calls: unknown[] = [];
  const raw = createWasmRawCallbacks(
    makeHostCallbacks({
      coreStorage: {
        readCoreStorage: async (key) => {
          calls.push(["public", key]);
          return undefined;
        },
      },
      secretCoreStorage: {
        readSecretCoreStorage: async (key) => {
          calls.push(["secret", key]);
          throw new Error("protected store unavailable");
        },
      },
    }),
  );
  expect(
    await raw.readCoreStorage(
      CoreStorageKey.enc({ tag: "StatementRenewalTargets" }),
    ),
  ).toBeUndefined();
  await expect(
    raw.readSecretCoreStorage(SecretCoreStorageKey.enc({ tag: "AuthSession" })),
  ).rejects.toThrow("protected store unavailable");
  expect(calls).toEqual([
    ["public", { tag: "StatementRenewalTargets", value: undefined }],
    ["secret", { tag: "AuthSession", value: undefined }],
  ]);
});

const defaultTheme = (variant: ThemeVariant): HostThemeSubscribeItemValue => ({
  name: { tag: "Default" },
  variant,
});

const namedTheme = (
  name: string,
  variant: ThemeVariant,
): HostThemeSubscribeItemValue => ({
  name: { tag: "Custom", value: name },
  variant,
});
const PRODUCT_ACCOUNT = {
  dotNsIdentifier: "playground.dot",
  derivationIndex: { tag: "Index" as const, value: 0 },
};
const PROOF_CONTEXT = {
  productId: "playground.dot",
  suffix: { tag: "Index" as const, value: 0 },
};
const RING_LOCATION = {
  chainId: GENESIS,
  junctions: [{ tag: "PalletInstance" as const, value: 67 }],
};
const SIGN_PAYLOAD: HostSignPayloadData = {
  blockHash: GENESIS,
  blockNumber: "0x01",
  era: "0x00",
  genesisHash: GENESIS,
  method: "0x0102",
  nonce: "0x00",
  specVersion: "0x01",
  tip: "0x00",
  transactionVersion: "0x01",
  signedExtensions: [],
  version: 4,
  assetId: undefined,
  metadataHash: undefined,
  mode: undefined,
};

describe("createWasmRawCallbacks", () => {
  it("decodes requests and encodes typed responses", async () => {
    const writes: [string, number[]][] = [];
    const clears: string[] = [];
    const cancelled: number[] = [];
    const askedBy: ProductContext[] = [];
    const worker: ProductContext = {
      productId: "camera.dot",
      executionKind: "Worker",
    };
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        notifications: {
          pushNotification: async (notification) => ({
            id: notification.text.length,
          }),
          cancelNotification: async (id) => {
            cancelled.push(id);
          },
        },
        permissions: {
          devicePermission: async (product, request) => {
            askedBy.push(product);
            return request === "Camera" ? "AllowAlways" : "Deny";
          },
          remotePermission: async (product, request) => {
            askedBy.push(product);
            return request.permission.tag === "ChainSubmit"
              ? "AllowOnce"
              : "Deny";
          },
        },
        features: {
          featureSupported: async (request) => ({
            supported:
              request.tag === "Chain" && request.value.genesisHash === GENESIS,
          }),
        },
        productStorage: {
          read: async (key) => new TextEncoder().encode(`read:${key}`),
          write: async (key, value) => {
            writes.push([key, [...value]]);
          },
          clear: async (key) => {
            clears.push(key);
          },
        },
      }),
    );

    expect(
      HostPushNotificationResponse.dec(
        await raw.pushNotification!(
          HostPushNotificationRequest.enc({
            text: "hello",
            deeplink: undefined,
            scheduledAt: undefined,
          }),
        ),
      ).id,
    ).toBe(5);
    expect(
      PermissionDecision.dec(
        await raw.devicePermission!(
          ProductContext.enc(worker),
          HostDevicePermissionRequest.enc("Camera"),
        ),
      ),
    ).toBe("AllowAlways");
    expect(
      PermissionDecision.dec(
        await raw.remotePermission!(
          ProductContext.enc(worker),
          RemotePermissionRequest.enc({
            permission: { tag: "ChainSubmit" },
          }),
        ),
      ),
    ).toBe("AllowOnce");
    expect(askedBy).toEqual([worker, worker]);
    expect(
      HostFeatureSupportedResponse.dec(
        await raw.featureSupported!(
          HostFeatureSupportedRequest.enc({
            tag: "Chain",
            value: { genesisHash: GENESIS },
          }),
        ),
      ).supported,
    ).toBe(true);
    expect(await raw.read!("session")).toEqual(
      new TextEncoder().encode("read:session"),
    );

    await raw.write!("session", new Uint8Array([1, 2, 3]));
    await raw.clear!("session");
    await raw.cancelNotification?.(9);

    expect(writes).toEqual([["session", [1, 2, 3]]]);
    expect(clears).toEqual(["session"]);
    expect(cancelled).toEqual([9]);
  });

  it("bridges lifecycle, confirmations, and preimage callbacks", async () => {
    const calls: unknown[][] = [];
    async function* preimages() {
      yield ok(undefined);
      yield ok(new Uint8Array([4, 5, 6]));
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        auth: {
          authStateChanged: (state) => {
            calls.push(["authStateChanged", state]);
          },
        },
        secretCoreStorage: {
          readSecretCoreStorage: async (key) =>
            key.tag === "AuthSession" ? new Uint8Array([1, 2, 3]) : undefined,
          writeSecretCoreStorage: async (key, value) => {
            calls.push(["writeSecretCoreStorage", key, [...value]]);
          },
          clearSecretCoreStorage: async (key) => {
            calls.push(["clearSecretCoreStorage", key]);
          },
        },
        userConfirmation: {
          confirmUserAction: async (review) => {
            switch (review.tag) {
              case "SignPayload":
                return (
                  review.value.tag === "Product" &&
                  review.value.value.callingProductId === "playground.dot" &&
                  review.value.value.request.account.dotNsIdentifier ===
                    "playground.dot" &&
                  review.value.value.request.payload.method === "0x0102"
                );
              case "SignRaw":
                return (
                  review.value.tag === "Product" &&
                  review.value.value.callingProductId === "playground.dot" &&
                  review.value.value.watermarked === false &&
                  review.value.value.request.payload.tag === "Bytes" &&
                  review.value.value.request.payload.value.bytes === "0x0304"
                );
              case "CreateTransaction":
                return (
                  review.value.tag === "Product" &&
                  review.value.value.callingProductId === "playground.dot" &&
                  review.value.value.payload.signer.derivationIndex.tag ===
                    "Index" &&
                  review.value.value.payload.callData === "0x0506"
                );
              case "AccountAlias":
                return (
                  review.value.callingProductId === "playground.dot" &&
                  review.value.context.productId === "playground.dot" &&
                  review.value.ringLocation.junctions[0]?.tag ===
                    "PalletInstance"
                );
              case "CreateProof":
                return (
                  review.value.callingProductId === "playground.dot" &&
                  review.value.context.suffix.tag === "Index" &&
                  review.value.message[0] === 7
                );
              case "AccountAccess":
                return (
                  review.value.requestingProductId === "playground.dot" &&
                  review.value.targetProductId === "wallet.dot"
                );
              case "ResourceAllocation":
                return (
                  review.value.resources[0]?.tag === "StatementStoreAllowance"
                );
              case "PreimageSubmit":
                calls.push([
                  "confirmUserAction:PreimageSubmit",
                  review.value.size,
                ]);
                return review.value.size === 42n;
              default:
                return false;
            }
          },
        },
        preimage: {
          lookupPreimage: (key) => {
            calls.push(["lookupPreimage", [...key]]);
            return preimages();
          },
        },
      }),
    );

    const preimageEvents: (number[] | null)[] = [];
    const preimageErrors: string[] = [];
    const disposePreimages = raw.lookupPreimage!(
      new Uint8Array([9]),
      (value) => preimageEvents.push(value ? [...value] : null),
      (error) => preimageErrors.push(error.reason),
    );

    raw.authStateChanged?.(
      AuthState.enc({
        tag: "Pairing",
        value: { deeplink: "polkadotapp://example" },
      }),
    );
    const authSessionKey = SecretCoreStorageKey.enc({ tag: "AuthSession" });
    expect(await raw.readSecretCoreStorage!(authSessionKey)).toEqual(
      new Uint8Array([1, 2, 3]),
    );
    await raw.writeSecretCoreStorage!(
      authSessionKey,
      new Uint8Array([3, 2, 1]),
    );
    await raw.clearSecretCoreStorage!(authSessionKey);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "SignPayload",
          value: {
            tag: "Product",
            value: {
              callingProductId: "playground.dot",
              request: {
                account: PRODUCT_ACCOUNT,
                payload: SIGN_PAYLOAD,
              },
            },
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "SignRaw",
          value: {
            tag: "Product",
            value: {
              callingProductId: "playground.dot",
              request: {
                account: PRODUCT_ACCOUNT,
                payload: {
                  tag: "Bytes",
                  value: { bytes: "0x0304" },
                },
              },
              watermarked: false,
            },
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "CreateTransaction",
          value: {
            tag: "Product",
            value: {
              callingProductId: "playground.dot",
              payload: {
                signer: PRODUCT_ACCOUNT,
                genesisHash: GENESIS,
                callData: "0x0506",
                extensions: [],
                txExtVersion: 0,
                contacts: [],
              },
            },
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "AccountAlias",
          value: {
            callingProductId: "playground.dot",
            context: PROOF_CONTEXT,
            ringLocation: RING_LOCATION,
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "CreateProof",
          value: {
            callingProductId: "playground.dot",
            context: PROOF_CONTEXT,
            ringLocation: RING_LOCATION,
            message: new Uint8Array([7, 8]),
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "AccountAccess",
          value: {
            requestingProductId: "playground.dot",
            targetProductId: "wallet.dot",
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "ResourceAllocation",
          value: {
            callingProductId: "playground.dot",
            resources: [{ tag: "StatementStoreAllowance" }],
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "PreimageSubmit",
          value: { size: 42n },
        }),
      ),
    ).toBe(true);

    await settle();

    expect(preimageEvents).toEqual([null, [4, 5, 6]]);
    expect(calls).toEqual([
      ["lookupPreimage", [9]],
      [
        "authStateChanged",
        { tag: "Pairing", value: { deeplink: "polkadotapp://example" } },
      ],
      [
        "writeSecretCoreStorage",
        { tag: "AuthSession", value: undefined },
        [3, 2, 1],
      ],
      ["clearSecretCoreStorage", { tag: "AuthSession", value: undefined }],
      ["confirmUserAction:PreimageSubmit", 42n],
    ]);

    disposePreimages?.();
  });

  it("omits the chat callbacks when the host does not serve chat", () => {
    const raw = createWasmRawCallbacks(makeHostCallbacks());

    expect(raw.createChatRoom).toBeUndefined();
    expect(raw.registerChatBot).toBeUndefined();
    expect(raw.postChatMessage).toBeUndefined();
    expect(raw.subscribeChatRooms).toBeUndefined();
  });

  it("adapts the chat callbacks when the host serves chat", async () => {
    const seen: string[] = [];
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        chat: {
          createChatRoom: async (product, request) => {
            seen.push(`${product.productId}:${request.roomId}`);
            return { status: "Exists" };
          },
        },
      }),
    );

    const product = ProductContext.enc({
      productId: "chat.dot",
      executionKind: "Worker",
    });
    const request = HostChatCreateRoomRequest.enc({
      roomId: "room",
      name: "Support",
      icon: "",
    });
    const response = await raw.createChatRoom!(product, request);

    expect(seen).toEqual(["chat.dot:room"]);
    expect(HostChatCreateRoomResponse.dec(response)).toEqual({
      status: "Exists",
    });
  });

  it("adapts typed result subscriptions", async () => {
    async function* themes() {
      yield ok<HostThemeSubscribeItemValue>(namedTheme("midnight", "Dark"));
      yield ok<HostThemeSubscribeItemValue>(defaultTheme("Light"));
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        theme: {
          subscribeTheme: () => themes(),
        },
      }),
    );
    const seen: HostThemeSubscribeItemValue[] = [];
    const themeErrors: string[] = [];
    const dispose = raw.subscribeTheme?.(
      (theme) => seen.push(HostThemeSubscribeItem.dec(theme!)),
      (error) => themeErrors.push(error.reason),
    );

    await settle();

    expect(seen).toEqual([
      namedTheme("midnight", "Dark"),
      defaultTheme("Light"),
    ]);
    dispose?.();
  });

  it("propagates typed result subscription errors", async () => {
    async function* themes() {
      yield ok<HostThemeSubscribeItemValue>(defaultTheme("Dark"));
      yield err<HostThemeSubscribeItemValue, GenericError>({
        reason: "theme stream failed",
      });
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        theme: {
          subscribeTheme: () => themes(),
        },
      }),
    );
    const seen: HostThemeSubscribeItemValue[] = [];
    const errors: GenericError[] = [];
    const dispose = raw.subscribeTheme?.(
      (theme) => seen.push(HostThemeSubscribeItem.dec(theme!)),
      (error) => errors.push(error),
    );

    await settle();

    expect(seen).toEqual([defaultTheme("Dark")]);
    expect(errors).toEqual([{ reason: "theme stream failed" }]);
    dispose?.();
  });

  it("propagates thrown subscription iterator errors", async () => {
    async function* themes() {
      yield ok<HostThemeSubscribeItemValue>(defaultTheme("Dark"));
      throw new Error("theme iterator failed");
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        theme: {
          subscribeTheme: () => themes(),
        },
      }),
    );
    const seen: HostThemeSubscribeItemValue[] = [];
    const errors: GenericError[] = [];
    const dispose = raw.subscribeTheme?.(
      (theme) => seen.push(HostThemeSubscribeItem.dec(theme!)),
      (error) => errors.push(error),
    );

    await settle();

    expect(seen).toEqual([defaultTheme("Dark")]);
    expect(errors).toEqual([{ reason: "theme iterator failed" }]);
    dispose?.();
  });

  it("bridges typed chain connections", async () => {
    const sent: string[] = [];
    const responses = ['{"jsonrpc":"2.0","id":1,"result":"ok"}'];
    let closes = 0;
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        chain: {
          connect: async (genesisHash) => {
            expect([...genesisHash]).toEqual(Array(32).fill(0x11));
            return {
              send(request) {
                sent.push(request);
              },
              async *responses() {
                yield* responses;
              },
              close() {
                closes += 1;
              },
            };
          },
        },
      }),
    );

    expect(typeof raw.chainConnect).toBe("function");
    const received: string[] = [];
    const connection = await raw.chainConnect!(GENESIS, (json) =>
      received.push(json),
    );
    expect(connection).toBeTruthy();

    connection!.send('{"jsonrpc":"2.0","id":1,"method":"system_health"}');
    await settle();

    expect(sent).toEqual(['{"jsonrpc":"2.0","id":1,"method":"system_health"}']);
    expect(received).toEqual(responses);
    connection!.close();
    expect(closes).toBe(1);
  });
});

describe("ProductContext codec", () => {
  // Mirror of the Rust test
  // `product_context_encoding_matches_the_generated_host_codec` in
  // `rust/crates/truapi/src/platform.rs`, which pins these same bytes
  // through `parity-scale-codec`. Both halves must be edited together: a
  // ProductContext travels the wasm callback boundary encoded in Rust and
  // decoded here.
  it("product context encoding matches the Rust platform codec", () => {
    expect([...ProductExecutionKind.enc("App")]).toEqual([0]);
    expect([...ProductExecutionKind.enc("Widget")]).toEqual([1]);
    expect([...ProductExecutionKind.enc("Worker")]).toEqual([2]);

    const context = { productId: "app.dot", executionKind: "Worker" } as const;
    expect([...ProductContext.enc(context)]).toEqual([
      28,
      ...new TextEncoder().encode("app.dot"),
      2,
    ]);
    expect(ProductContext.dec(ProductContext.enc(context))).toEqual(context);
  });
});
