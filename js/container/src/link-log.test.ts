import { describe, expect, it } from 'bun:test';

import { logHostLinkReturn } from './link-log.js';

type Status = 'connecting' | 'connected' | 'disconnected';

function link() {
  let now = 0;
  let status: Status = 'disconnected';
  const listeners = new Set<(status: Status) => void>();
  return {
    win: { performance: { now: () => now } },
    // Mirrors the host connection: a new subscriber hears the current status at
    // once, and later only changes.
    subscribeConnectionStatus(callback: (status: Status) => void) {
      listeners.add(callback);
      callback(status);
      return () => listeners.delete(callback);
    },
    advance(ms: number) {
      now += ms;
    },
    set(next: Status) {
      status = next;
      for (const listener of listeners) listener(next);
    },
  };
}

function warningsDuring(run: () => void): unknown[][] {
  const warnings: unknown[][] = [];
  const originalWarn = console.warn;
  console.warn = (...args: unknown[]) => {
    warnings.push(args);
  };
  try {
    run();
  } finally {
    console.warn = originalWarn;
  }
  return warnings;
}

describe('logHostLinkReturn', () => {
  it('reports the gap from the first loss when the link comes back', () => {
    const host = link();
    const warnings = warningsDuring(() => {
      logHostLinkReturn(host.win, host.subscribeConnectionStatus);
      host.set('connecting');
      host.set('connected');
      host.set('disconnected');
      host.advance(100);
      host.set('connecting');
      host.set('disconnected');
      host.advance(40.4);
      host.set('connecting');
      host.set('connected');
    });
    expect(warnings).toEqual([['[truapi] host link back after 140 ms']]);
  });

  it('stays quiet on the first connect', () => {
    const host = link();
    const warnings = warningsDuring(() => {
      logHostLinkReturn(host.win, host.subscribeConnectionStatus);
      host.advance(500);
      host.set('connecting');
      host.set('connected');
    });
    expect(warnings).toEqual([]);
  });
});
