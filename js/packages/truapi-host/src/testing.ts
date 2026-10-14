// Test-only entry point: the in-memory mock host and its runtime config.
//
// Separate from `./web` so a product bundling the real host runtime does not
// carry the mock with it. The implementation lives under `web/` because it
// mocks the web host's callback seam; only the entry point is split.

export {
  createMockHost,
  mockRuntimeConfig,
  MOCK_GENESIS,
} from "./web/create-mock-host.js";
export type {
  ChainStatus,
  ChatMessageRecord,
  MockFaults,
  MockHost,
  MockHostConfig,
  PermissionLogEntry,
  PermissionKind,
  PermissionPolicy,
  SigningLogEntry,
} from "./web/create-mock-host.js";
export {
  defaultChatRoomConfig,
  MOCK_CHAT_PAGE_SIZE,
  MOCK_CHAT_PARTICIPANT_PAGE_SIZE,
  MOCK_CHAT_PEER,
  MOCK_CHAT_TIMINGS,
} from "./web/mock-chat.js";
export type {
  MockActionOutcome,
  MockChatFeedUpdate,
  MockChatListRow,
  MockChatNotice,
  MockChatQueryRecord,
  MockChatRoom,
  MockChatTimings,
  MockChatUser,
  MockMessageMenu,
  MockParticipantBuiltIn,
  MockParticipantMenu,
  MockSendOutcome,
} from "./web/mock-chat.js";
