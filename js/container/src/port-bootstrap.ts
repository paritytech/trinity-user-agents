import type { MessagePortBridge } from '@parity/truapi/internal';

/** Sent to the parent so it knows the container is listening. It carries no secret. */
export const CONTAINER_READY = 'truapi-container-ready';
/** The parent's answer, carrying the private port as its only transferred object. */
export const CONTAINER_INIT = 'truapi-container-init';

/**
 * Take the host's private port, once, from the window that embeds this page.
 *
 * Runs before any product code, so its listener is the first one and, by being a
 * capture listener that stops propagation, the only one that ever sees the
 * message that carries the port. Anything else is left alone: a message from
 * another window, without exactly one port, or of another type is not ours.
 *
 * With `parentOrigin`, the announcement is addressed to that origin only and a
 * message from any other origin is ignored, so a page that embeds this one from
 * elsewhere, such as a subframe of the product, cannot hand it a port. Without
 * it the parent is trusted by window alone.
 *
 * Returns whether a parent window exists to answer. A top-level page has none,
 * so the bridge never receives a port and every authorization stays pending.
 */
export function installMessagePortBootstrap(
  win: Window & typeof globalThis,
  bridge: MessagePortBridge,
  parentOrigin?: string,
): boolean {
  const apply = Reflect.apply;
  const addEventListener = win.EventTarget.prototype.addEventListener;
  const removeEventListener = win.EventTarget.prototype.removeEventListener;
  const stopImmediatePropagation = win.Event.prototype.stopImmediatePropagation;
  const postMessage = win.postMessage;
  const descriptor = Object.getOwnPropertyDescriptor;
  const data = descriptor(win.MessageEvent.prototype, 'data')!.get!;
  const source = descriptor(win.MessageEvent.prototype, 'source')!.get!;
  const ports = descriptor(win.MessageEvent.prototype, 'ports')!.get!;
  const origin = descriptor(win.MessageEvent.prototype, 'origin')!.get!;
  const parent = win.parent;

  function onMessage(event: MessageEvent): void {
    if (apply(source, event, []) !== parent) return;
    if (parentOrigin !== undefined && apply(origin, event, []) !== parentOrigin) return;
    const payload = apply(data, event, []);
    if (payload === null || typeof payload !== 'object') return;
    if ((payload as { type?: unknown }).type !== CONTAINER_INIT) return;
    const transferred = apply(ports, event, []) as readonly MessagePort[];
    if (transferred.length !== 1) return;
    apply(stopImmediatePropagation, event, []);
    if (bridge.attach(transferred[0])) {
      apply(removeEventListener, win, ['message', onMessage, true]);
    }
  }

  apply(addEventListener, win, ['message', onMessage, true]);
  if (parent === win) return false;
  apply(postMessage, parent, [{ type: CONTAINER_READY }, parentOrigin ?? '*']);
  return true;
}
