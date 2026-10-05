import { freezeValue } from './freeze.js';

export function installMediaPolicy(
  win: Pick<typeof globalThis, 'Promise' | 'TypeError' | 'DOMException'> & {
    navigator?: { mediaDevices?: object };
  },
): void {
  const navigator = win.navigator;
  if (!navigator) return;
  const descriptor = Object.getOwnPropertyDescriptor;
  const prototypeOf: (value: object) => object | null = Object.getPrototypeOf;
  const NativePromise = win.Promise;
  const NativeTypeError = win.TypeError;
  const NativeDOMException = win.DOMException;
  const apply = Reflect.apply;

  function denied(): DOMException {
    return new NativeDOMException(
      'Product capture is unavailable; use Core Media',
      'NotAllowedError',
    );
  }

  function lockMethod(
    target: object,
    name: string,
    method: (...args: unknown[]) => unknown,
  ): void {
    let owner: object | null = target;
    while (owner) {
      if (owner === target || descriptor(owner, name)) {
        freezeValue(owner, name, method);
      }
      owner = prototypeOf(owner);
    }
  }

  const devices = navigator.mediaDevices;
  for (const name of ['getUserMedia', 'getDisplayMedia']) {
    if (!devices || typeof Reflect.get(devices, name) !== 'function') continue;
    lockMethod(devices, name, function () {
      return new NativePromise(
        (_resolve: unknown, reject: (error: unknown) => void) => reject(denied()),
      );
    });
  }

  for (const name of [
    'getUserMedia',
    'webkitGetUserMedia',
    'mozGetUserMedia',
    'msGetUserMedia',
  ]) {
    if (typeof Reflect.get(navigator, name) !== 'function') continue;
    lockMethod(navigator, name, function (_input: unknown, _success: unknown, failure: unknown) {
      if (typeof failure !== 'function') {
        throw new NativeTypeError('A capture failure callback is required');
      }
      apply(failure, undefined, [denied()]);
    });
  }
}
