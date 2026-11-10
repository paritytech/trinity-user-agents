import { type QuickJSContext } from 'quickjs-emscripten';

// Generic DOM-spec primitives that are not interface-specific.
// Currently: DOMException (https://webidl.spec.whatwg.org/#idl-DOMException).
// Inject this before any global that creates DOMExceptions (e.g. AbortController).
const SOURCE = `(() => {
  if (typeof globalThis.DOMException === 'function') return;

  class DOMException extends Error {
    constructor(message = '', name = 'Error') {
      super(String(message));
      this.name = String(name);
    }
    get code() {
      return DOMException.#codeFor(this.name);
    }
    static #codeFor(name) {
      switch (name) {
        case 'IndexSizeError': return 1;
        case 'HierarchyRequestError': return 3;
        case 'WrongDocumentError': return 4;
        case 'InvalidCharacterError': return 5;
        case 'NoModificationAllowedError': return 7;
        case 'NotFoundError': return 8;
        case 'NotSupportedError': return 9;
        case 'InUseAttributeError': return 10;
        case 'InvalidStateError': return 11;
        case 'SyntaxError': return 12;
        case 'InvalidModificationError': return 13;
        case 'NamespaceError': return 14;
        case 'InvalidAccessError': return 15;
        case 'TypeMismatchError': return 17;
        case 'SecurityError': return 18;
        case 'NetworkError': return 19;
        case 'AbortError': return 20;
        case 'URLMismatchError': return 21;
        case 'QuotaExceededError': return 22;
        case 'TimeoutError': return 23;
        case 'InvalidNodeTypeError': return 24;
        case 'DataCloneError': return 25;
        default: return 0;
      }
    }
  }

  Object.defineProperty(DOMException.prototype, Symbol.toStringTag, {
    value: 'DOMException', configurable: true,
  });

  globalThis.DOMException = DOMException;
})();`;

export function injectDOMGeneric(vm: QuickJSContext) {
  const result = vm.evalCode(SOURCE);
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`Failed to inject DOMException: ${JSON.stringify(msg)}`);
  }
  result.value.dispose();
}
