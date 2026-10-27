import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The Chat Extension v2 room requests of the rust chat bridge reach the chat store as Swift domain values.
struct RustChatExecutionBridgeRoomTests {
    private let api = RecordingChatMessaging()

    @Test func theConfigurationOfARoomIsTheStoredOneOrTheDefault() async throws {
        let bridge = await makeBridge()
        #expect(try await bridge.roomConfig(roomId: "r") == .hostDefault)

        api.declarations = declarations(config: config)
        #expect(try await bridge.roomConfig(roomId: "r") == wireConfig)
    }

    @Test func aConfigurationReachesTheStoreWithEveryField() async throws {
        let bridge = await makeBridge()

        try await bridge.setRoomConfig(roomId: "r", config: wireConfig)

        #expect(api.roomConfigs.map(\.roomId) == ["r"])
        #expect(api.roomConfigs.map(\.config) == [config])
    }

    /// The native chat draws a name only, so the host keeps the name and drops the description.
    @Test func metadataKeepsTheName() async throws {
        let bridge = await makeBridge()

        try await bridge.setRoomMetadata(
            roomId: "r",
            metadata: ChatRoomMetadata(name: "Lobby", description: "Weekly game")
        )
        try await bridge.setRoomMetadata(roomId: "r", metadata: ChatRoomMetadata(name: "Plain", description: nil))

        #expect(api.roomMetadata == [.init(roomId: "r", name: "Lobby"), .init(roomId: "r", name: "Plain")])
    }

    @Test(arguments: PreviewCase.all)
    func eachPreviewVariantReachesTheStore(_ previewCase: PreviewCase) async throws {
        let bridge = await makeBridge()

        try await bridge.setRoomPreview(roomId: "r", preview: previewCase.wire)

        #expect(api.roomPreviews.map(\.preview) == [previewCase.stored])
    }

    @Test func everyOtherRequestRefusesARoomThatDoesNotExist() async {
        let bridge = await makeBridge()
        api.unknownRooms = ["gone"]

        await #expect(throws: ChatRoomError.self) { try await bridge.roomConfig(roomId: "gone") }
        await #expect(throws: ChatRoomError.self) {
            try await bridge.setRoomConfig(roomId: "gone", config: .hostDefault)
        }
        await #expect(throws: ChatRoomError.self) {
            try await bridge.setRoomMetadata(
                roomId: "gone",
                metadata: ChatRoomMetadata(name: "x", description: nil)
            )
        }
        await #expect(throws: ChatRoomError.self) {
            try await bridge.setRoomPreview(roomId: "gone", preview: PreviewCase.all[0].wire)
        }
    }

    @Test func aRequestWithoutARoomIsRefused() async {
        let bridge = await makeBridge()

        await #expect(throws: HostRejection.self) { try await bridge.setRoomConfig(roomId: "", config: .hostDefault) }
        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomPreview(roomId: "", preview: PreviewCase.all[0].wire)
        }
        #expect(api.roomConfigs.isEmpty)
    }
}

/// One `set_room_preview` request and the preview that the store keeps for it.
struct PreviewCase: CustomTestStringConvertible, Sendable {
    let testDescription: String
    let wire: ChatRoomPreview
    let stored: Chat.RoomPreview

    static let all: [PreviewCase] = [
        PreviewCase(
            testDescription: "host defaults",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: nil,
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0
            ),
            stored: .hostDefault
        ),
        PreviewCase(
            testDescription: "a timestamp",
            wire: ChatRoomPreview(
                timestamp: 1_700_000_000_000,
                badge: nil,
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0
            ),
            stored: .init(timestamp: 1_700_000_000_000, badge: .hostDefault, lastMessage: nil, unseenReaction: false)
        ),
        PreviewCase(
            testDescription: "count badge",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: .unreadCount(count: 4),
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0
            ),
            stored: .init(timestamp: nil, badge: .unreadCount(4), lastMessage: nil, unseenReaction: false)
        ),
        PreviewCase(
            testDescription: "hidden badge",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: .hidden,
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0
            ),
            stored: .init(timestamp: nil, badge: .none, lastMessage: nil, unseenReaction: false)
        ),
        PreviewCase(
            testDescription: "unseen reaction",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: nil,
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 1
            ),
            stored: .init(timestamp: nil, badge: .hostDefault, lastMessage: nil, unseenReaction: true)
        )
    ]
}

private extension RustChatExecutionBridgeRoomTests {
    var config: Chat.RoomConfig {
        Chat.RoomConfig(productSource: true, reactionDetails: true, reportSeen: true)
    }

    var wireConfig: ChatRoomConfig {
        ChatRoomConfig(productSource: true, reactionDetails: true, reportSeen: true)
    }

    func makeBridge() async -> RustChatExecutionBridge {
        await RustChatExecutionBridge(
            dependencies: MainActor.run { makeChatBridgeDependencies() },
            chatMessaging: api
        )
    }

    func declarations(config: Chat.RoomConfig?) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(config: config, configGeneration: 1, preview: .hostDefault)
    }
}
