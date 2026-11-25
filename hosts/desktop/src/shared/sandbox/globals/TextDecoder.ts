import { type QuickJSContext } from 'quickjs-emscripten';

import { extractBytesFromVm, withProp } from '../buffers';

export function injectTextDecoder(vm: QuickJSContext) {
  const textDecoderCtor = vm.newConstructorFunction('TextDecoder', (...args) => {
    const proto = vm.newObject();

    const decodeFn = vm.newFunction('decode', function (...args) {
      const dataHandle = args[0];
      if (dataHandle === undefined || vm.typeof(dataHandle) === 'undefined') {
        return vm.newString('');
      }

      const encoding = withProp(vm, this, 'encoding', h => {
        const raw = vm.dump(h);
        return typeof raw === 'string' ? raw : 'utf-8';
      });

      // BufferSource: TypedArray (has .buffer) or raw ArrayBuffer.
      const bytes = withProp(vm, dataHandle, 'buffer', bufH => {
        if (vm.typeof(bufH) !== 'undefined') {
          return extractBytesFromVm(vm, dataHandle);
        }
        const lifetime = vm.getArrayBuffer(dataHandle);
        try {
          return lifetime.value.slice(0);
        } finally {
          lifetime.dispose();
        }
      });

      return vm.newString(new TextDecoder(encoding).decode(bytes));
    });

    vm.setProp(proto, 'decode', decodeFn);

    const instance = vm.newObject(proto);
    const raw = args[0] !== undefined ? vm.dump(args[0]) : undefined;
    const encoding = typeof raw === 'string' ? raw : 'utf-8';
    const encHandle = vm.newString(encoding);
    // Spec: TextDecoder.prototype.encoding is read-only.
    vm.defineProp(instance, 'encoding', {
      value: encHandle,
      configurable: false,
      enumerable: true,
    });
    encHandle.dispose();

    proto.dispose();
    decodeFn.dispose();
    return instance;
  });

  vm.setProp(vm.global, 'TextDecoder', textDecoderCtor);
  textDecoderCtor.dispose();
}
