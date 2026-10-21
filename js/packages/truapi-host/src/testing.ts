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
  defaultChatRoomFeatures,
  MOCK_CHAT_FEATURE_NAMES,
  MOCK_CHAT_INLINE_MAX_BYTES,
  MOCK_CHAT_PAGE_SIZE,
  MOCK_CHAT_PARTICIPANT_PAGE_SIZE,
  MOCK_CHAT_PEER,
  MOCK_CHAT_SEARCH_PAGE_SIZE,
  MOCK_CHAT_TIMINGS,
} from "./web/mock-chat.js";
export type {
  MockActionOutcome,
  MockAttachmentOutcome,
  MockChatAttachment,
  MockChatFeedUpdate,
  MockChatListRow,
  MockChatNotice,
  MockChatQueryRecord,
  MockChatRoom,
  MockChatRoomUserState,
  MockChatTimings,
  MockChatUser,
  MockMessageMenu,
  MockSendOutcome,
} from "./web/mock-chat.js";
