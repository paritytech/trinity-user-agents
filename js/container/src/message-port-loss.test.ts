import { describe, expect, it } from 'bun:test';

import { reloadAfterMessagePortLoss } from './message-port-loss.js';

type Status = 'connecting' | 'connected' | 'disconnected';

function page(visibility: 'visible' | 'hidden', storage = new Map<string, string>()) {
  const timers = new Map<number, { callback: () => void; due: number }>();
  let nextTimer = 1;
  let now = 0;
  const visibilityListeners = new Set<() => void>();
  const statusListeners = new Set<(status: Status) => void>();
  let status: Status = 'disconnected';
  let reloads = 0;
  const win = {
    document: {
      visibilityState: visibility,
      addEventListener(type: string, listener: () => void) {
        if (type === 'visibilitychange') visibilityListeners.add(listener);
      },
      removeEventListener(type: string, listener: () => void) {
        if (type === 'visibilitychange') visibilityListeners.delete(listener);
      },
    },
    location: { reload: () => { reloads += 1; } },
    performance: { now: () => now },
    sessionStorage: {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => { storage.set(key, value); },
      removeItem: (key: string) => { storage.delete(key); },
    },
    setTimeout(callback: () => void, ms: number) {
      timers.set(nextTimer, { callback, due: now + ms });
      return nextTimer++;
    },
    clearTimeout(id: number | undefined) {
      if (id !== undefined) timers.delete(id);
    },
  };
  return {
    win,
    // Mirrors the host connection: a new subscriber hears the current status at
    // once, and later only changes.
    subscribeConnectionStatus(callback: (status: Status) => void) {
      statusListeners.add(callback);
      callback(status);
      return () => statusListeners.delete(callback);
    },
    loseConnection() {
      for (const next of ['connected', 'disconnected'] as const) {
        status = next;
        for (const listener of statusListeners) listener(next);
      }
    },
    setVisibility(next: 'visible' | 'hidden') {
      win.document.visibilityState = next;
      for (const listener of visibilityListeners) listener();
    },
    async expireProbe({ lateBy = 0 } = {}) {
      // Let a live port deliver before the probe deadline is forced.
      await new Promise((resolve) => setTimeout(resolve, 20));
      for (const [id, timer] of [...timers]) {
        timers.delete(id);
        now = timer.due + lateBy;
        timer.callback();
      }
    },
    reloads: () => reloads,
  };
}

describe('reloadAfterMessagePortLoss', () => {
  // WebKit disentangles every existing MessagePort when its networking process
  // dies, and a channel created before the loss never delivers again; only a
  // reload replaces it.
  it('reloads a page whose MessagePorts died with the host connection', async () => {
    const host = page('visible');
    const canary = new MessageChannel();
    reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);

    canary.port1.close();
    host.loseConnection();
    await host.expireProbe();

    expect(host.reloads()).toBe(1);
  });

  // A lost socket alone leaves every port working; the transport reconnects and
  // the page keeps its state.
  it('keeps a page whose MessagePorts survive a disconnect', async () => {
    const host = page('visible');
    const canary = new MessageChannel();
    reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);

    host.loseConnection();
    await host.expireProbe();

    expect(host.reloads()).toBe(0);
    canary.port1.close();
  });

  // A suspended page cannot answer in time, so its probe waits for the page to
  // return to the foreground, which is also when WebKit's loss becomes visible.
  it('probes a hidden page once it becomes visible', async () => {
    const host = page('hidden');
    const canary = new MessageChannel();
    reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);

    canary.port1.close();
    host.loseConnection();
    await host.expireProbe();
    const whileHidden = host.reloads();
    host.setVisibility('visible');
    await host.expireProbe();

    expect([whileHidden, host.reloads()]).toEqual([0, 1]);
  });

  // A main thread blocked past the deadline fires it late, possibly ahead of the
  // canary's queued reply, so the page probes again before deciding.
  it('probes again when the deadline fires late', async () => {
    const host = page('visible');
    const canary = new MessageChannel();
    reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);

    canary.port1.close();
    host.loseConnection();
    await host.expireProbe({ lateBy: 500 });
    const afterLateDeadline = host.reloads();
    await host.expireProbe();

    expect([afterLateDeadline, host.reloads()]).toEqual([0, 1]);
  });

  // The connection reports its current status to every new subscriber, and a
  // page starts out disconnected. That is not a lost connection, so loading a
  // page never arms a reload.
  it('leaves a freshly loaded page alone', async () => {
    const host = page('visible');
    const canary = new MessageChannel();
    canary.port1.close();
    reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);

    await host.expireProbe();

    expect(host.reloads()).toBe(0);
  });

  // A reload only helps if the fresh page's ports work. If they fail too, for
  // instance before the networking process is back, reloading again would loop
  // and lose the page's state each time; an answered probe ends that episode.
  it('reloads at most once until a probe is answered', async () => {
    const storage = new Map<string, string>();
    const reloadsPerLoad: number[] = [];
    for (const portsWorkAtLoad of [false, false, true]) {
      const host = page('visible', storage);
      const canary = new MessageChannel();
      if (!portsWorkAtLoad) canary.port1.close();
      reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);
      await host.expireProbe();
      canary.port1.close();
      host.loseConnection();
      await host.expireProbe();
      reloadsPerLoad.push(host.reloads());
    }

    expect(reloadsPerLoad).toEqual([1, 0, 1]);
  });

  // Product code runs after the container and can replace performance.now; the
  // deadline check keeps the clock from before any product script ran.
  it('keeps its own clock when the product replaces performance.now', async () => {
    const host = page('visible');
    const canary = new MessageChannel();
    reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);
    host.win.performance.now = () => 0;

    canary.port1.close();
    host.loseConnection();
    await host.expireProbe({ lateBy: 500 });

    expect(host.reloads()).toBe(0);
  });

  // A page being torn down disposes its connection, which may still report a
  // disconnect; reloading it then would revive a page the user left.
  it('stops probing once stopped', async () => {
    const host = page('visible');
    const canary = new MessageChannel();
    const stop = reloadAfterMessagePortLoss(host.win, host.subscribeConnectionStatus, canary);

    canary.port1.close();
    host.loseConnection();
    stop();
    host.loseConnection();
    host.setVisibility('visible');
    await host.expireProbe();

    expect(host.reloads()).toBe(0);
  });
});
