/**
 * Product-tagged console logger for sandbox internals.
 *
 * Local rather than injected: every message here is a host-side diagnostic about
 * one VM, and tagging it with the product is the only thing a caller would ever
 * configure. `debug` is the verbose level and is silenced in production builds
 * (`silenceDebugConsole`).
 */
export type Logger = {
  debug(...args: unknown[]): void;
  log(...args: unknown[]): void;
  info(...args: unknown[]): void;
  warn(...args: unknown[]): void;
  error(...args: unknown[]): void;
};

export function createLogger(scope: string): Logger {
  const tag = `[sandbox:${scope}]`;

  return {
    debug: (...args) => console.debug(tag, ...args),
    log: (...args) => console.info(tag, ...args),
    info: (...args) => console.info(tag, ...args),
    warn: (...args) => console.warn(tag, ...args),
    error: (...args) => console.error(tag, ...args),
  };
}
