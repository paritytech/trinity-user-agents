import { type ChatMessageContent } from '@parity/truapi';
import { type TrUApiProductProvider } from '@parity/truapi-host';
import { toHex } from 'polkadot-api/utils';
import { type Subscription, distinctUntilChanged, map, of, switchMap } from 'rxjs';
import * as v from 'valibot';

import { hexString } from '@/shared/types';
import { type MessageContent, productRoomUseCase } from '@/domains/chat';
import { truapiRuntime, truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

/**
 * A user message in the core's vocabulary, or `null` for a variant the wire cannot
 * carry.
 *
 * Dropping is deliberate and not a gap to fill later: the domain records kinds that
 * exist only host-side (a reply, an edit, a sync carrier), and inventing a wire shape
 * for them would tell the product something the protocol has no word for. The mirror
 * of `productRuntimeService.toChatMessageContent`, which degrades in the other
 * direction for the same reason.
 */
function toWireContent(content: MessageContent): ChatMessageContent | null {
  switch (content.type) {
    case 'text':
      return { tag: 'Text', value: { text: content.text } };
    case 'richText':
      return { tag: 'RichText', value: { text: content.text ?? '', media: [] } };
    case 'reacted':
      return { tag: 'Reaction', value: { messageId: content.messageId, emoji: content.emoji } };
    case 'reactionRemoved':
      return { tag: 'ReactionRemoved', value: { messageId: content.messageId, emoji: content.emoji } };
    case 'custom':
      return { tag: 'Custom', value: { messageType: content.messageType, payload: v.parse(hexString, toHex(content.payload)) } };
    default:
      return null;
  }
}

/**
 * Deliver the user's messages to a running product.
 *
 * The core serves the product's `Chat/action_subscribe` stream but publishes nothing
 * into it — that is the host's half, and without it a product can post and draw but
 * never hears the user, which is most of what a chat product is for.
 *
 * Keyed on the signed-in identity and re-keyed when it changes: rooms are stored per
 * user, so a delivery loop left on the previous identity would forward one user's
 * messages into a session opened for another.
 *
 * Returns a teardown; the worker's lifetime owns it.
 */
function start(productId: string, provider: TrUApiProductProvider): VoidFunction {
  const publish = provider.publishChatAction;
  // TODO(chat-workers): diagnostic — remove once the worker-send break is pinned.
  console.info('[chat-worker-diag] delivery start', { productId, hasPublishChatAction: Boolean(publish) });
  // Absent on runtimes with no live channel — nothing to deliver into.
  if (!publish) return () => undefined;

  const subscription: Subscription = truapiRuntime.value$
    .pipe(
      map(state => state.authState),
      distinctUntilChanged(),
      map(truapiRuntimeUseCase.toUserId),
      switchMap(userId => (userId ? productRoomUseCase.watchOutgoingMessages({ productId, userId }) : of(null))),
    )
    .subscribe(message => {
      if (!message) return;

      const content = toWireContent(message.content);
      if (!content) return;

      // TODO(chat-workers): diagnostic — remove once the worker-send break is pinned.
      console.info('[chat-worker-diag] delivering outgoing message to worker', { productId, roomId: message.roomId });
      publish
        .call(provider, { roomId: message.roomId, peer: message.peerId, payload: { tag: 'MessagePosted', value: content } })
        .catch((error: unknown) => {
          // One undelivered message, not a broken pipe: the stream stays subscribed so
          // the next message still has a chance.
          console.warn('[product-worker] chat delivery failed', { productId, roomId: message.roomId, error });
        });
    });

  return () => subscription.unsubscribe();
}

export const chatDeliveryUseCase = {
  start,
};
