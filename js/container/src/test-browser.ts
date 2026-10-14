import { build } from 'esbuild';

export async function browserScript(contents: string): Promise<string> {
  const result = await build({
    stdin: { contents, resolveDir: import.meta.dir, loader: 'ts' },
    bundle: true,
    format: 'iife',
    target: 'es2020',
    write: false,
  });
  return result.outputFiles[0].text;
}

export function browserGlobals() {
  class BrowserEvents extends EventTarget {}
  const NativeMessageEvent: new (type: string, init?: MessageEventInit) => MessageEvent = MessageEvent;
  class BrowserMessage extends NativeMessageEvent {}
  class BrowserEncoder extends TextEncoder {}
  class BrowserDecoder extends TextDecoder {}
  for (const [target, original] of [
    [BrowserEvents, EventTarget], [BrowserMessage, MessageEvent],
    [BrowserEncoder, TextEncoder], [BrowserDecoder, TextDecoder],
  ] as const) {
    for (const [name, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(original.prototype))) {
      if (name !== 'constructor') Object.defineProperty(target.prototype, name, descriptor);
    }
  }
  return {
    performance: { now: () => performance.now() },
    EventTarget: BrowserEvents,
    MessageEvent: BrowserMessage,
    TextEncoder: BrowserEncoder,
    TextDecoder: BrowserDecoder,
  };
}

const { buffer, byteOffset, byteLength } = Object.getOwnPropertyDescriptors(Object.getPrototypeOf(Uint8Array.prototype));

export function frameBytes(value: Uint8Array): Uint8Array {
  return new Uint8Array(buffer.get!.call(value), byteOffset.get!.call(value), byteLength.get!.call(value));
}
