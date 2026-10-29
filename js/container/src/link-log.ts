import type { HostConnection } from '@parity/truapi/internal';

interface TimedWindow {
  performance: { now(): number };
}

/**
 * Warn how long the host link stayed down once it is back.
 *
 * The host connection warns about each loss with its cause, so this adds only
 * the return, timed from the first loss through any refused redials. The first
 * connect after load is not a return and logs nothing.
 */
export function logHostLinkReturn(
  win: TimedWindow,
  subscribeConnectionStatus: HostConnection['subscribeConnectionStatus'],
): void {
  // Captured before product code runs, since a product can replace it.
  const now = win.performance.now.bind(win.performance);
  let connected = false;
  let lostAt: number | undefined;
  subscribeConnectionStatus((status) => {
    if (status === 'connected') {
      if (lostAt !== undefined) {
        console.warn(`[truapi] host link back after ${Math.round(now() - lostAt)} ms`);
      }
      connected = true;
      lostAt = undefined;
    } else if (status === 'disconnected' && connected) {
      connected = false;
      lostAt = now();
    }
  });
}
