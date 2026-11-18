import type { TrUApiClient } from "./generated/client.js";
import type * as T from "./generated/types.js";
import { CHAT_CREATE_ROOM } from "./generated/wire-table.js";
import * as S from "./scale.js";
import type { CallOptions, TrUApiTransport } from "./transport.js";

/** Request to create a chat room, as a product built against the earlier client sends it. */
export interface HostChatCreateRoomRequestV1 {
  /** The product's own id for the room. Every later call names the room with it. */
  roomId: string;
  /** Room display name. */
  name: string;
  /** URL or base64 image. */
  icon: string;
}

/** Result of a room registration, as the earlier client reads it. */
export interface HostChatCreateRoomResponseV1 {
  /** `New` or `Exists`. */
  status: "New" | "Exists";
}

/** Chat room registration error, as the earlier client reads it. */
export type HostChatCreateRoomErrorV1 =
  /** Not allowed. */
  | { tag: "PermissionDenied"; value?: undefined }
  /** Catch-all. */
  | { tag: "Unknown"; value: { reason: string } };

const CreateRoomRequestV1 = S.indexedTaggedUnion({
  V1: [
    0,
    S.Struct({
      roomId: S.str,
      name: S.str,
      icon: S.str,
    }) as S.Codec<HostChatCreateRoomRequestV1>,
  ] as const,
});

const CreateRoomResponseV1 = S.indexedTaggedUnion({
  V1: [
    0,
    S.Struct({
      status: S.Status("New", "Exists"),
    }) as S.Codec<HostChatCreateRoomResponseV1>,
  ] as const,
});

const CreateRoomErrorV1 = S.indexedTaggedUnion({
  V1: [
    0,
    S.TaggedUnion({
      PermissionDenied: S._void,
      Unknown: S.Struct({ reason: S.str }) as S.Codec<{ reason: string }>,
    }) as S.Codec<HostChatCreateRoomErrorV1>,
  ] as const,
});

/** Whether `request` has the shape of the earlier client: a `roomId` and no `roomIdSource`. */
function isV1Request(request: unknown): request is HostChatCreateRoomRequestV1 {
  if (typeof request !== "object" || request === null) return false;
  const fields = request as Record<string, unknown>;
  return typeof fields.roomId === "string" && fields.roomIdSource === undefined;
}

/**
 * Lets `client.chat.createRoom` also take the request of the earlier client.
 *
 * A product built against the earlier client calls the client that a host
 * injects with `{ roomId, name, icon }`. The generated method sends version 2,
 * which needs `roomIdSource` and `config`, so it cannot encode that request.
 * With this, such a request goes out in version 1 and the answer is read in
 * version 1, as the earlier client did, and the host keeps the room under the
 * product's `roomId`. Every other request goes to the generated method.
 */
export function acceptChatV1CreateRoom(
  client: TrUApiClient,
  transport: TrUApiTransport,
): void {
  const createRoomV2 = client.chat.createRoom.bind(client.chat);
  const createRoomV1 = (
    request: HostChatCreateRoomRequestV1,
    options?: CallOptions,
  ) =>
    transport.request({
      ids: CHAT_CREATE_ROOM,
      payload: CreateRoomRequestV1.enc({ tag: "V1", value: request }),
      signal: options?.signal,
      decodeResponse: (payload) => {
        const result = S.Result(
          CreateRoomResponseV1,
          S.CallError(CreateRoomErrorV1),
        ).dec(payload);
        return result.success
          ? { success: true, value: result.value.value }
          : result;
      },
    });
  const createRoom = (
    request: T.HostChatCreateRoomRequest | HostChatCreateRoomRequestV1,
    options?: CallOptions,
  ) =>
    isV1Request(request)
      ? createRoomV1(request, options)
      : createRoomV2(request, options);
  Object.defineProperty(client.chat, "createRoom", {
    value: createRoom as TrUApiClient["chat"]["createRoom"],
    enumerable: false,
    configurable: false,
    writable: false,
  });
}
