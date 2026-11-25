import { type QuickJSContext } from 'quickjs-emscripten';

import { type Logger } from '../logger';

// Per-argument cap on stringified payload size. Sandbox code that calls
// `console.log("X".repeat(1e9))` would otherwise dump that string straight to
// the host logger. 64 KiB is far above any human-readable use.
const MAX_STRING_ARG_BYTES = 64 * 1024;

// Strip C0 control characters except \t (HT) and \n (LF). Anything else —
// notably \x1b (ESC, used in ANSI escape sequences) and \r — gets replaced
// with the Unicode replacement char so terminal-aware loggers cannot be
// driven by sandbox output (clear screen, cursor moves, color injection).
// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\x00-\x08\x0b-\x1f\x7f]/g;

function sanitizeString(s: string): string {
  let out = s.replace(CONTROL_CHARS, '�');
  if (out.length > MAX_STRING_ARG_BYTES) out = out.slice(0, MAX_STRING_ARG_BYTES) + '…[truncated]';
  return out;
}

function sanitizeArg(v: unknown): unknown {
  return typeof v === 'string' ? sanitizeString(v) : v;
}

// Apply WHATWG console format specifiers to the first arg if it's a string
// containing `%` substitutions. Returns the new arg list with the substituted
// string prefixed and any unconsumed args appended.
//
// Spec: https://console.spec.whatwg.org/#formatter
function formatArgs(args: unknown[]): unknown[] {
  if (args.length === 0 || typeof args[0] !== 'string') return args;
  const fmt = args[0];
  if (!fmt.includes('%')) return args;

  let argIdx = 1;
  const formatted = fmt.replace(/%[sdifoOc%]/g, match => {
    if (match === '%%') return '%';
    if (argIdx >= args.length) return match;
    const arg = args[argIdx++];
    switch (match) {
      case '%s':
        return typeof arg === 'string' ? arg : String(arg);
      case '%d':
      case '%i': {
        const n = Number(arg);
        return Number.isFinite(n) ? String(Math.trunc(n)) : 'NaN';
      }
      case '%f':
        return String(Number(arg));
      case '%o':
      case '%O':
        try {
          return JSON.stringify(arg);
        } catch {
          return String(arg);
        }
      case '%c':
        // CSS styling — not applicable to a logger sink, drop the directive.
        return '';
      default:
        return match;
    }
  });
  return [formatted, ...args.slice(argIdx)];
}

export function injectConsole(vm: QuickJSContext, logger: Logger) {
  const consoleObj = vm.newObject();
  for (const method of ['log', 'info', 'warn', 'error'] as const) {
    const fn = vm.newFunction(method, (...args) => {
      const dumped = args.map(h => vm.dump(h));
      const formatted = formatArgs(dumped).map(sanitizeArg);
      logger[method](...formatted);
      return vm.undefined;
    });
    vm.setProp(consoleObj, method, fn);
    fn.dispose();
  }
  vm.setProp(vm.global, 'console', consoleObj);
  consoleObj.dispose();
}
