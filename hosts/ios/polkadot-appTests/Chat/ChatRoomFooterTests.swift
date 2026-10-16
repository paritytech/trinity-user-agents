import AsyncExtensions
import Foundation
import Keystore_iOS
import Operation_iOS
import Testing
@testable import polkadot_app

struct ChatRoomFooterTests {
    private let bot = ChatSampleExtension(logger: StubLogger())
    private let facade = UserDataStorageTestFacade()

    private func room(footer: Chat.RoomFooter?) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension(bot.identifier, roomId: "room"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomFooter: footer
        )
    }

    @Test func aRoomWithAnEmptyFooterShowsNoInput() {
        let registry = SingleBotRegistry(bot: bot)

        #expect(room(footer: .empty).peerMetadata(using: registry).input == .empty)
    }

    /// A room the product never set a footer on keeps the text input.
    @Test(arguments: [nil, Chat.RoomFooter.textInput])
    func anyOtherRoomShowsTheBotInput(footer: Chat.RoomFooter?) {
        let registry = SingleBotRegistry(bot: bot)

        #expect(room(footer: footer).peerMetadata(using: registry).input == bot.peerMetadata.input)
    }

    @Test func anEmptyFooterIsStoredOnTheRoom() async throws {
        let context = makeContext()
        _ = try await context.createRoom(for: bot, roomId: "room", name: nil, icon: nil)

        try await context.setRoomFooter(for: bot, roomId: "room", footer: .empty)

        #expect(try await storedRoom()?.roomFooter == .empty)
    }

    @Test func aFooterForARoomNeverCreatedIsRefused() async throws {
        let context = makeContext()

        await #expect(throws: RoomFooterError.self) {
            try await context.setRoomFooter(for: bot, roomId: "room", footer: .empty)
        }
    }

    /// A room shows the text input anyway, so asking for it writes nothing.
    @Test func askingForTheTextInputLeavesAFreshRoomUntouched() async throws {
        let context = makeContext()
        _ = try await context.createRoom(for: bot, roomId: "room", name: nil, icon: nil)

        try await context.setRoomFooter(for: bot, roomId: "room", footer: .textInput)

        #expect(try await storedRoom()?.roomFooter == nil)
    }
}

private extension ChatRoomFooterTests {
    func makeContext() -> ChatExtensionDiscoverContext {
        ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: StubLogger()
        )
    }

    func storedRoom() async throws -> Chat.LocalModel? {
        let chatId = Chat.Id.chatExtension(bot.identifier, roomId: "room").rawRepresentation
        return try await facade.makeRepo(mapper: ChatModelMapper())
            .fetchOperation(by: { chatId }, options: RepositoryFetchOptions())
            .asyncExecute()
    }
}

extension InMemorySettingsManager: ChatExtensionBotSettings {}

private struct SingleBotRegistry: ChatExtensionsRegistering {
    let bot: ChatExtensionBotProtocol

    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> {
        AsyncStream<ChatExtensionRegistryChange> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func discover() {}

    func getExtensions(for _: Chat.Id) -> [ChatExtending] { [bot] }

    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol? {
        extensionId == bot.identifier ? bot : nil
    }

    func getWidgetProviders() -> [ChatExtensionWidgetProvider] { [] }
}
