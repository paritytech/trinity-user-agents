import type { RemotePermission } from '@parity/truapi';
import type { InternalTrUApiClient } from '@parity/truapi/internal';

export type NetworkAuthorization = (
  url: string,
  decide: (allowed: boolean) => void,
) => () => void;

export function createPermissionAuthorization(
  win: Window & typeof globalThis,
  client?: InternalTrUApiClient,
): {
  network: NetworkAuthorization;
} {
  const NativeURL = win.URL;
  const NativeAbortController = win.AbortController;
  const apply = Reflect.apply;
  const descriptor = Object.getOwnPropertyDescriptor;
  const hostname = descriptor(NativeURL.prototype, 'hostname')!.get!;
  const protocol = descriptor(NativeURL.prototype, 'protocol')!.get!;
  const indexOf = String.prototype.indexOf;

  function authorize(
    operation: (signal: AbortSignal) => Promise<boolean>,
    decide: (allowed: boolean) => void,
  ): () => void {
    const controller = new NativeAbortController();
    void (async () => {
      let allowed = false;
      try {
        if (client) allowed = await operation(controller.signal);
      } catch {
        allowed = false;
      }
      if (!controller.signal.aborted) {
        try { decide(allowed); } catch { /* Product callbacks are independent. */ }
      }
    })();
    return () => controller.abort();
  }

  function remote(permission: RemotePermission, decide: (allowed: boolean) => void): () => void {
    return authorize(async signal => {
      const result = await client!.permissions.authorizeRemotePermission({ permission }, { signal });
      return result.isOk() && result.value.granted === true;
    }, decide);
  }

  return {
    network(url, decide) {
      try {
        const destination = new NativeURL(url);
        const scheme = apply(protocol, destination, []);
        const domain = apply(hostname, destination, []);
        if (
          (scheme === 'http:' || scheme === 'https:' || scheme === 'ws:' || scheme === 'wss:') &&
          domain && apply(indexOf, domain, ['*']) === -1
        ) return remote({ tag: 'Remote', value: { domains: [domain] } }, decide);
      } catch { /* Invalid destinations fail closed. */ }
      try { decide(false); } catch { /* Product callbacks are independent. */ }
      return () => {};
    },
  };
}
