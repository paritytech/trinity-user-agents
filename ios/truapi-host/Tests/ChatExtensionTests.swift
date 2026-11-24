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
    /// is left with the behaviour that the product did not ask for. A change
    /// it does not serve answers `unsupported`, so the product can tell it
    /// from a failure worth a retry.
    @Test
    func defaultsRefuseWhatTheHostDoesNotServe() async throws {
        let bridge = FirstVersionChatHostBridge()
        var config = ChatRoomConfig.hostDefault
        config.productSource = true

        await #expect(throws: HostChatCreateRoomError.self) {
            _ = try await bridge.createRoom(roomId: "room", name: "Room", icon: "", config: config)
        }
        #expect(bridge.createdRooms.isEmpty)
        let preview = ChatRoomPreview(
            timestamp: nil,
            badge: nil,
            lastMessage: nil,
            lastReaction: nil,
            unseenReactions: 0,
            pinned: false,
            activity: nil
        )
        #expect(try await bridge.setRoomPreview(roomId: "room", preview: preview) == .unsupported)
        #expect(try await bridge.deleteRoom(roomId: "room") == .unsupported)
        let fed = try await bridge.updateFeed(
            roomId: "room",
            update: .statusChanged(messageId: "message", status: .read),
            persist: true
        )
        #expect(fed == .unsupported)
    }

    /// The core reaches a bridge as `any ChatHostBridge`. A host that keeps
    /// configurations overrides the four-argument `createRoom` and
    /// `roomConfig`, so both must be protocol requirements: a method that only
    /// the extension declares would run its default instead.
    @Test
    func aHostThatKeepsConfigurationsServesThemThroughTheProtocol() async throws {
        let declaring = DeclaringChatHostBridge()
        let bridge: any ChatHostBridge = declaring
        var config = ChatRoomConfig.hostDefault
        config.productSource = true

        let status = try await bridge.createRoom(roomId: "room", name: "Room", icon: "", config: config)

        #expect(status == .new)
        #expect(declaring.configs == ["room": config])
        #expect(try await bridge.roomConfig(roomId: "room") == config)
    }
}

/// A host that keeps the configuration of each room.
private final class DeclaringChatHostBridge: ChatHostBridge, @unchecked Sendable {
    private(set) var configs: [String: ChatRoomConfig] = [:]

    func createRoom(
        roomId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatRoomRegistrationStatus {
        throw HostRejection.Rejected(reason: "the core creates rooms with their configuration")
    }

    func createRoom(
        roomId: String,
        name _: String,
        icon _: String,
        config: ChatRoomConfig
    ) async throws -> ChatRoomRegistrationStatus {
        configs[roomId] = config
        return .new
    }

    func roomConfig(roomId: String) async throws -> ChatRoomConfig {
        configs[roomId] ?? .hostDefault
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
