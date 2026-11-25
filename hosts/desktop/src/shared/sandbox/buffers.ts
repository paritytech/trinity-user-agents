import { type QuickJSContext, type QuickJSHandle } from 'quickjs-emscripten';

// Run `fn` with a fresh handle for `target[key]`, disposing the handle whether
// `fn` returns or throws. Encapsulates the "getProp → use → dispose" pattern.
export function withProp<T>(
  vm: QuickJSContext,
  target: QuickJSHandle,
  key: string | number,
  fn: (handle: QuickJSHandle) => T,
): T {
  const handle = vm.getProp(target, key);
  try {
    return fn(handle);
  } finally {
    handle.dispose();
  }
}

// Set a property to a freshly-built handle, disposing the handle whether
// `setProp` returns or throws. Caller's most common dispose lifecycle.
export function setPropAndDispose(vm: QuickJSContext, target: QuickJSHandle, key: string | number, value: QuickJSHandle): void {
  try {
    vm.setProp(target, key, value);
  } finally {
    value.dispose();
  }
}

// Read a TypedArray view out of the VM as a fresh host-side Uint8Array.
//
// Safety:
//   - All transient handles are disposed via try/finally even if any step
//     throws, so partial extraction can't leak QuickJS handles.
//   - The returned bytes are a snapshot (`.slice()` of the WASM-mirror Uint8Array)
//     before the lifetime is freed, so they remain valid after the VM frees
//     or mutates the source.
export function extractBytesFromVm(vm: QuickJSContext, viewHandle: QuickJSHandle): Uint8Array {
  const offset = withProp(vm, viewHandle, 'byteOffset', h => vm.getNumber(h));
  const length = withProp(vm, viewHandle, 'byteLength', h => vm.getNumber(h));
  return withProp(vm, viewHandle, 'buffer', bufH => {
    const lifetime = vm.getArrayBuffer(bufH);
    try {
      return lifetime.value.slice(offset, offset + length);
    } finally {
      lifetime.dispose();
    }
  });
}

// Build a Uint8Array inside the VM containing a copy of the given host bytes.
// Returns a handle the caller is responsible for disposing.
//
// Safety:
//   - `vm.newArrayBuffer` copies the input into VM-managed memory; the host's
//     `bytes.buffer` is never aliased into the VM and the VM cannot mutate
//     host memory through this path.
//   - The fast path passing `bytes.buffer` directly is taken only when the
//     view exactly covers the buffer, so adjacent bytes of a larger buffer
//     are never exposed to the VM.
//   - `bufferHandle` is disposed via try/finally to prevent a leak if
//     `vm.callFunction` throws.
export function sendBytesToVm(vm: QuickJSContext, toUint8ArrayFn: QuickJSHandle, bytes: Uint8Array): QuickJSHandle {
  const buffer =
    bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength
      ? bytes.buffer
      : bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);

  const bufferHandle = vm.newArrayBuffer(buffer);
  let result;
  try {
    result = vm.callFunction(toUint8ArrayFn, vm.undefined, bufferHandle);
  } finally {
    bufferHandle.dispose();
  }
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`sendBytesToVm: failed to wrap ArrayBuffer: ${JSON.stringify(msg)}`);
  }
  return result.value;
}
