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

    @Test func metadataKeepsTheNameTheDescriptionAndTheAvatarBytes() async throws {
        let bridge = await makeBridge()

        try await bridge.setRoomMetadata(roomId: "r", metadata: ChatRoomMetadata(
            name: "Lobby",
            avatar: .inline(mimeType: "image/png", bytes: Data([1, 2])),
            description: "Weekly game"
        ))
        try await bridge.setRoomMetadata(roomId: "r", metadata: ChatRoomMetadata(
            name: "Lobby",
            avatar: .preimage(key: Data([7])),
            description: nil
        ))
        try await bridge.setRoomMetadata(roomId: "r", metadata: ChatRoomMetadata(
            name: "Plain",
            avatar: nil,
            description: nil
        ))

        #expect(api.roomMetadata == [
            .init(roomId: "r", name: "Lobby", description: "Weekly game", avatar: .inline(Data([1, 2]))),
            .init(roomId: "r", name: "Lobby", description: nil, avatar: .preimage(key: Data([7]))),
            .init(roomId: "r", name: "Plain", description: nil, avatar: nil)
        ])
    }

    @Test(arguments: PreviewCase.all)
    func eachPreviewVariantReachesTheStore(_ previewCase: PreviewCase) async throws {
        let bridge = await makeBridge()

        try await bridge.setRoomPreview(roomId: "r", preview: previewCase.wire)

        #expect(api.roomPreviews.map(\.preview) == [previewCase.stored])
    }

    @Test func anInputFooterKeepsItsSettings() async throws {
        let bridge = await makeBridge()
        let destination = Data(repeating: 1, count: 32)

        try await bridge.setRoomFooter(roomId: "r", footer: .input(ChatInputConfig(
            placeholder: "Ask",
            photo: false,
            video: true,
            file: true,
            payment: .enabled(destination: destination)
        )))

        #expect(api.footerConfigs.map(\.footer) == [.input(.init(
            placeholder: "Ask",
            photo: false,
            video: true,
            file: true,
            payment: .enabled(destination: destination)
        ))])
    }

    /// `delete_room` is idempotent, so a room that is gone already is not an error.
    @Test func deletingARoomThatIsGoneSucceeds() async throws {
        let bridge = await makeBridge()
        api.unknownRooms = ["gone"]

        try await bridge.deleteRoom(roomId: "gone")
        try await bridge.deleteRoom(roomId: "r")

        #expect(api.deletedRoomIds == ["r"])
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
                metadata: ChatRoomMetadata(name: "x", avatar: nil, description: nil)
            )
        }
        await #expect(throws: ChatRoomError.self) {
            try await bridge.setRoomPreview(roomId: "gone", preview: PreviewCase.all[0].wire)
        }
    }

    @Test func aRequestWithoutARoomIsRefused() async {
        let bridge = await makeBridge()

        await #expect(throws: HostRejection.self) { try await bridge.setRoomConfig(roomId: "", config: .hostDefault) }
        await #expect(throws: HostRejection.self) { try await bridge.deleteRoom(roomId: "") }
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
            wire: ChatRoomPreview(order: .timestamp, timestamp: nil, badge: nil, text: nil),
            stored: .hostDefault
        ),
        PreviewCase(
            testDescription: "pinned with a timestamp",
            wire: ChatRoomPreview(order: .pinToTop, timestamp: 1_700_000_000_000, badge: nil, text: nil),
            stored: .init(
                order: .pinToTop,
                timestamp: 1_700_000_000_000,
                badge: .hostDefault,
                text: nil,
                ongoing: false
            )
        ),
        PreviewCase(
            testDescription: "count badge",
            wire: ChatRoomPreview(order: .timestamp, timestamp: nil, badge: .unreadCount(count: 4), text: nil),
            stored: .init(order: .byTimestamp, timestamp: nil, badge: .unreadCount(4), text: nil, ongoing: false)
        ),
        PreviewCase(
            testDescription: "hidden badge",
            wire: ChatRoomPreview(order: .timestamp, timestamp: nil, badge: .hidden, text: nil),
            stored: .init(order: .byTimestamp, timestamp: nil, badge: .none, text: nil, ongoing: false)
        ),
        PreviewCase(
            testDescription: "inline image badge",
            wire: ChatRoomPreview(
                order: .timestamp,
                timestamp: nil,
                badge: .image(resource: .inline(mimeType: "image/png", bytes: Data([5]))),
                text: nil
            ),
            stored: .init(
                order: .byTimestamp,
                timestamp: nil,
                badge: .image(.inline(Data([5]))),
                text: nil,
                ongoing: false
            )
        ),
        PreviewCase(
            testDescription: "preimage image badge",
            wire: ChatRoomPreview(
                order: .timestamp,
                timestamp: nil,
                badge: .image(resource: .preimage(key: Data([6]))),
                text: nil
            ),
            stored: .init(
                order: .byTimestamp,
                timestamp: nil,
                badge: .image(.preimage(key: Data([6]))),
                text: nil,
                ongoing: false
            )
        ),
        PreviewCase(
            testDescription: "ongoing text",
            wire: ChatRoomPreview(
                order: .timestamp,
                timestamp: nil,
                badge: nil,
                text: ChatRoomPreviewText(text: "Bot is typing", ongoing: true)
            ),
            stored: .init(
                order: .byTimestamp,
                timestamp: nil,
                badge: .hostDefault,
                text: "Bot is typing",
                ongoing: true
            )
        )
    ]
}

private extension RustChatExecutionBridgeRoomTests {
    var config: Chat.RoomConfig {
        Chat.RoomConfig(
            productSource: true,
            reactionDetails: true,
            participants: false,
            messageActions: true,
            allowedReactions: ["👍"],
            placeholder: "Nothing yet",
            reportSeen: true
        )
    }

    var wireConfig: ChatRoomConfig {
        ChatRoomConfig(
            productSource: true,
            reactionDetails: true,
            participants: false,
            messageActions: true,
            allowedReactions: ["👍"],
            placeholder: "Nothing yet",
            reportSeen: true
        )
    }

    func makeBridge() async -> RustChatExecutionBridge {
        await RustChatExecutionBridge(
            dependencies: MainActor.run { makeChatBridgeDependencies() },
            chatMessaging: api
        )
    }

    func declarations(config: Chat.RoomConfig?) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: config,
            configGeneration: 1,
            preview: .hostDefault,
            footerConfig: nil
        )
    }
}
