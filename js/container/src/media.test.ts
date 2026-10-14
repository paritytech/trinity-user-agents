import { describe, expect, it } from 'bun:test';

import { installMediaPolicy } from './media.js';

/* eslint-disable @typescript-eslint/no-explicit-any */

function realm() {
  const calls: any[] = [];
  const stream = { id: 'native-stream' };
  let failure: unknown;
  class MediaDevices {
    getDisplayMedia() {
      calls.push('display');
      return Promise.resolve(stream);
    }
    getUserMedia(constraints: any) {
      calls.push(constraints);
      return failure ? Promise.reject(failure) : Promise.resolve(stream);
    }
  }
  class Navigator {
    mediaDevices = new MediaDevices();
    getUserMedia(
      constraints: any,
      success: (value: unknown) => void,
      error: (value: unknown) => void,
    ) {
      calls.push(constraints);
      if (failure) error(failure);
      else success(stream);
    }
    webkitGetUserMedia(
      constraints: any,
      success: (value: unknown) => void,
      error: (value: unknown) => void,
    ) {
      calls.push(constraints);
      if (failure) error(failure);
      else success(stream);
    }
  }
  const win = {
    navigator: new Navigator(),
    MediaDevices,
    Navigator,
    Promise,
    TypeError,
    DOMException,
  };
  return {
    win,
    calls,
    stream,
    fail(error: unknown) {
      failure = error;
    },
  };
}

function gated() {
  const fixture = realm();
  const requests: Array<{
    audio: boolean;
    video: boolean;
    decide: (allowed: boolean) => void;
  }> = [];
  installMediaPolicy(
    fixture.win,
    (audio: boolean, video: boolean, decide: (allowed: boolean) => void) => {
      requests.push({ audio, video, decide });
      return () => {};
    },
  );
  return { ...fixture, requests };
}

describe('media capture permission', () => {
  it('authorizes every capture independently, including while an earlier stream remains live', async () => {
    const { win, calls, stream, requests } = gated();
    const first = win.navigator.mediaDevices.getUserMedia({ video: true });
    expect([requests.length, calls.length]).toEqual([1, 0]);
    requests[0]!.decide(true);
    expect(await first).toBe(stream);
    const second = win.navigator.mediaDevices.getUserMedia({ video: true });
    expect([requests.length, calls.length]).toEqual([2, 1]);
    requests[1]!.decide(false);
    requests[1]!.decide(true);
    await expect(second).rejects.toMatchObject({ name: 'NotAllowedError' });
    expect(calls).toEqual([{ audio: false, video: true }]);
  });

  it('does not share an approval between overlapping capture calls', async () => {
    const { win, calls, requests } = gated();
    const first = win.navigator.mediaDevices.getUserMedia({ audio: true });
    const second = win.navigator.mediaDevices.getUserMedia({
      audio: true,
      video: true,
    });
    expect([requests.length, calls.length]).toEqual([2, 0]);
    requests[1]!.decide(false);
    requests[0]!.decide(true);
    await first;
    await expect(second).rejects.toMatchObject({ name: 'NotAllowedError' });
    expect(calls).toEqual([{ audio: true, video: false }]);
  });

  it('preserves the native stream and native rejection', async () => {
    const { win, stream, requests, fail } = gated();
    const approved = win.navigator.mediaDevices.getUserMedia({ video: true });
    requests[0]!.decide(true);
    expect(await approved).toBe(stream);
    const error = new DOMException('Camera unavailable', 'NotReadableError');
    fail(error);
    const rejected = win.navigator.mediaDevices.getUserMedia({ video: true });
    requests[1]!.decide(true);
    await expect(rejected).rejects.toBe(error);
  });

  it('denies capture when there is no authorization transport', async () => {
    const { win, calls } = realm();
    installMediaPolicy(win, false);
    await expect(
      win.navigator.mediaDevices.getUserMedia({ video: true }),
    ).rejects.toMatchObject({ name: 'NotAllowedError' });
    expect(calls).toEqual([]);
  });
});

describe('constraints agree with the authorized media types', () => {
  it('snapshots inherited audio/video getters once before permission yields', async () => {
    const { win, calls, requests } = gated();
    let audioReads = 0;
    let videoReads = 0;
    const video = { width: { ideal: 640 }, facingMode: 'environment' };
    const prototype = {
      get audio() {
        audioReads += 1;
        return false;
      },
      get video() {
        videoReads += 1;
        return video;
      },
    };
    const constraints = Object.create(prototype);
    const pending = win.navigator.mediaDevices.getUserMedia(constraints);
    Object.defineProperty(constraints, 'audio', { value: true });
    Object.defineProperty(constraints, 'video', { value: false });
    requests[0]!.decide(true);
    await pending;
    expect([
      audioReads,
      videoReads,
      requests[0]!.audio,
      requests[0]!.video,
      calls,
    ]).toEqual([1, 1, false, true, [{ audio: false, video }]]);
    expect(calls[0].video).toBe(video);
  });

  it('uses WebIDL union conversion for null, objects and primitive values', async () => {
    const cases: Array<[unknown, boolean]> = [
      [undefined, false],
      [false, false],
      [0, false],
      ['', false],
      [NaN, false],
      [0n, false],
      [null, true],
      [{}, true],
      [new Boolean(false), true],
      [() => {}, true],
      [true, true],
      [1, true],
      ['false', true],
      [1n, true],
      [Symbol('audio'), true],
    ];
    for (const [audio, expectedAudio] of cases) {
      const { win, calls, requests } = gated();
      const pending = win.navigator.mediaDevices.getUserMedia({
        audio,
        video: true,
      });
      expect([requests[0]!.audio, requests[0]!.video]).toEqual([
        expectedAudio,
        true,
      ]);
      requests[0]!.decide(true);
      await pending;
      expect(calls[0].video).toBe(true);
      if (audio === null) expect(calls[0].audio).toEqual({});
      else if (typeof audio === 'object' || typeof audio === 'function')
        expect(calls[0].audio).toBe(audio);
      else expect(calls[0].audio).toBe(expectedAudio);
    }
  });

  it('rejects empty or invalid constraints before consuming permission', async () => {
    const { win, calls, requests } = gated();
    for (const constraints of [
      undefined,
      null,
      {},
      { audio: false, video: false },
      1,
      'video',
    ]) {
      await expect(
        win.navigator.mediaDevices.getUserMedia(constraints),
      ).rejects.toBeInstanceOf(TypeError);
    }
    const error = new Error('getter failed');
    await expect(
      win.navigator.mediaDevices.getUserMedia({
        get video() {
          throw error;
        },
      }),
    ).rejects.toBe(error);
    expect([requests, calls]).toEqual([[], []]);
  });
});

describe('capture entry points stay protected', () => {
  it('guards prototype methods and blocks unsupported screen capture without consuming consent', async () => {
    const { win, requests, calls } = gated();
    const method = Object.getPrototypeOf(
      win.navigator.mediaDevices,
    ).getUserMedia;
    expect(() => {
      delete (win.MediaDevices.prototype as any).getUserMedia;
    }).toThrow();
    expect(() =>
      Object.defineProperty(win.navigator.mediaDevices, 'getUserMedia', {
        value: () => {},
      }),
    ).toThrow();
    await expect(method.call({}, { video: true })).rejects.toBeInstanceOf(
      TypeError,
    );
    expect(requests).toEqual([]);
    const pending = method.call(win.navigator.mediaDevices, { video: true });
    requests[0]!.decide(false);
    await expect(pending).rejects.toMatchObject({ name: 'NotAllowedError' });
    await expect(
      win.navigator.mediaDevices.getDisplayMedia(),
    ).rejects.toMatchObject({ name: 'NotAllowedError' });
    await expect(
      win.MediaDevices.prototype.getDisplayMedia.call(
        win.navigator.mediaDevices,
      ),
    ).rejects.toMatchObject({ name: 'NotAllowedError' });
    expect(() => {
      delete (win.MediaDevices.prototype as any).getDisplayMedia;
    }).toThrow();
    expect([requests.length, calls]).toEqual([1, []]);
  });

  it('routes standard and prefixed callback APIs through the same per-call check', () => {
    const { win, stream, calls, requests } = gated();
    let result: unknown;
    let error: unknown;
    const returned = win.navigator.getUserMedia(
      { video: true },
      (value) => {
        result = value;
      },
      (value) => {
        error = value;
      },
    );
    expect([returned, result, calls.length]).toEqual([undefined, undefined, 0]);
    requests[0]!.decide(true);
    expect(result).toBe(stream);
    const legacy = Object.getPrototypeOf(win.navigator).webkitGetUserMedia;
    legacy.call(
      win.navigator,
      { audio: true },
      () => {},
      (failure: unknown) => {
        error = failure;
      },
    );
    requests[1]!.decide(false);
    expect([calls.length, (error as DOMException).name]).toEqual([
      1,
      'NotAllowedError',
    ]);
    expect(() => {
      delete (win.Navigator.prototype as any).webkitGetUserMedia;
    }).toThrow();
  });

  it('also protects callback-only browsers', () => {
    const fixture = realm();
    const { win, calls, stream } = fixture;
    delete (win.navigator as any).mediaDevices;
    let decide!: (allowed: boolean) => void;
    installMediaPolicy(
      win,
      (_audio: boolean, _video: boolean, callback: typeof decide) => {
        decide = callback;
        return () => {};
      },
    );
    let result: unknown;
    win.navigator.webkitGetUserMedia(
      { video: true },
      (value) => {
        result = value;
      },
      () => {},
    );
    expect(calls).toEqual([]);
    decide(true);
    expect(result).toBe(stream);
  });

  it('locks the native prototype even when the host has installed an instance method', async () => {
    const { win, calls } = realm();
    const native = win.navigator.mediaDevices.getUserMedia;
    win.navigator.mediaDevices.getUserMedia = (constraints) =>
      native.call(win.navigator.mediaDevices, constraints);
    installMediaPolicy(
      win,
      (
        _audio: boolean,
        _video: boolean,
        decide: (allowed: boolean) => void,
      ) => {
        decide(false);
        return () => {};
      },
    );
    const recovered = win.MediaDevices.prototype.getUserMedia;
    await expect(
      recovered.call(win.navigator.mediaDevices, { video: true }),
    ).rejects.toMatchObject({ name: 'NotAllowedError' });
    expect(calls).toEqual([]);
  });

  it('does not expose authority through replaced Promise, Reflect or collection methods', async () => {
    const { win, calls, requests } = gated();
    const original = {
      then: Promise.prototype.then,
      apply: Reflect.apply,
      set: Map.prototype.set,
    };
    const stolen: unknown[] = [];
    let pending!: Promise<unknown>;
    try {
      Promise.prototype.then = function (...args: any[]): any {
        stolen.push(args);
        return this;
      };
      Reflect.apply = function (...args: any[]): any {
        stolen.push(args);
        return true;
      };
      Map.prototype.set = function (...args: any[]) {
        stolen.push(args);
        return this;
      };
      pending = win.navigator.mediaDevices.getUserMedia({ video: true });
    } finally {
      Promise.prototype.then = original.then;
      Reflect.apply = original.apply;
      Map.prototype.set = original.set;
    }
    expect([stolen, calls, requests.length]).toEqual([[], [], 1]);
    requests[0]!.decide(false);
    await expect(pending).rejects.toMatchObject({ name: 'NotAllowedError' });
  });

  it('does not read product-defined descriptor defaults while snapshotting media choices', async () => {
    const { win, calls, requests } = gated();
    let inheritedDescriptorReads = 0;
    let pending!: Promise<unknown>;
    Object.defineProperty(Object.prototype, 'configurable', {
      get() {
        inheritedDescriptorReads += 1;
        return false;
      },
      configurable: true,
    });
    try {
      pending = win.navigator.mediaDevices.getUserMedia({
        audio: false,
        video: true,
      });
    } finally {
      delete (Object.prototype as any).configurable;
    }
    expect(inheritedDescriptorReads).toBe(0);
    requests[0]!.decide(true);
    await pending;
    expect(calls).toEqual([{ audio: false, video: true }]);
  });
});
