import Foundation
import Keystore_iOS
import Operation_iOS
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A body that a product posts goes from the rust bridge, through the bound chat surface, into Core Data.
struct ProductBodyStorageTests {
    private let bot = ChatSampleExtension(logger: StubLogger())
    private let facade = UserDataStorageTestFacade()
    private let surface = ProductChatSurface()

    /// Stored rows carry this index, so it never changes.
    @Test func productBodyKeepsItsStoredContentType() {
        #expect(Chat.LocalMessage.Content.ContentType.productBody.rawValue == 247)
    }

    @Test func richTextIsStoredWithItsMedia() async throws {
        let messageId = try await post(.richText(ChatRichText(
            text: "**hi**",
            media: [ChatMedia(url: "https://example.com/a.png")]
        )))

        let expected = Chat.LocalMessage.Content.productBody(.richText(.init(
            text: "**hi**",
            media: ["https://example.com/a.png"]
        )))
        #expect(try await storedMessage(messageId)?.content == expected)
    }

    @Test func actionsAreStoredWithTheirButtonsAndLayout() async throws {
        let messageId = try await post(.actions(ChatActions(
            text: nil,
            actions: [ChatAction(actionId: "a", title: "A"), ChatAction(actionId: "b", title: "B")],
            layout: .grid
        )))

        let expected = Chat.LocalMessage.Content.productBody(.actions(.init(
            text: nil,
            buttons: [.init(actionId: "a", title: "A"), .init(actionId: "b", title: "B")],
            layout: .grid
        )))
        #expect(try await storedMessage(messageId)?.content == expected)
    }

    @Test func aFileIsStoredWithItsAddressAndSize() async throws {
        let messageId = try await post(.file(ChatFile(
            url: "https://example.com/r.pdf",
            fileName: "r.pdf",
            mimeType: "application/pdf",
            sizeBytes: 4_096,
            text: nil
        )))

        let expected = Chat.LocalMessage.Content.productBody(.file(.init(
            url: "https://example.com/r.pdf",
            fileName: "r.pdf",
            mimeType: "application/pdf",
            sizeBytes: 4_096,
            text: nil
        )))
        let stored = try await storedMessage(messageId)
        #expect(stored?.content == expected)
        #expect(stored?.origin == .chatExtension(bot.identifier))
    }
}

private extension ProductBodyStorageTests {
    func post(_ content: ChatMessageContent) async throws -> String {
        let context = ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: StubLogger(),
            messageOrderAllocator: InMemoryChatMessageOrderAllocator()
        )
        _ = try await context.createRoom(for: bot, roomId: "room", name: nil, icon: nil)
        surface.bind(.init(bot: bot, context: context), owner: bot)

        let bridge = await RustChatExecutionBridge(
            dependencies: MainActor.run { makeChatBridgeDependencies() },
            chatMessaging: surface
        )
        return try await bridge.postMessage(roomId: "room", content: content, alt: nil)
    }

    func storedMessage(_ messageId: String) async throws -> Chat.LocalMessage? {
        try await facade.makeRepo(mapper: ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator()))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .first { $0.messageId == messageId }
    }
}
