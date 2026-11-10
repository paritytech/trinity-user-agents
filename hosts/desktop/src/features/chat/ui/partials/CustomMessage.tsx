import { type RenderContext } from '@parity/truapi';
import { toHex } from 'polkadot-api/utils';
import { useCallback, useMemo } from 'react';
import * as v from 'valibot';

import { hexString } from '@/shared/types';
import { useProductWorkerInstance, useWorkerDemand } from '@/aggregates/product-workers';
import { type ActionHandler, type SubscribeToNode, RendererTree } from '@/widgets/RendererTree';

type Props = {
  productId: string;
  messageId: string;
  messageType: string;
  payload: Uint8Array;
  roomId: string;
};

const NO_SUBSCRIPTION = () => {};

/**
 * A message the product draws itself.
 *
 * Everything about *drawing* — the render tree and the visibility gate — belongs to
 * `RendererTree`, which the input surface uses for the same purpose. What is chat's
 * alone lives here: the tree comes from the product that owns the room, and an action
 * goes back naming the same message the tree was drawn for.
 *
 * One `RenderContext` addresses both directions. That is the point of the core's
 * addressing — the context that named the body names an action inside it, so the host
 * mints no correlation id of its own.
 *
 * Declaring worker demand is what makes a message in a room whose product is not on
 * screen draw at all: without it there is no running worker to ask, and the core's
 * render reference can keep a worker alive but cannot start one.
 */
export const CustomMessage = ({ productId, messageId, messageType, payload, roomId }: Props) => {
  useWorkerDemand(productId);
  const instance = useProductWorkerInstance(productId);

  const context = useMemo<RenderContext>(
    () => ({ tag: 'ChatMessage', value: { roomId, messageId, messageType } }),
    [roomId, messageId, messageType],
  );

  const subscribe = useCallback<SubscribeToNode>(
    sink => {
      // Optional on the provider — present only on a runtime holding a live channel
      // to the core.
      const render = instance?.coreProvider.render;
      if (!instance || !render) return NO_SUBSCRIPTION;

      // The core reports failure through the sink rather than throwing, and exactly
      // one terminal fires per render. `onComplete` is deliberately unhandled: it
      // means the stream ended and the last tree stands, so there is nothing to do.
      // Host-side teardown — the disposer below — reports neither, which is what
      // keeps a scrolled-away message from looking like a failed one.
      return render.call(
        instance.coreProvider,
        { context, payload: v.parse(hexString, toHex(payload)) },
        { onUpdate: sink.onNode, onError: () => sink.onError() },
      );
    },
    [instance, context, payload],
  );

  const onAction = useCallback<ActionHandler>(
    (actionId, value) => {
      const publish = instance?.coreProvider.publishRendererAction;
      if (!instance || !publish) return;

      publish
        // A button press carries no payload; a text field carries the UTF-8 bytes of
        // its new value, unframed.
        .call(instance.coreProvider, { context, actionId, payload: value ? v.parse(hexString, toHex(value)) : '0x' })
        .catch((error: unknown) => {
          // The action is a user gesture with no reply path; a rejection means the
          // product never heard it. Reporting beats pretending it landed.
          console.warn('[chat] renderer action was not delivered', { productId, messageId, actionId, error });
        });
    },
    [instance, context, productId, messageId],
  );

  return <RendererTree productId={productId} subscribe={subscribe} onAction={onAction} />;
};
