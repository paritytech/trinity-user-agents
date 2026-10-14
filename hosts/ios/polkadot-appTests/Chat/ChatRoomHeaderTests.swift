import AsyncExtensions
import Foundation
import Keystore_iOS
import Operation_iOS
import PolkadotUI
import Testing
@testable import polkadot_app

/// The header of a product room shows its description and its avatar (02-DESIGN §8.1, P3.5).
struct ChatRoomHeaderTests {
    private let bot = ChatSampleExtension(logger: StubLogger())

    @Test func theDescriptionIsTheLineUnderTheName() {
        let header = header(for: room(description: "Weekly game", avatar: nil))

        #expect(header.username == "Lobby")
        #expect(header.additionalInfo == "Weekly game")
    }

    @Test func aRoomWithoutADescriptionShowsNoLine() {
        #expect(header(for: room(description: nil, avatar: nil)).additionalInfo == nil)
    }

    @Test func theDeclaredAvatarLoadsThroughTheResourceResolver() {
        let key = Data(repeating: 7, count: 32)
        let iconFactory = RecordingProductIconViewModelFactory()

        let header = header(for: room(description: nil, avatar: .preimage(key: key)), iconFactory: iconFactory)

        #expect(header.icon != nil)
        #expect(iconFactory.requestedResources == [.preimage(key: key)])
    }

    @Test func theDeclaredAvatarWinsOverTheV1Icon() {
        let chat = room(description: nil, avatar: .inline(Data([1, 2])), icon: "https://example.com/icon.png")

        #expect(chat.peerMetadata(using: SingleBotRegistry(bot: bot)).icon == .roomImage(.inline(Data([1, 2]))))
    }

    @Test func aRoomWithoutAnAvatarKeepsTheV1Icon() {
        let chat = room(description: nil, avatar: nil, icon: "https://example.com/icon.png")

        #expect(chat.peerMetadata(using: SingleBotRegistry(bot: bot)).icon == .resource("https://example.com/icon.png"))
    }

    @Test func theMetadataThatTheProductSetsReachesTheHeader() async throws {
        let facade = UserDataStorageTestFacade()
        let context = ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: StubLogger(),
            messageOrderAllocator: InMemoryChatMessageOrderAllocator()
        )
        let surface = ProductChatSurface()
        surface.bind(.init(bot: bot, context: context), owner: bot)
        _ = try await context.createRoom(for: bot, roomId: "lobby", name: "Old", icon: "https://example.com/v1.png")

        try await surface.setRoomMetadata(
            roomId: "lobby",
            name: "Lobby",
            description: "Weekly game",
            avatar: .preimage(key: Data([9]))
        )

        let chat = try #require(try await AnyDataProviderRepository(
            facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
        )
        .fetchOperation(
            by: { Chat.Id.chatExtension(bot.identifier, roomId: "lobby").rawRepresentation },
            options: RepositoryFetchOptions()
        )
        .asyncExecute())
        #expect(chat.roomMetadata == Chat.RoomMetadata(
            chatRelativeId: "lobby",
            name: "Lobby",
            icon: "https://example.com/v1.png"
        ))
        #expect(header(for: chat).additionalInfo == "Weekly game")
        #expect(chat.peerMetadata(using: SingleBotRegistry(bot: bot)).icon == .roomImage(.preimage(key: Data([9]))))
    }
}

private extension ChatRoomHeaderTests {
    func room(description: String?, avatar: Chat.RoomImage?, icon: String? = nil) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension(bot.identifier, roomId: "lobby"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: "lobby", name: "Lobby", icon: icon),
            roomDeclarations: Chat.RoomDeclarations(
                description: description,
                avatar: avatar,
                config: nil,
                configGeneration: 0,
                preview: .hostDefault,
                toolbar: nil,
                suggestions: nil,
                commands: nil,
                footerConfig: nil
            )
        )
    }

    func header(
        for chat: Chat.LocalModel,
        iconFactory: ProductIconViewModelMaking = RecordingProductIconViewModelFactory()
    ) -> ChatHeaderConfiguration {
        chat.chatMetadata(using: SingleBotRegistry(bot: bot)).chatContactInfo(iconViewModelFactory: iconFactory)
    }
}
