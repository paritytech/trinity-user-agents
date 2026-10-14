import { describe, expect, it } from 'bun:test';

import { installWebRtcPolicy } from './webrtc.js';

/* eslint-disable @typescript-eslint/no-explicit-any */

function realm(): any {
  class NativePeerConnection {
    operations: Array<[string, unknown[]]> = [];
    pools: number[] = [];
    closed = false;
    configuration: any;

    constructor(configuration: any = {}) {
      this.configuration = {
        iceCandidatePoolSize: configuration?.iceCandidatePoolSize ?? 0,
        iceServers: configuration?.iceServers ?? [],
      };
      this.pools.push(this.configuration.iceCandidatePoolSize);
    }
    createOffer(...args: any[]) {
      this.operations.push(['createOffer', args]);
      const offer = { sdp: 'native', type: 'offer' };
      if (typeof args[0] === 'function') args[0](offer);
      return Promise.resolve(offer);
    }
    createAnswer(...args: any[]) {
      this.operations.push(['createAnswer', args]);
      return Promise.resolve({ sdp: 'native', type: 'answer' });
    }
    setLocalDescription(...args: any[]) {
      this.operations.push(['setLocalDescription', args]);
      return Promise.resolve();
    }
    setRemoteDescription(...args: any[]) {
      this.operations.push(['setRemoteDescription', args]);
      return Promise.resolve();
    }
    addIceCandidate(...args: any[]) {
      this.operations.push(['addIceCandidate', args]);
      return Promise.resolve();
    }
    getConfiguration() {
      return { ...this.configuration };
    }
    setConfiguration(configuration: any) {
      this.configuration = {
        iceCandidatePoolSize: configuration.iceCandidatePoolSize ?? 0,
        iceServers: configuration.iceServers ?? [],
      };
      this.pools.push(this.configuration.iceCandidatePoolSize);
    }
    close() {
      this.closed = true;
    }
    createDataChannel() {
      return { send() {} };
    }
  }
  return {
    RTCPeerConnection: NativePeerConnection,
    webkitRTCPeerConnection: NativePeerConnection,
    Promise,
    TypeError,
  };
}

function gated() {
  const win = realm();
  const decisions: Array<(allowed: boolean) => void> = [];
  let cancelled = 0;
  installWebRtcPolicy(win, (decide) => {
    decisions.push(decide);
    return () => {
      cancelled += 1;
    };
  });
  return { win, decisions, cancellations: () => cancelled };
}

function denied(): any {
  const win = realm();
  installWebRtcPolicy(win, false);
  return win;
}

describe('connection permission', () => {
  it('shares one decision across concurrent methods and reuses it only for that connection', async () => {
    const { win, decisions } = gated();
    const first = new win.RTCPeerConnection();
    const offer = first.createOffer({ iceRestart: true });
    const answer = first.createAnswer();
    expect([decisions.length, first.operations]).toEqual([1, []]);
    decisions[0]!(true);
    expect(await offer).toEqual({ sdp: 'native', type: 'offer' });
    expect(await answer).toEqual({ sdp: 'native', type: 'answer' });
    await first.setLocalDescription({ type: 'offer', sdp: 'native' });
    await first.setRemoteDescription({ type: 'answer', sdp: 'remote' });
    await first.addIceCandidate({ candidate: 'candidate' });
    expect([decisions.length, first.operations]).toEqual([
      1,
      [
        ['createOffer', [{ iceRestart: true }]],
        ['createAnswer', []],
        ['setLocalDescription', [{ type: 'offer', sdp: 'native' }]],
        ['setRemoteDescription', [{ type: 'answer', sdp: 'remote' }]],
        ['addIceCandidate', [{ candidate: 'candidate' }]],
      ],
    ]);
    const second = new win.RTCPeerConnection();
    const rejected = second.createOffer();
    decisions[1]!(false);
    await expect(rejected).rejects.toThrow('WebRTC access is not allowed');
    expect([decisions.length, second.closed, second.operations]).toEqual([
      2,
      true,
      [],
    ]);
  });

  it('closes a denied connection and cannot revive it with a late approval', async () => {
    const { win, decisions } = gated();
    const connection = new win.RTCPeerConnection();
    const pending = connection.createOffer();
    decisions[0]!(false);
    decisions[0]!(true);
    await expect(pending).rejects.toThrow('WebRTC access is not allowed');
    await expect(connection.createAnswer()).rejects.toThrow(
      'WebRTC access is not allowed',
    );
    expect([
      connection.operations,
      decisions.length,
      connection.closed,
    ]).toEqual([[], 1, true]);
  });

  it('cancels pending authorization when closed without running queued methods', async () => {
    const { win, decisions, cancellations } = gated();
    const connection = new win.RTCPeerConnection();
    const pending = connection.createOffer();
    connection.close();
    decisions[0]!(true);
    await expect(pending).rejects.toThrow('WebRTC connection is closed');
    await expect(connection.createOffer()).rejects.toThrow(
      'WebRTC connection is closed',
    );
    expect([connection.operations, connection.closed, cancellations()]).toEqual(
      [[], true, 1],
    );
  });

  it('supports immediate decisions and native callback arguments', async () => {
    const win = realm();
    installWebRtcPolicy(win, (decide) => {
      decide(true);
      return () => {};
    });
    const connection = new win.RTCPeerConnection();
    let received: unknown;
    await connection.createOffer(
      (offer: unknown) => {
        received = offer;
      },
      () => {},
    );
    expect(received).toEqual({ sdp: 'native', type: 'offer' });
  });

  it('reports denial through a supplied legacy error callback', async () => {
    const { win, decisions } = gated();
    const connection = new win.RTCPeerConnection();
    let failure: unknown;
    const pending = connection.createOffer(
      () => {},
      (error: unknown) => {
        failure = error;
      },
    );
    decisions[0]!(false);
    await pending;
    expect(String(failure)).toContain('WebRTC access is not allowed');
    expect(connection.operations).toEqual([]);
  });
});

describe('ICE pooling before consent', () => {
  it('defers constructor pooling and preserves inherited and nonenumerable configuration', async () => {
    const { win, decisions } = gated();
    const configuration = Object.create({
      iceServers: [{ urls: 'stun:example.com' }],
    });
    Object.defineProperty(configuration, 'iceCandidatePoolSize', { value: 4 });
    const connection = new win.RTCPeerConnection(configuration);
    expect([
      connection.pools,
      connection.getConfiguration(),
      decisions.length,
    ]).toEqual([
      [0],
      { iceCandidatePoolSize: 4, iceServers: [{ urls: 'stun:example.com' }] },
      0,
    ]);
    const pending = connection.createOffer();
    decisions[0]!(true);
    await pending;
    expect(connection.pools).toEqual([0, 4]);
  });

  it('defers setConfiguration pooling and applies only the latest requested value', async () => {
    const { win, decisions } = gated();
    const connection = new win.RTCPeerConnection({ iceCandidatePoolSize: 4 });
    connection.setConfiguration({ iceCandidatePoolSize: 7 });
    expect(connection.pools).toEqual([0, 0]);
    const pending = connection.createOffer();
    connection.setConfiguration({ iceCandidatePoolSize: 2 });
    decisions[0]!(true);
    await pending;
    connection.setConfiguration({ iceCandidatePoolSize: 3 });
    expect(connection.pools).toEqual([0, 0, 0, 2, 3]);
  });

  it('validates the WebIDL octet range without starting ICE', () => {
    const { win } = gated();
    for (const iceCandidatePoolSize of [
      -1,
      256,
      NaN,
      Infinity,
      1n,
      Symbol('pool'),
    ]) {
      expect(
        () => new win.RTCPeerConnection({ iceCandidatePoolSize }),
      ).toThrow();
    }
    const connection = new win.RTCPeerConnection({
      iceCandidatePoolSize: '2.8',
    });
    expect([
      connection.pools,
      connection.getConfiguration().iceCandidatePoolSize,
    ]).toEqual([[0], 2]);
  });

  it('cannot start pooling through the recovered prototype constructor or setConfiguration', async () => {
    const { win, decisions } = gated();
    const Constructor = win.RTCPeerConnection.prototype.constructor;
    const connection = new Constructor({ iceCandidatePoolSize: 8 });
    Object.getPrototypeOf(connection).setConfiguration.call(connection, {
      iceCandidatePoolSize: 9,
    });
    expect(connection.pools).toEqual([0, 0]);
    const pending = connection.createOffer();
    decisions[0]!(false);
    await expect(pending).rejects.toThrow();
    expect(connection.pools).toEqual([0, 0]);
  });
});

describe('authorization cannot be forged by product code', () => {
  it('keeps native methods guarded after prototype and constructor recovery', async () => {
    const { win, decisions } = gated();
    const Constructor = win.RTCPeerConnection;
    const connection = new Constructor();
    expect(Object.getPrototypeOf(Constructor)).toBe(Function.prototype);
    expect(Constructor.prototype.constructor).toBe(Constructor);
    expect(connection instanceof Constructor).toBe(true);
    expect(() => {
      delete Constructor.prototype.createOffer;
    }).toThrow();
    expect(() =>
      Object.defineProperty(Constructor.prototype, 'createOffer', {
        value: () => true,
      }),
    ).toThrow();
    const pending =
      Object.getPrototypeOf(connection).createOffer.call(connection);
    decisions[0]!(false);
    await expect(pending).rejects.toThrow();
    expect(connection.operations).toEqual([]);
  });

  it('protects each vendor constructor and does not wrap shared aliases twice', async () => {
    const { win, decisions } = gated();
    expect(win.webkitRTCPeerConnection).toBe(win.RTCPeerConnection);
    const connection = new win.webkitRTCPeerConnection();
    const pending = connection.createOffer();
    decisions[0]!(true);
    await pending;
    expect([decisions.length, connection.operations.length]).toEqual([1, 1]);
    const separate = realm();
    separate.webkitRTCPeerConnection = realm().RTCPeerConnection;
    installWebRtcPolicy(separate, (decide) => {
      decide(false);
      return () => {};
    });
    await expect(
      new separate.webkitRTCPeerConnection().createOffer(),
    ).rejects.toThrow();

    const shared = realm();
    const Native = shared.RTCPeerConnection;
    const Alias = function (...args: any[]) {
      return Reflect.construct(Native, args);
    };
    Alias.prototype = Native.prototype;
    shared.webkitRTCPeerConnection = Alias;
    installWebRtcPolicy(shared, (decide) => {
      decide(true);
      return () => {};
    });
    expect(shared.webkitRTCPeerConnection).toBe(shared.RTCPeerConnection);
    await expect(
      new shared.webkitRTCPeerConnection().createOffer(),
    ).resolves.toMatchObject({ sdp: 'native' });
  });

  it('does not expose pending state to replaced collection or Promise primitives', async () => {
    const { win, decisions } = gated();
    const original = {
      mapSet: Map.prototype.set,
      weakSet: WeakMap.prototype.set,
      weakGet: WeakMap.prototype.get,
      then: Promise.prototype.then,
      apply: Reflect.apply,
    };
    const stolen: unknown[] = [];
    let pending: Promise<unknown>;
    let connection: any;
    try {
      Map.prototype.set = function (...args: any[]) {
        stolen.push(args);
        return this;
      };
      WeakMap.prototype.set = function (...args: any[]) {
        stolen.push(args);
        return this;
      };
      WeakMap.prototype.get = function (...args: any[]) {
        stolen.push(args);
        return { phase: 'allowed' };
      };
      Promise.prototype.then = function (...args: any[]): any {
        stolen.push(args);
        return this;
      };
      Reflect.apply = function (...args: any[]): any {
        stolen.push(args);
        return true;
      };
      connection = new win.RTCPeerConnection();
      pending = connection.createOffer();
    } finally {
      Map.prototype.set = original.mapSet;
      WeakMap.prototype.set = original.weakSet;
      WeakMap.prototype.get = original.weakGet;
      Promise.prototype.then = original.then;
      Reflect.apply = original.apply;
    }
    expect([stolen, connection.operations, decisions.length]).toEqual([
      [],
      [],
      1,
    ]);
    decisions[0]!(false);
    await expect(pending!).rejects.toThrow();
    expect(connection.operations).toEqual([]);
  });
});

describe('an unsupported host stays disabled', () => {
  it('removes standard and prefixed constructors', () => {
    const win = denied();
    expect([win.RTCPeerConnection, win.webkitRTCPeerConnection]).toEqual([
      undefined,
      undefined,
    ]);
  });

  it('cannot be restored by assignment or redefining the property', () => {
    const win = denied();
    win.RTCPeerConnection = realm().RTCPeerConnection;
    expect(win.RTCPeerConnection).toBeUndefined();
    expect(() =>
      Object.defineProperty(win, 'RTCPeerConnection', {
        value: realm().RTCPeerConnection,
      }),
    ).toThrow();
  });
});
