import { connect } from "node:tls";

import type { CheckResult } from "./checks";

/** Fewer days than this left on a certificate counts as failing. */
export const EXPIRY_WARNING_DAYS = 14;

const DAY_MS = 86_400_000;

/** When the certificate the host serves on port 443 expires. */
export function peerExpiry(host: string): Promise<Date> {
  return new Promise((resolve, reject) => {
    const socket = connect({ host, port: 443, servername: host }, () => {
      const validTo = socket.getPeerCertificate().valid_to;
      socket.end();
      if (validTo) {
        resolve(new Date(validTo));
      } else {
        reject(new Error("no certificate"));
      }
    });
    socket.setTimeout(20_000, () => {
      socket.destroy();
      reject(new Error("timed out"));
    });
    socket.on("error", reject);
  });
}

/** The distinct hosts of the https links, in their order in the list. */
export function httpsHosts(urls: string[]): string[] {
  return [
    ...new Set(
      urls
        .map((url) => new URL(url))
        .filter((url) => url.protocol === "https:")
        .map((url) => url.hostname),
    ),
  ];
}

export async function checkCertificate(
  host: string,
  now = new Date(),
  expiry: (host: string) => Promise<Date> = peerExpiry,
): Promise<CheckResult> {
  // Keyed apart from the link checks so their failures are counted apart.
  const base = { name: `${host} certificate`, url: `tls://${host}` };
  try {
    const expires = await expiry(host);
    const days = Math.floor((expires.getTime() - now.getTime()) / DAY_MS);
    const date = expires.toISOString().slice(0, 10);
    if (days < 0) {
      return { ...base, ok: false, detail: `expired on ${date}` };
    }
    return {
      ...base,
      ok: days >= EXPIRY_WARNING_DAYS,
      detail: `expires in ${days} days (${date})`,
    };
  } catch (error) {
    return {
      ...base,
      ok: false,
      detail: error instanceof Error ? error.message : String(error),
    };
  }
}
