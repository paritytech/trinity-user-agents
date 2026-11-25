// Test-only entry point: the in-memory mock host and its runtime config.
//
// Separate from `./web` so a product bundling the real host runtime does not
// carry the mock with it. The implementation lives under `web/` because it
// mocks the web host's callback seam; only the entry point is split.

export {
  createMockHost,
  mockRuntimeConfig,
  mockTestStack,
  MOCK_GENESIS,
} from "./web/create-mock-host.js";
export type {
  ChainStatus,
  ChatMessageRecord,
  MockFaults,
  MockHost,
  MockHostConfig,
  MockRuntimeConfig,
  PermissionLogEntry,
  PermissionKind,
  PermissionPolicy,
  SigningLogEntry,
  TestStack,
  TestStackChain,
} from "./web/create-mock-host.js";
