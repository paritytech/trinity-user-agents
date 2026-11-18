import type { HostConnection } from '@parity/truapi/internal';

type Client = HostConnection['client'];
type ChatClient = Client['chat'];
type CreateRoom = ChatClient['createRoom'];
type PostMessage = ChatClient['postMessage'];
type SetRoomFooter = ChatClient['setRoomFooter'];

/**
 * The host-store default when the product declares no config, such as a
 * v1 caller: every part stays on the host, with no product queries.
 */
export const DEFAULT_CHAT_ROOM_CONFIG = {
  productSource: false,
  reactionDetails: false,
  reportSeen: false,
} as const;

/**
 * A v1 `chat.createRoom` carries neither `roomIdSource` nor `config`. The
 * generated V2 encoder reads `request.config.productSource` while encoding
 * and throws before the frame reaches the host. Default both here, at the
 * host's encode edge, so a v1-shaped call encodes without the product
 * changing. Config the product does send wins field by field.
 *
 * The host keeps the room under the id it derives from the source, but a v1
 * product names the room with its own `roomId` in every later call. So the
 * client remembers the id each v1-shaped `createRoom` answered, and
 * `postMessage` and `setRoomFooter` send that id in place of the product's.
 */
export function withChatRoomDefaults(client: Client): Client {
  const chat: ChatClient = client.chat;
  const createRoom = chat.createRoom.bind(chat);
  const postMessage = chat.postMessage.bind(chat);
  const setRoomFooter = chat.setRoomFooter.bind(chat);
  const derivedRoomIds = new Map<string, string>();
  const hostRoomId = (roomId: string) => derivedRoomIds.get(roomId) ?? roomId;

  (chat as { createRoom: CreateRoom }).createRoom = (request, options) => {
    const raw = request as unknown as {
      roomIdSource?: string;
      roomId?: string;
      config?: Partial<typeof DEFAULT_CHAT_ROOM_CONFIG>;
    };
    const productRoomId = raw.roomIdSource === undefined ? raw.roomId : undefined;
    return createRoom(
      {
        ...request,
        roomIdSource: raw.roomIdSource ?? raw.roomId ?? '',
        config: { ...DEFAULT_CHAT_ROOM_CONFIG, ...raw.config },
      } as Parameters<CreateRoom>[0],
      options,
    ).map((response) => {
      if (productRoomId !== undefined) derivedRoomIds.set(productRoomId, response.roomId);
      return response;
    });
  };
  (chat as { postMessage: PostMessage }).postMessage = (request, options) =>
    postMessage({ ...request, roomId: hostRoomId(request.roomId) }, options);
  (chat as { setRoomFooter: SetRoomFooter }).setRoomFooter = (request, options) =>
    setRoomFooter({ ...request, roomId: hostRoomId(request.roomId) }, options);
  return client;
}
