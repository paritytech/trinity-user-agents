import Foundation
import Testing
@testable import TrUAPIHost

struct ChatHostBridgeDefaultsTests {
    /// A product may declare the configuration and metadata that a host
    /// without the v2 requests serves anyway, so that host must accept them.
    @Test
    func defaultsAcceptTheDeclarationsOfANewRoom() async throws {
        let bridge = FirstVersionChatHostBridge()

        let status = try await bridge.createRoom(
            roomId: "room",
            name: "Room",
            icon: "",
            config: .hostDefault
        )
        try await bridge.setRoomMetadata(
            roomId: "room",
            metadata: ChatRoomMetadata(name: "Room", description: nil, icon: nil, placeholder: nil)
        )

        #expect(status == .new)
        #expect(bridge.createdRooms == ["room"])
        #expect(try await bridge.roomConfig(roomId: "room") == .hostDefault)
    }

    /// A declaration that the host cannot serve must fail, not disappear. A
    /// configuration it cannot keep fails before the room exists, so no room
    /// is left with the behaviour that the product did not ask for.
    @Test
    func defaultsRefuseWhatTheHostDoesNotServe() async throws {
        let bridge = FirstVersionChatHostBridge()
        var config = ChatRoomConfig.hostDefault
        config.productSource = true

        await #expect(throws: HostChatCreateRoomError.UnsupportedConfig) {
            _ = try await bridge.createRoom(roomId: "room", name: "Room", icon: "", config: config)
        }
        #expect(bridge.createdRooms.isEmpty)
        await #expect(throws: HostRejection.self) {
            try await bridge.deleteRoom(roomId: "room")
        }
        await #expect(throws: HostRejection.self) {
            try await bridge.updateFeed(
                roomId: "room",
                update: .statusChanged(messageId: "message", status: .read),
                persist: true
            )
        }
    }
}

/// A host that implements only the requests of the first chat version.
private final class FirstVersionChatHostBridge: ChatHostBridge, @unchecked Sendable {
    private(set) var createdRooms: [String] = []

    func createRoom(
        roomId: String,
        name _: String,
        icon _: String
    ) async throws -> ChatRoomRegistrationStatus {
        createdRooms.append(roomId)
        return .new
    }

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
