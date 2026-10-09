import { describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";
import { hexToBytes, str, Vector } from "@parity/truapi/scale";

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
  NativeChatFilePickRequest,
  NativeChatFileExportRequest,
  NativeChatPickedFile,
  NativeCoinageRequest,
  NativeCoinageResponse,
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
  it("leaves the native callback absent for the built-in Rust wallet", () => {
    const raw = createWasmRawCallbacks(makeHostCallbacks());
    expect(raw.nativeCoinage).toBeUndefined();
  });

  it("rejects malformed native registration rather than enabling the Rust wallet", () => {
    for (const coinageWallet of [null, false, {}, { nativeCoinage: 1 }]) {
      expect(() =>
        createWasmRawCallbacks({
          ...makeHostCallbacks(),
          coinageWallet: coinageWallet as never,
        }),
      ).toThrow();
    }
  });

  it("keeps the registered native wallet after rejection or infrastructure failure", async () => {
    const host = makeHostCallbacks({
      coinageWallet: {
        nativeCoinage: async (request) => {
          if (request.operation.tag === "Reconcile")
            throw new Error("private native memo bearer material");
          return { tag: "Failed", value: { reason: "Unavailable" } };
        },
      },
    });
    const native = createWasmRawCallbacks(host);
    const request = NativeCoinageRequest.enc({
      scope: {
        rootPublicKey: new Uint8Array(32),
        genesisHash: new Uint8Array(32),
      },
      operation: { tag: "Denomination" },
    });
    expect(
      NativeCoinageResponse.dec(await native.nativeCoinage!(request)),
    ).toEqual({
      tag: "Failed",
      value: { reason: "Unavailable" },
    });
    const infrastructureFailure = native.nativeCoinage!(
      NativeCoinageRequest.enc({
        scope: {
          rootPublicKey: new Uint8Array(32),
          genesisHash: new Uint8Array(32),
        },
        operation: { tag: "Reconcile" },
      }),
    );
    await expect(infrastructureFailure).rejects.toThrow();
    await infrastructureFailure.catch((error: Error) => {
      expect(error.message).not.toContain("bearer material");
    });
    // Neither mutation of the source group nor failure changes the captured service.
    host.coinageWallet!.nativeCoinage = async () => ({ tag: "Done" });
    host.coinageWallet = undefined;
    expect(
      NativeCoinageResponse.dec(await native.nativeCoinage!(request)),
    ).toEqual({
      tag: "Failed",
      value: { reason: "Unavailable" },
    });
  });

  it("fails every file operation closed when the embedding has no custody backend", async () => {
    const raw = createWasmRawCallbacks(makeHostCallbacks());
    const context = {
      productId: "chat.dot",
      peerIdentity: new Uint8Array(32),
      peerUsername: undefined,
    };
    const pick = NativeChatFilePickRequest.enc({ ...context, maxFiles: 1 });
    const save = NativeChatFileExportRequest.enc({
      ...context,
      metadata: {
        mimeType: "application/octet-stream",
        sizeBytes: 0,
        kind: { tag: "File" },
      },
    });
    for (const operation of [
      () => raw.pickChatFiles(pick),
      () => raw.readChatFile("source", 0n, 0),
      () => raw.releaseChatFile("source"),
      () => raw.beginChatFileExport(save),
      () => raw.writeChatFileExport("export", 0n, new Uint8Array()),
      () => raw.finishChatFileExport("export"),
      () => raw.cancelChatFileExport("export"),
    ]) {
      await expect(operation()).rejects.toThrow();
    }
  });

  it("distinguishes explicit file cancellation from unavailable custody", async () => {
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        nativeChatFiles: {
          pickChatFiles: async () => [],
          beginChatFileExport: async () => undefined,
        },
      }),
    );
    const context = {
      productId: "chat.dot",
      peerIdentity: new Uint8Array(32),
      peerUsername: undefined,
    };
    expect(
      Vector(NativeChatPickedFile).dec(
        await raw.pickChatFiles(
          NativeChatFilePickRequest.enc({ ...context, maxFiles: 1 }),
        ),
      ),
    ).toEqual([]);
    const cancelled = await raw.beginChatFileExport(
      NativeChatFileExportRequest.enc({
        ...context,
        metadata: {
          mimeType: "application/octet-stream",
          sizeBytes: 0,
          kind: { tag: "File" },
        },
      }),
    );
    expect(cancelled == null).toBe(true);
  });

  it("does not invent an identity search provider for hosts that omit it", () => {
    const raw = createWasmRawCallbacks(makeHostCallbacks());
    expect(raw.identityUsernameCandidates).toBeUndefined();
  });

  it("encodes username candidate accounts as one SCALE vector", async () => {
    const first = new Uint8Array(32).fill(0x11);
    const second = new Uint8Array(32).fill(0x22);
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        identityBackend: {
          identityUsernameCandidates: async () => [first, second],
        },
      }),
    );

    expect(
      await raw.identityUsernameCandidates!("alice", new Uint8Array(32)),
    ).toEqual(new Uint8Array([8, ...first, ...second]));
  });

  it("keeps backend failure distinct from a successful empty search", async () => {
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        identityBackend: {
          identityUsernameCandidates: async (username) => {
            if (username === "unavailable")
              throw new Error("authentication expired");
            return [];
          },
        },
      }),
    );

    await expect(
      raw.identityUsernameCandidates!("unavailable", new Uint8Array(32)),
    ).rejects.toThrow("authentication expired");
    expect(
      await raw.identityUsernameCandidates!("absent", new Uint8Array(32)),
    ).toEqual(new Uint8Array([0]));
  });

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
        coreStorage: {
          readCoreStorage: async (key) =>
            key.tag === "AuthSession" ? new Uint8Array([1, 2, 3]) : undefined,
          writeCoreStorage: async (key, value) => {
            calls.push(["writeCoreStorage", key, [...value]]);
          },
          clearCoreStorage: async (key) => {
            calls.push(["clearCoreStorage", key]);
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
    const authSessionKey = CoreStorageKey.enc({ tag: "AuthSession" });
    expect(await raw.readCoreStorage!(authSessionKey)).toEqual(
      new Uint8Array([1, 2, 3]),
    );
    await raw.writeCoreStorage!(authSessionKey, new Uint8Array([3, 2, 1]));
    await raw.clearCoreStorage!(authSessionKey);
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
      ["writeCoreStorage", { tag: "AuthSession", value: undefined }, [3, 2, 1]],
      ["clearCoreStorage", { tag: "AuthSession", value: undefined }],
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

  it("keeps an unconfigured HOP provider unavailable", async () => {
    const raw = createWasmRawCallbacks(makeHostCallbacks());
    expect(
      Vector(str).dec(await raw.allowedHopEndpoints(hexToBytes(GENESIS))),
    ).toEqual([]);
    await expect(
      raw.hopConnect(GENESIS, "wss://hop.example", () => {}),
    ).rejects.toThrow();
  });

  it("requires an exact current trusted WSS endpoint before dialing HOP", async () => {
    const endpoint = "wss://hop.example/rpc";
    let allowed = [
      endpoint,
      "ws://hop.example/rpc",
      "wss://user@hop.example/rpc",
      "wss://hop.example/rpc#fragment",
    ];
    const dials: string[] = [];
    const sent: string[] = [];
    const received: string[] = [];
    let closes = 0;
    let closed = 0;
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        hop: {
          async allowedHopEndpoints(genesis) {
            expect(genesis).toEqual(hexToBytes(GENESIS));
            return allowed;
          },
          async connectHop(genesis, url) {
            expect(genesis).toEqual(hexToBytes(GENESIS));
            dials.push(url);
            return {
              send: (request) => sent.push(request),
              async *responses() {
                yield '{"id":1,"result":"ok"}';
              },
              close() {
                closes += 1;
              },
            };
          },
        },
      }),
    );
    expect(
      Vector(str).dec(await raw.allowedHopEndpoints(hexToBytes(GENESIS))),
    ).toEqual(allowed);
    for (const denied of [
      "wss://HOP.example/rpc",
      "wss://other.example/rpc",
      ...allowed.slice(1),
    ]) {
      await expect(raw.hopConnect(GENESIS, denied, () => {})).rejects.toThrow();
    }
    const connection = await raw.hopConnect(
      GENESIS,
      endpoint,
      (response) => received.push(response),
      () => {
        closed += 1;
      },
    );
    connection!.send('{"id":1,"method":"hop_info"}');
    await settle();
    expect(dials).toEqual([endpoint]);
    expect(sent).toEqual(['{"id":1,"method":"hop_info"}']);
    expect(received).toEqual(['{"id":1,"result":"ok"}']);
    expect(closes).toBe(1);
    expect(closed).toBe(1);
    connection!.close();
    expect(closes).toBe(1);
    expect(() => connection!.send("{}")).toThrow();
    allowed = [];
    await expect(raw.hopConnect(GENESIS, endpoint, () => {})).rejects.toThrow();
    expect(dials).toEqual([endpoint]);
  });

  it("drops responses arriving after a connection is closed", async () => {
    const next = Promise.withResolvers<IteratorResult<string>>();
    let closes = 0;
    let returns = 0;
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        chain: {
          async connect() {
            return {
              send() {},
              responses: () => ({
                [Symbol.asyncIterator]: () => ({
                  next: () => next.promise,
                  async return() {
                    returns += 1;
                    return { done: true, value: undefined };
                  },
                }),
              }),
              close() {
                closes += 1;
              },
            };
          },
        },
      }),
    );
    const received: string[] = [];
    const connection = await raw.chainConnect(GENESIS, (response) =>
      received.push(response),
    );
    connection!.close();
    connection!.close();
    next.resolve({ done: false, value: "late response" });
    await settle();
    expect(received).toEqual([]);
    expect(closes).toBe(1);
    expect(returns).toBe(1);
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
