import { type RendererNode } from '@parity/truapi';

/** One product-authored render tree, decoded off the wire. */
export type RendererTreeNode = RendererNode;

/** What the user did to the tree, sent back to whoever drew it. */
export type ActionHandler = (actionId: string, value?: Uint8Array) => void;

/**
 * Opens the render-tree subscription and returns its teardown.
 *
 * The caller supplies this because *where* a tree comes from differs per surface
 * — a chat message subscribes through the core's render request, and an
 * input candidate through the input-routing gateway — while the drawing and the
 * visibility gating are the same either way. Must be stable (`useCallback`): a
 * new identity resubscribes.
 */
export type SubscribeToNode = (sink: RenderSink) => VoidFunction;

/**
 * What a render can report back.
 *
 * `onError` is not "the subscription ended" — it means the tree already delivered
 * is **partial** and must come off screen, so a product that fails halfway is never
 * shown as if it had finished. A teardown the host initiates reports nothing at
 * all, which is why cancellation cannot be confused with failure.
 */
export type RenderSink = {
  onNode(node: RendererTreeNode): void;
  onError(): void;
};
