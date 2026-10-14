// Ambient declaration of the wasm-pack glue `make wasm` emits to
// `dist/wasm/web/`. Declaring it under `src/` lets the worker import the glue
// by a literal specifier that resolves against `dist/worker-runtime.js` once
// compiled, so bundlers follow it and emit the glue plus its `.wasm` payload
// themselves. Declarations are not emitted, so this file never shadows the
// real artifact, and the typecheck does not require it to have been built.

import type { WasmModuleShape } from "../../wasm-module.js";

declare const init: WasmModuleShape["default"];
export default init;

export const WasmPairingHostRuntime: WasmModuleShape["WasmPairingHostRuntime"];
export const WasmProductRuntime: WasmModuleShape["WasmProductRuntime"];
export const setLogLevel: (level: string) => void;
export const deriveProductAccountPublicKey: WasmModuleShape["deriveProductAccountPublicKey"];
export const productAccountAddress: WasmModuleShape["productAccountAddress"];
export const wireSchemaHash: () => string;
