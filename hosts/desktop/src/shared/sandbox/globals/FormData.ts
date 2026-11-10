import { type QuickJSContext } from 'quickjs-emscripten';

// Spec: https://xhr.spec.whatwg.org/#interface-formdata
//
// Depends on `Blob` already being injected (Blob values are stored as-is and
// pass through fetch's multipart encoder via `Blob.__getBytes`).
const SOURCE = `(() => {
  if (typeof globalThis.FormData === 'function') return;

  const _fdEntries = new WeakMap();

  function normalizeFormValue(value) {
    if (typeof Blob === 'function' && value instanceof Blob) return value;
    return String(value);
  }

  class FormData {
    constructor() { _fdEntries.set(this, []); }
    append(name, value) { _fdEntries.get(this).push([String(name), normalizeFormValue(value)]); }
    set(name, value) {
      const n = String(name);
      const v = normalizeFormValue(value);
      const list = _fdEntries.get(this);
      let placed = false;
      const next = [];
      for (const [k, val] of list) {
        if (k === n) {
          if (!placed) { next.push([n, v]); placed = true; }
        } else next.push([k, val]);
      }
      if (!placed) next.push([n, v]);
      _fdEntries.set(this, next);
    }
    delete(name) {
      const n = String(name);
      _fdEntries.set(this, _fdEntries.get(this).filter(([k]) => k !== n));
    }
    get(name) {
      const n = String(name);
      for (const [k, v] of _fdEntries.get(this)) if (k === n) return v;
      return null;
    }
    getAll(name) {
      const n = String(name);
      return _fdEntries.get(this).filter(([k]) => k === n).map(([, v]) => v);
    }
    has(name) {
      const n = String(name);
      return _fdEntries.get(this).some(([k]) => k === n);
    }
    *entries() { for (const e of _fdEntries.get(this)) yield [e[0], e[1]]; }
    *keys() { for (const e of _fdEntries.get(this)) yield e[0]; }
    *values() { for (const e of _fdEntries.get(this)) yield e[1]; }
    forEach(cb, thisArg) { for (const [k, v] of this.entries()) cb.call(thisArg, v, k, this); }
  }
  FormData.prototype[Symbol.iterator] = FormData.prototype.entries;
  Object.defineProperty(FormData.prototype, Symbol.toStringTag, { value: 'FormData', configurable: true });

  globalThis.FormData = FormData;
})();`;

export function injectFormData(vm: QuickJSContext) {
  const result = vm.evalCode(SOURCE);
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`Failed to inject FormData: ${JSON.stringify(msg)}`);
  }
  result.value.dispose();
}
