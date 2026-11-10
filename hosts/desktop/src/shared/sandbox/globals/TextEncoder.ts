import { type QuickJSContext, type QuickJSHandle } from 'quickjs-emscripten';

import { sendBytesToVm } from '../buffers';

export function injectTextEncoder(vm: QuickJSContext, toUint8ArrayFn: QuickJSHandle) {
  const textEncoderCtor = vm.newConstructorFunction('TextEncoder', () => {
    const proto = vm.newObject();
    const encodeFn = vm.newFunction('encode', strHandle => {
      const raw = vm.dump(strHandle);
      const str = typeof raw === 'string' ? raw : '';
      const hostBytes = new TextEncoder().encode(str);
      return sendBytesToVm(vm, toUint8ArrayFn, hostBytes);
    });

    vm.setProp(proto, 'encode', encodeFn);

    const instance = vm.newObject(proto);
    // Spec: TextEncoder.prototype.encoding is read-only and always 'utf-8'.
    const encHandle = vm.newString('utf-8');
    vm.defineProp(instance, 'encoding', {
      value: encHandle,
      configurable: false,
      enumerable: true,
    });
    encHandle.dispose();
    proto.dispose();
    encodeFn.dispose();
    return instance;
  });

  vm.setProp(vm.global, 'TextEncoder', textEncoderCtor);
  textEncoderCtor.dispose();
}
