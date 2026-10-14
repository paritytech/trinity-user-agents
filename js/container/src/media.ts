/* eslint-disable @typescript-eslint/no-explicit-any */

import { freezeValue } from './freeze.js';
import type { MediaAuthorization } from './network-transport.js';

export function installMediaPolicy(
  win: any,
  authorize: MediaAuthorization | false,
): void {
  const navigator = win.navigator;
  if (!navigator) return;
  const devices = navigator.mediaDevices;
  const nativeGetUserMedia = devices?.getUserMedia;
  const apply = Reflect.apply;
  const get = Reflect.get;
  const object = Object;
  const create = Object.create;
  const freeze = Object.freeze;
  const descriptor = Object.getOwnPropertyDescriptor;
  const prototypeOf = Object.getPrototypeOf;
  const NativePromise = win.Promise;
  const NativeTypeError = win.TypeError;
  const NativeDOMException = win.DOMException;

  function trackConstraints(value: any): any {
    if (value === undefined) return false;
    // The dictionary arm of the WebIDL union makes null request a track too.
    if (value === null) return create(null);
    return object(value) === value ? value : !!value;
  }

  function snapshot(input: any): MediaStreamConstraints {
    if (input !== null && input !== undefined && object(input) !== input) {
      throw new NativeTypeError('MediaStreamConstraints must be an object');
    }
    const missing = input === null || input === undefined;
    const audio = trackConstraints(
      missing ? undefined : get(input, 'audio', input),
    );
    const video = trackConstraints(
      missing ? undefined : get(input, 'video', input),
    );
    if (audio === false && video === false) {
      throw new NativeTypeError(
        'At least one of audio or video must be requested',
      );
    }
    const constraints = create(null);
    constraints.audio = audio;
    constraints.video = video;
    return freeze(constraints);
  }

  function capture(
    input: any,
    invoke: (constraints: MediaStreamConstraints) => void,
    reject: (error: unknown) => void,
  ): void {
    let settled = false;
    let cancel: (() => void) | undefined;
    try {
      const constraints = snapshot(input);
      function decided(allowed: boolean): void {
        if (settled) return;
        settled = true;
        cancel?.();
        if (allowed !== true) {
          reject(
            new NativeDOMException(
              'Media capture is not allowed',
              'NotAllowedError',
            ),
          );
          return;
        }
        try {
          invoke(constraints);
        } catch (error) {
          reject(error);
        }
      }
      if (typeof authorize !== 'function') {
        decided(false);
        return;
      }
      const cancellation = authorize(
        constraints.audio !== false,
        constraints.video !== false,
        decided,
      );
      if (settled) cancellation();
      else cancel = cancellation;
    } catch (error) {
      settled = true;
      cancel?.();
      reject(error);
    }
  }

  function lockMethod(
    target: any,
    name: string,
    method: (...args: any[]) => any,
  ): void {
    let owner = target;
    while (owner) {
      if (owner === target || descriptor(owner, name))
        freezeValue(owner, name, method);
      owner = prototypeOf(owner);
    }
  }

  if (typeof devices?.getDisplayMedia === 'function') {
    lockMethod(devices, 'getDisplayMedia', function () {
      return new NativePromise(
        (_resolve: unknown, reject: (error: unknown) => void) => {
          reject(
            new NativeDOMException(
              'Screen capture is not allowed',
              'NotAllowedError',
            ),
          );
        },
      );
    });
  }

  if (typeof nativeGetUserMedia === 'function') {
    lockMethod(devices, 'getUserMedia', function (this: any, input: any) {
      return new NativePromise(
        (resolve: (value: any) => void, reject: (error: unknown) => void) => {
          if (this !== devices) {
            reject(new NativeTypeError('Invalid MediaDevices receiver'));
            return;
          }
          capture(
            input,
            (constraints) =>
              resolve(apply(nativeGetUserMedia, devices, [constraints])),
            reject,
          );
        },
      );
    });
  }

  for (const name of [
    'getUserMedia',
    'webkitGetUserMedia',
    'mozGetUserMedia',
    'msGetUserMedia',
  ]) {
    const native = navigator[name];
    if (typeof native !== 'function') continue;
    lockMethod(
      navigator,
      name,
      function (this: any, input: any, success: any, failure: any) {
        if (
          this !== navigator ||
          typeof success !== 'function' ||
          typeof failure !== 'function'
        ) {
          throw new NativeTypeError(
            'Invalid getUserMedia receiver or callbacks',
          );
        }
        capture(
          input,
          (constraints) => {
            apply(native, navigator, [constraints, success, failure]);
          },
          (error) => {
            apply(failure, undefined, [error]);
          },
        );
      },
    );
  }
}
