// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// @dotli/host-cli, the terminal host for the TrUAPI Rust/WASM core.
//
// A peer of dotli's web host, not a layer on top of it: both depend directly
// on `@parity/truapi-host` (the host engine) and implement its 18 platform
// callbacks. The web host answers with modals and localStorage. This one
// answers with a pairing QR, readline prompts, owner-only files, and pooled
// sockets.

export {
  createCliHost,
  type CliHost,
  type CliHostConfig,
  type CliHostProduct,
} from "./host.js";
export {
  createTerminalPresenter,
  type HostPresenter,
  type TerminalPresenterOptions,
} from "./presenter.js";
export { describeReview, type ConfirmRequest } from "./reviews.js";
export {
  createChainPool,
  type ChainEndpoint,
  type ChainEndpoints,
  type ChainPool,
  type ChainPoolOptions,
  type SocketLike,
} from "./chain-pool.js";
export {
  FileKeyValueStore,
  InMemoryKeyValueStore,
  type KeyValueStore,
} from "./kv.js";
export {
  createLoopbackProvider,
  type CoreWireCallbacks,
  type FrameReceiver,
} from "./loopback.js";
export {
  createHostCallbacks,
  coreSlot,
  type HostCallbackDeps,
} from "./callbacks.js";
export {
  serializeOperationStarts,
  type JsonRpcProvider,
} from "./operation-order.js";
export { explainProductError, isProbableSsoTimeout } from "./errors.js";
export { openControllingTerminal, type TtyStreams } from "./tty.js";
export { renderQrTerminal } from "./qr.js";
export { loadWasmCore, type WasmCore } from "./wasm.js";
