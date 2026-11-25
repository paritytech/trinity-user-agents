import Foundation
import Testing
@testable import TrUAPIHost

struct ChatHostBridgeDefaultsTests {
    /// A product may declare the configuration that a host without the v2
    /// requests serves anyway, so that host must accept it, and the core then
    /// finds the room in the room list of that host.
    @Test
    func defaultsAcceptTheConfigurationOfANewRoom() async throws {
        let bridge = FirstVersionChatHostBridge()

        let status = try await bridge.createRoom(
            roomId: "room",
            name: "Room",
            icon: "",
            config: .hostDefault
        )

        #expect(status == .new)
        #expect(bridge.createdRooms == ["room"])
        let hosted = ChatRoomState(participatingAs: .roomHost, config: .hostDefault)
        #expect(try await bridge.room(roomId: "room") == hosted)
        #expect(try await bridge.room(roomId: "other") == nil)
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
        let metadata = ChatRoomMetadata(name: "Room", description: nil, icon: nil, placeholder: nil)
        #expect(try await bridge.setRoomMetadata(roomId: "room", metadata: metadata) == .unsupported)
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
    /// configurations overrides the four-argument `createRoom` and `room`, so
    /// both must be protocol requirements: a method that only the extension
    /// declares would run its default instead.
    @Test
    func aHostThatKeepsConfigurationsServesThemThroughTheProtocol() async throws {
        let declaring = DeclaringChatHostBridge()
        let bridge: any ChatHostBridge = declaring
        var config = ChatRoomConfig.hostDefault
        config.productSource = true

        let status = try await bridge.createRoom(roomId: "room", name: "Room", icon: "", config: config)

        #expect(status == .new)
        #expect(declaring.configs == ["room": config])
        let hosted = ChatRoomState(participatingAs: .roomHost, config: config)
        #expect(try await bridge.room(roomId: "room") == hosted)
    }

    /// A host that keeps configurations implements only the four-argument
    /// `createRoom`, and a host without rooms implements neither. Each default
    /// `createRoom` must not call the other for ever: the core reaches the
    /// four-argument one, and a host that implements neither refuses the room.
    @Test
    func aHostImplementsOnlyTheCreateRoomItNeeds() async throws {
        let declaring = DeclaringChatHostBridge()
        let bridge: any ChatHostBridge = declaring

        let status = try await bridge.createRoom(roomId: "room", name: "Room", icon: "", config: .hostDefault)

        #expect(status == .new)
        #expect(declaring.configs == ["room": .hostDefault])
        let roomless: any ChatHostBridge = RoomlessChatHostBridge()
        await #expect(throws: HostRejection.self) {
            _ = try await roomless.createRoom(roomId: "room", name: "Room", icon: "", config: .hostDefault)
        }
    }
}

/// A host that keeps the configuration of each room, and so implements only
/// the four-argument `createRoom`.
private final class DeclaringChatHostBridge: ChatHostBridge, @unchecked Sendable {
    private(set) var configs: [String: ChatRoomConfig] = [:]

    func createRoom(
        roomId: String,
        name _: String,
        icon _: String,
        config: ChatRoomConfig
    ) async throws -> ChatRoomRegistrationStatus {
        configs[roomId] = config
        return .new
    }

    func room(roomId: String) async throws -> ChatRoomState? {
        configs[roomId].map { ChatRoomState(participatingAs: .roomHost, config: $0) }
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

    func listRooms() async throws -> [ChatRoom] {
        createdRooms.map { ChatRoom(roomId: $0, participatingAs: .roomHost) }
    }
}

/// A host that creates no rooms: it implements neither `createRoom`.
private final class RoomlessChatHostBridge: ChatHostBridge, @unchecked Sendable {
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
