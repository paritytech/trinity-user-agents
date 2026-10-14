/**
 * Generic JS-side transport for native <-> JS communication.
 *
 * Platform-agnostic: the caller provides a `sendToNative` function
 * so this can be reused by both Android and iOS.
 *
 * Native side replies via `window.__container_callback__(id, payload)`.
 */

interface PendingEntry {
  resolve?: (value: any) => void;
  reject?: (error: any) => void;
  onUpdate?: (data: any) => void;
  onError?: (error: Error) => void;
}

export interface NativeTransport {
  callNative(method: string, params: unknown): Promise<any>;
  subscribeNative(
    method: string,
    params: unknown,
    onUpdate: (data: any) => void,
    onError?: (error: Error) => void,
  ): () => void;
}

export function createNativeTransport(
  sendToNative: (message: object) => void,
): NativeTransport {
  const pending = new Map<string, PendingEntry>();
  let nextId = 0;

  // Native side calls this via evaluateJavascript / WKScriptMessageHandler
  (window as any)['__container_callback__'] = (id: string, payload: any) => {
    const entry = pending.get(id);
    if (!entry) return;

    const msg = typeof payload === 'string' ? JSON.parse(payload) : payload;

    if ('value' in msg) {
      entry.resolve?.(msg.value);
      pending.delete(id);
    } else if ('error' in msg) {
      // The native bridge sends { error: { code, message } }; older calls may still send a string.
      const rawError = msg.error;
      const e =
        rawError !== null && typeof rawError === 'object'
          ? Object.assign(new Error(rawError.message ?? 'Unknown error'), { code: rawError.code })
          : new Error(rawError);
      entry.reject?.(e);
      entry.onError?.(e);
      pending.delete(id);
    } else if ('update' in msg) {
      entry.onUpdate?.(msg.update);
    } else if ('complete' in msg) {
      pending.delete(id);
    }
  };

  return {
    callNative(method: string, params: unknown): Promise<any> {
      return new Promise((resolve, reject) => {
        const id = `r${nextId++}`;
        pending.set(id, { resolve, reject });
        sendToNative({ type: 'request', id, method, params });
      });
    },

    subscribeNative(
      method: string,
      params: unknown,
      onUpdate: (data: any) => void,
      onError?: (error: Error) => void,
    ): () => void {
      const id = `r${nextId++}`;
      pending.set(id, { onUpdate, onError });
      sendToNative({ type: 'subscribe', id, method, params });

      return () => {
        pending.delete(id);
        sendToNative({ type: 'unsubscribe', id });
      };
    },
  };
}
