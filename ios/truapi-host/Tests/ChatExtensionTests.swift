import Foundation
import Testing
@testable import TrUAPIHost

struct ChatHostBridgeDefaultsTests {
    /// A product may declare the configuration and metadata that a host
    /// without the v2 requests serves anyway, so that host must accept them.
    @Test
    func defaultsAcceptTheDeclarationsOfANewRoom() async throws {
        let bridge: ChatHostBridge = FirstVersionChatHostBridge()

        try await bridge.setRoomConfig(roomId: "room", config: .hostDefault)
        try await bridge.setRoomMetadata(
            roomId: "room",
            metadata: ChatRoomMetadata(name: "Room", description: nil)
        )

        #expect(try await bridge.roomConfig(roomId: "room") == .hostDefault)
    }

    /// A declaration that the host cannot serve must fail, not disappear.
    @Test
    func defaultsRefuseWhatTheHostDoesNotServe() async throws {
        let bridge: ChatHostBridge = FirstVersionChatHostBridge()
        var config = ChatRoomConfig.hostDefault
        config.productSource = true

        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomConfig(roomId: "room", config: config)
        }
        await #expect(throws: HostRejection.self) {
            try await bridge.deleteRoom(roomId: "room")
        }
        await #expect(throws: NativeChatFeedRejection.self) {
            try await bridge.updateFeed(
                roomId: "room",
                update: .statusChanged(messageId: "message", status: .read),
                persist: true
            )
        }
    }
}

/// A host that implements only the requests of the first chat version.
private final class FirstVersionChatHostBridge: ChatHostBridge {
    func createRoom(
        roomId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatRoomRegistrationStatus { .new }

    func registerBot(
        botId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatBotRegistrationStatus { .new }

    func postMessage(roomId _: String, content _: ChatMessageContent, alt _: String?) async throws -> String {
        "message-id"
    }

    func setRoomFooter(roomId _: String, footer _: ChatRoomFooter) async throws {}

    func listRooms() async throws -> [ChatRoom] { [] }
}
