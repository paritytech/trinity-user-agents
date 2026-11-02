/**
 * Which pages may offer the dev server's demos. Shared by the dev server's
 * plugins and the page, and free of other imports so the Vite config can
 * load it.
 */

/** Host names the demos answer on: this machine only. */
export const LOOPBACK_HOSTNAMES = ["localhost", "127.0.0.1", "[::1]", "::1"];

/** Whether this page may offer the demos: a dev server on this machine. */
export function demoAllowed(dev: boolean, hostname: string): boolean {
  return dev && LOOPBACK_HOSTNAMES.includes(hostname);
}
