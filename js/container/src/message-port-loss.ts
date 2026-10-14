import type { HostConnection } from '@parity/truapi/internal';

// A live port delivers within one task; the margin only absorbs a busy main thread.
const PROBE_TIMEOUT_MS = 1000;
// A deadline this late means the main thread was blocked, and the canary's reply may
// still be queued behind it.
const LATE_DEADLINE_MS = 250;
// Outlives the reload, so a page whose fresh ports fail too is not reloaded again.
const RELOADED_KEY = '__truapi_message_port_reload';

interface ProbedWindow {
  document: {
    visibilityState: DocumentVisibilityState;
    addEventListener(type: 'visibilitychange', listener: () => void): void;
    removeEventListener(type: 'visibilitychange', listener: () => void): void;
  };
  location: { reload(): void };
  performance: { now(): number };
  sessionStorage: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
  setTimeout(callback: () => void, ms: number): number;
  clearTimeout(id: number | undefined): void;
}

/**
 * Reload the page once WebKit has disentangled its MessagePorts.
 *
 * WebKit brokers every MessagePort through its networking process. Losing that
 * process closes every existing port without an error or an event, while ports
 * created afterwards work. The host connection recovers on its own, but a
 * channel created before the loss never delivers again. A canary channel
 * created with the container tells that loss apart from a plain socket loss,
 * which keeps the page. A probe follows each lost connection once the page is
 * visible, and the page reloads at most once until a probe is answered, which
 * a reloaded page confirms at load. Returns a function that stops watching.
 */
export function reloadAfterMessagePortLoss(
  win: ProbedWindow,
  subscribeConnectionStatus: HostConnection['subscribeConnectionStatus'],
  canary: MessageChannel = new MessageChannel(),
): () => void {
  // Captured before product code runs, since a product can replace these globals.
  const now = win.performance.now.bind(win.performance);
  const storage = sessionStorageOf(win);
  let lost = storage?.getItem(RELOADED_KEY) != null;
  let deadline: number | undefined;

  // Listening only while probing, since a port with a listener keeps script
  // hosts such as Bun alive.
  function settle(): void {
    win.clearTimeout(deadline);
    deadline = undefined;
    canary.port1.onmessage = null;
  }

  function answered(): void {
    lost = false;
    storage?.removeItem(RELOADED_KEY);
    settle();
  }

  function check(): void {
    if (!lost || deadline !== undefined || win.document.visibilityState !== 'visible') return;
    const due = now() + PROBE_TIMEOUT_MS;
    deadline = win.setTimeout(() => {
      settle();
      if (now() - due > LATE_DEADLINE_MS) check();
      else reload();
    }, PROBE_TIMEOUT_MS);
    canary.port1.onmessage = answered;
    canary.port2.postMessage(null);
  }

  function reload(): void {
    if (storage?.getItem(RELOADED_KEY) != null) return;
    storage?.setItem(RELOADED_KEY, '1');
    win.location.reload();
  }

  let subscribed = false;
  const unsubscribe = subscribeConnectionStatus((status) => {
    // Subscribing replays the current status, and a fresh page starts disconnected.
    if (!subscribed || status !== 'disconnected') return;
    lost = true;
    check();
  });
  subscribed = true;
  win.document.addEventListener('visibilitychange', check);
  check();
  return () => {
    settle();
    unsubscribe();
    win.document.removeEventListener('visibilitychange', check);
  };
}

function sessionStorageOf(win: ProbedWindow): ProbedWindow['sessionStorage'] | undefined {
  try {
    return win.sessionStorage;
  } catch {
    // A page denied storage cannot bound its reloads across loads.
    return undefined;
  }
}
