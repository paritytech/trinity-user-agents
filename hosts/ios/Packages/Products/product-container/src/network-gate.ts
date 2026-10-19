/**
 * Page-side gate in front of `allowNetworkAccess`.
 *
 * Native keeps the grant, but asking it on every request puts each fetch
 * behind a bridge round trip, and a bridge that stops answering holds every
 * request forever. A host that was allowed is remembered for `ttlMs`, requests
 * to the same host in flight share one answer, and the native call itself is
 * bounded by `timeoutMs`.
 */

export type NetworkAccessCall = (url: string, timeoutMs: number) => Promise<{ allowed: boolean }>;

export interface NetworkGateOptions {
  ttlMs: number;
  timeoutMs: number;
  now?: () => number;
}

export interface NetworkGate {
  allow(url: string): Promise<boolean>;
}

export function createNetworkGate(call: NetworkAccessCall, options: NetworkGateOptions): NetworkGate {
  const now = options.now ?? (() => Date.now());
  const allowedUntil = new Map<string, number>();
  const inFlight = new Map<string, Promise<boolean>>();

  const hostOf = (url: string): string | null => {
    try {
      return new URL(url, location.href).host || null;
    } catch {
      return null;
    }
  };

  const ask = (url: string): Promise<boolean> => call(url, options.timeoutMs).then((response) => response.allowed);

  return {
    allow(url: string): Promise<boolean> {
      const host = hostOf(url);
      if (host === null) return ask(url);

      const until = allowedUntil.get(host);
      if (until !== undefined && until > now()) return Promise.resolve(true);

      const pending = inFlight.get(host);
      if (pending) return pending;

      const request = ask(url)
        .then((allowed) => {
          if (allowed) allowedUntil.set(host, now() + options.ttlMs);
          return allowed;
        })
        .finally(() => inFlight.delete(host));

      inFlight.set(host, request);
      return request;
    },
  };
}
