import { type QuickJSContext } from 'quickjs-emscripten';

// Spec: https://w3c.github.io/webcrypto/#Crypto-method-getRandomValues
//
// Generates random bytes host-side, copies them into the caller's view, and
// returns the original typed array (per spec — the same instance the caller
// passed in). The in-VM helper handles the byteOffset/byteLength and view-type
// preservation so the host code stays trivial.
const COPY_HELPER_SRC =
  '(target, srcBuf) => new Uint8Array(target.buffer, target.byteOffset, target.byteLength).set(new Uint8Array(srcBuf))';

export function injectCrypto(vm: QuickJSContext): VoidFunction {
  const helperResult = vm.evalCode(COPY_HELPER_SRC);
  if (helperResult.error) {
    const msg = vm.dump(helperResult.error);
    helperResult.error.dispose();
    throw new Error(`Failed to inject crypto helper: ${JSON.stringify(msg)}`);
  }
  const copyHelper = helperResult.value;

  const cryptoInstance = vm.newObject();

  const getRandomValues = vm.newFunction('getRandomValues', arg => {
    const lengthHandle = vm.getProp(arg, 'byteLength');
    const byteLength = vm.getNumber(lengthHandle);
    lengthHandle.dispose();

    const bytes = new Uint8Array(byteLength);
    crypto.getRandomValues(bytes);

    const srcHandle = vm.newArrayBuffer(bytes.buffer);
    const callRes = vm.callFunction(copyHelper, vm.undefined, arg, srcHandle);
    srcHandle.dispose();
    if (callRes.error) callRes.error.dispose();
    else callRes.value.dispose();

    // Spec: return the same typed array passed in.
    return arg.dup();
  });

  vm.setProp(cryptoInstance, 'getRandomValues', getRandomValues);
  vm.setProp(vm.global, 'crypto', cryptoInstance);
  cryptoInstance.dispose();
  getRandomValues.dispose();

  // copyHelper must outlive injection — it's invoked from getRandomValues.
  return () => copyHelper.dispose();
}
