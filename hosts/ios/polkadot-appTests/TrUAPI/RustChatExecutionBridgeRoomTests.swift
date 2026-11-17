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

    /// The native chat draws no description, so the host keeps the name, the icon and the placeholder and drops
    /// the description. No icon keeps the current icon, and an empty icon removes it, as in `createRoom`.
    @Test func metadataKeepsTheNameTheIconAndThePlaceholder() async throws {
        let bridge = await makeBridge()

        try await bridge.setRoomMetadata(
            roomId: "r",
            metadata: ChatRoomMetadata(name: "Lobby", description: "Weekly game", icon: nil, placeholder: nil)
        )
        try await bridge.setRoomMetadata(
            roomId: "r",
            metadata: ChatRoomMetadata(
                name: "Plain",
                description: nil,
                icon: "data:image/png;base64,iVBO",
                placeholder: "No games yet"
            )
        )
        try await bridge.setRoomMetadata(
            roomId: "r",
            metadata: ChatRoomMetadata(name: "Bare", description: nil, icon: "", placeholder: nil)
        )

        #expect(api.roomMetadata == [
            .init(roomId: "r", name: "Lobby", icon: .keep, placeholder: nil),
            .init(
                roomId: "r",
                name: "Plain",
                icon: .replace("data:image/png;base64,iVBO"),
                placeholder: "No games yet"
            ),
            .init(roomId: "r", name: "Bare", icon: .replace(nil), placeholder: nil)
        ])
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
                metadata: ChatRoomMetadata(name: "x", description: nil, icon: nil, placeholder: nil)
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
                unseenReactions: 0,
                pinned: false,
                activity: nil
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
                unseenReactions: 0,
                pinned: false,
                activity: nil
            ),
            stored: .init(
                timestamp: 1_700_000_000_000,
                badge: .hostDefault,
                lastMessage: nil,
                unseenReaction: false,
                pinned: false,
                activity: nil
            )
        ),
        PreviewCase(
            testDescription: "count badge",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: .unreadCount(count: 4),
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0,
                pinned: false,
                activity: nil
            ),
            stored: .init(
                timestamp: nil,
                badge: .unreadCount(4),
                lastMessage: nil,
                unseenReaction: false,
                pinned: false,
                activity: nil
            )
        ),
        PreviewCase(
            testDescription: "zero count badge",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: .unreadCount(count: 0),
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0,
                pinned: false,
                activity: nil
            ),
            stored: .init(
                timestamp: nil,
                badge: .unreadCount(0),
                lastMessage: nil,
                unseenReaction: false,
                pinned: false,
                activity: nil
            )
        ),
        PreviewCase(
            testDescription: "unseen reaction",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: nil,
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 1,
                pinned: false,
                activity: nil
            ),
            stored: .init(
                timestamp: nil,
                badge: .hostDefault,
                lastMessage: nil,
                unseenReaction: true,
                pinned: false,
                activity: nil
            )
        ),
        PreviewCase(
            testDescription: "image badge",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: .image(icon: "https://example.com/badge.png"),
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0,
                pinned: false,
                activity: nil
            ),
            stored: .init(
                timestamp: nil,
                badge: .image("https://example.com/badge.png"),
                lastMessage: nil,
                unseenReaction: false,
                pinned: false,
                activity: nil
            )
        ),
        PreviewCase(
            testDescription: "pinned with an ongoing activity",
            wire: ChatRoomPreview(
                timestamp: nil,
                badge: nil,
                lastMessage: nil,
                lastReaction: nil,
                unseenReactions: 0,
                pinned: true,
                activity: ChatRoomActivity(text: "Ben is typing", ongoing: true)
            ),
            stored: .init(
                timestamp: nil,
                badge: .hostDefault,
                lastMessage: nil,
                unseenReaction: false,
                pinned: true,
                activity: Chat.RoomActivity(text: "Ben is typing", ongoing: true)
            )
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
        Chat.RoomDeclarations(config: config, configGeneration: 1, preview: .hostDefault, placeholder: nil)
    }
}
