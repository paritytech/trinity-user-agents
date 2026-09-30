import { describe, expect, test } from 'bun:test';
import { createMessagePortBridge } from '@parity/truapi/internal';
import {
  CONTAINER_INIT,
  CONTAINER_READY,
  installMessagePortBootstrap,
} from './port-bootstrap.js';

/**
 * A window with a parent, as far as the bootstrap can tell. Messages are
 * dispatched to it as real MessageEvents, so the listener order, capture phase
 * and propagation stop are the runtime's, not a stand-in's.
 */
const sources = new WeakMap<Event, unknown>();
/** A MessageEvent whose `source` the test controls, read through the prototype like a browser's. */
const NativeMessageEvent: new (type: string, init?: MessageEventInit) => MessageEvent = MessageEvent;
class PageMessageEvent extends NativeMessageEvent {}
for (const [name, descriptor] of Object.entries(
  Object.getOwnPropertyDescriptors(MessageEvent.prototype),
)) {
  if (name !== 'constructor')
    Object.defineProperty(PageMessageEvent.prototype, name, descriptor);
}
Object.defineProperty(PageMessageEvent.prototype, 'source', {
  get(this: Event) {
    return sources.get(this) ?? null;
  },
  configurable: true,
});

function pageWithParent() {
  const parent = {
    posted: [] as unknown[],
    targets: [] as unknown[],
    postMessage(data: unknown) {
      this.posted.push(data);
    },
  };
  const win = new EventTarget() as unknown as Window & typeof globalThis;
  Object.assign(win, {
    parent,
    postMessage: (data: unknown, target: unknown) => {
      parent.posted.push(data);
      parent.targets.push(target);
    },
    EventTarget,
    Event,
    MessageEvent: PageMessageEvent,
  });
  return { win, parent };
}

function send(
  win: Window & typeof globalThis,
  init: { source: unknown; data: unknown; ports?: MessagePort[]; origin?: string },
) {
  const event = new PageMessageEvent('message', {
    data: init.data,
    ports: init.ports ?? [],
    origin: init.origin ?? '',
  });
  sources.set(event, init.source);
  win.dispatchEvent(event);
}

describe('message port bootstrap', () => {
  test('announces itself to the parent and holds no secret', () => {
    const { win, parent } = pageWithParent();
    installMessagePortBootstrap(win, createMessagePortBridge());
    expect(parent.posted).toEqual([{ type: CONTAINER_READY }]);
  });

  test('a top-level page has no host to answer and reports so', () => {
    const win = new EventTarget() as unknown as Window & typeof globalThis;
    Object.assign(win, {
      EventTarget,
      Event,
      MessageEvent: PageMessageEvent,
      postMessage() {},
    });
    (win as { parent: unknown }).parent = win;
    expect(installMessagePortBootstrap(win, createMessagePortBridge())).toBe(
      false,
    );
  });

  test('takes the port from its parent and hides the message from product listeners', () => {
    const { win, parent } = pageWithParent();
    const bridge = createMessagePortBridge();
    installMessagePortBootstrap(win, bridge);
    const productSaw: unknown[] = [];
    win.addEventListener('message', (e) => productSaw.push(e));
    win.addEventListener('message', (e) => productSaw.push(e), true);

    const { port1, port2 } = new MessageChannel();
    send(win, {
      source: parent,
      data: { type: CONTAINER_INIT },
      ports: [port2],
    });
    expect(productSaw).toEqual([]);
    expect(bridge.attach(new MessageChannel().port1)).toBe(false);
    port1.close();
  });

  // The product's own scripts run after the container, but a window listener
  // added later in capture phase must still come second.
  test('runs before a product listener added later in either phase', () => {
    const { win, parent } = pageWithParent();
    installMessagePortBootstrap(win, createMessagePortBridge());
    const order: string[] = [];
    win.addEventListener('message', () => order.push('product-bubble'));
    win.addEventListener('message', () => order.push('product-capture'), true);
    const { port1, port2 } = new MessageChannel();
    send(win, {
      source: parent,
      data: { type: CONTAINER_INIT },
      ports: [port2],
    });
    expect(order).toEqual([]);
    port1.close();
  });

  test('ignores an init message from any window but its parent', () => {
    const { win } = pageWithParent();
    const bridge = createMessagePortBridge();
    installMessagePortBootstrap(win, bridge);
    const productSaw: unknown[] = [];
    win.addEventListener('message', (e) => productSaw.push(e), true);

    const stranger = { postMessage() {} };
    const { port1, port2 } = new MessageChannel();
    send(win, {
      source: stranger,
      data: { type: CONTAINER_INIT },
      ports: [port2],
    });
    expect(productSaw.length).toBe(1);
    const fresh = new MessageChannel();
    expect(bridge.attach(fresh.port2)).toBe(true);
    port1.close();
    fresh.port1.close();
  });

  test('ignores a message without exactly one port, or of another type', () => {
    const { win, parent } = pageWithParent();
    const bridge = createMessagePortBridge();
    installMessagePortBootstrap(win, bridge);
    const productSaw: unknown[] = [];
    win.addEventListener('message', (e) => productSaw.push(e), true);
    const a = new MessageChannel();
    const b = new MessageChannel();
    send(win, { source: parent, data: { type: CONTAINER_INIT }, ports: [] });
    send(win, {
      source: parent,
      data: { type: CONTAINER_INIT },
      ports: [a.port2, b.port2],
    });
    send(win, { source: parent, data: { type: 'something-else' }, ports: [] });
    send(win, { source: parent, data: 'text' });
    send(win, { source: parent, data: null });
    expect(productSaw.length).toBe(5);
    expect(bridge.attach(new MessageChannel().port1)).toBe(true);
    a.port1.close();
    b.port1.close();
  });

  // Once the host's port is taken, a second init must reach nobody, not even as
  // a way to replace the channel.
  test('takes the first port only; a later init is not honoured', () => {
    const { win, parent } = pageWithParent();
    const bridge = createMessagePortBridge();
    installMessagePortBootstrap(win, bridge);
    const first = new MessageChannel();
    const second = new MessageChannel();
    send(win, {
      source: parent,
      data: { type: CONTAINER_INIT },
      ports: [first.port2],
    });
    const productSaw: unknown[] = [];
    win.addEventListener('message', (e) => productSaw.push(e), true);
    send(win, {
      source: parent,
      data: { type: CONTAINER_INIT },
      ports: [second.port2],
    });
    expect(productSaw.length).toBe(1);
    first.port1.close();
    second.port1.close();
  });

  test('without an origin it addresses the parent by window alone', () => {
    const { win, parent } = pageWithParent();
    installMessagePortBootstrap(win, createMessagePortBridge());
    expect(parent.targets).toEqual(['*']);
  });

  // A subframe of the product has the product as its parent. Without the pin the
  // product could hand its own port to that subframe and answer for it.
  test('with an origin, it announces to that origin and takes a port from it only', () => {
    const { win, parent } = pageWithParent();
    const bridge = createMessagePortBridge();
    installMessagePortBootstrap(win, bridge, 'https://host.example');
    expect(parent.targets).toEqual(['https://host.example']);

    const productSaw: unknown[] = [];
    win.addEventListener('message', (e) => productSaw.push(e), true);
    const foreign = new MessageChannel();
    send(win, {
      source: parent,
      origin: 'https://product.example',
      data: { type: CONTAINER_INIT },
      ports: [foreign.port2],
    });
    expect(productSaw.length).toBe(1);
    const fresh = new MessageChannel();
    expect(bridge.attach(fresh.port2)).toBe(true);
    foreign.port1.close();
    fresh.port1.close();
  });

  test('with an origin, a message from that origin is taken', () => {
    const { win, parent } = pageWithParent();
    const bridge = createMessagePortBridge();
    installMessagePortBootstrap(win, bridge, 'https://host.example');
    const host = new MessageChannel();
    send(win, {
      source: parent,
      origin: 'https://host.example',
      data: { type: CONTAINER_INIT },
      ports: [host.port2],
    });
    expect(bridge.attach(new MessageChannel().port1)).toBe(false);
    host.port1.close();
  });
});
