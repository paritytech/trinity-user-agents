import ChainRegistry
import Foundation
import Keystore_iOS
import Operation_iOS
@testable import PolkadotUI
import Testing
@testable import polkadot_app

/// The chat-list row of a product room with the preview that its product set (02-DESIGN §4, §8.7, §13 item 4).
struct ContactsListPreviewTests {
    /// The binding of the chat surface holds the bot weakly, so the suite keeps it.
    private let bot = ChatSampleExtension(logger: StubLogger())

    @Test func aRoomWithoutAPreviewShowsTheLastMessageAndTheHostCount() throws {
        let row = try row(for: room(preview: nil, unread: 2, hasReaction: true))

        #expect(row.message == "hello")
        #expect(row.date == Date.fromChatTimestamp(messageTimestamp))
        #expect(row.unreadCount == 2)
        #expect(row.hasReaction)
        #expect(row.badge == nil)
        #expect(!row.isOngoing)
    }

    /// A room that rings shows "Incoming call" as an activity that continues (07-LATER §11.2, S73).
    @Test func aRingingRoomShowsIncomingCall() throws {
        let chat = room(preview: preview(text: "Your turn"))
        let calls = ChatRoomCallStore()
        calls.set(.ringing(callId: "c1", video: false, caller: nil), for: chat.chatId)

        let row = try row(for: chat, calls: calls)

        #expect(row.message == String(localized: .ChatCall.ringingTitle))
        #expect(row.isOngoing)
    }

    @Test func aRoomWithAnActiveCallKeepsItsPreview() throws {
        let chat = room(preview: preview(text: "Your turn"))
        let calls = ChatRoomCallStore()
        calls.set(.active(callId: "c1", video: false, startedAt: 1, mediaSession: nil), for: chat.chatId)

        let row = try row(for: chat, calls: calls)

        #expect(row.message == "Your turn")
        #expect(!row.isOngoing)
    }

    @Test func theProductTextReplacesTheLastMessage() throws {
        let row = try row(for: room(preview: preview(text: "Your turn")))

        #expect(row.message == "Your turn")
        #expect(row.messageKind == .default)
        #expect(!row.isOngoing)
    }

    @Test func anOngoingTextIsDrawnAsAnActivity() throws {
        let row = try row(for: room(preview: preview(text: "Alice is thinking", ongoing: true)))

        #expect(row.message == "Alice is thinking")
        #expect(row.isOngoing)
    }

    @Test func theProductTimestampIsTheDateOfTheRow() throws {
        let row = try row(for: room(preview: preview(timestamp: 1_800_000_000_000)))

        #expect(row.date == Date.fromChatTimestamp(1_800_000_000_000))
        #expect(row.message == "hello")
    }

    /// A preview without a timestamp or a badge keeps the host date and the host count.
    @Test func aPreviewWithOnlyATextKeepsTheHostDateAndCount() throws {
        let row = try row(for: room(preview: preview(text: "Your turn"), unread: 3, hasReaction: true))

        #expect(row.date == Date.fromChatTimestamp(messageTimestamp))
        #expect(row.unreadCount == 3)
        #expect(row.hasReaction)
        #expect(row.badge == nil)
    }

    @Test func aNoBadgePreviewHidesTheHostBadges() throws {
        let row = try row(for: room(preview: preview(badge: .none), unread: 3, hasReaction: true))

        #expect(row.badge == .hidden)
        #expect(row.unreadCount == 0)
        #expect(!row.hasReaction)
    }

    @Test func aCountBadgeReplacesTheHostCount() throws {
        let row = try row(for: room(preview: preview(badge: .unreadCount(7)), unread: 3))

        #expect(row.badge == .counter(7))
        #expect(row.unreadCount == 0)
    }

    @Test func anImageBadgeLoadsTheImageOfTheProduct() throws {
        let image = Chat.RoomImage.preimage(key: Data(repeating: 1, count: 32))
        let row = try row(for: room(preview: preview(badge: .image(image))))

        guard case let .image(source) = row.badge else {
            Issue.record("the row must show the image badge")
            return
        }
        #expect(source.id == image.identifier)
        #expect(source.viewModel is ChatResourceImageViewModel)
    }

    @Test func aPinnedRoomGoesAboveNewerChats() {
        let newer = room(id: "newer", preview: nil, messageAt: 3_000, order: 3)
        let pinned = room(id: "pinned", preview: preview(order: .pinToTop), messageAt: 1_000, order: 1)

        let ordered = ChatsComparator.chatListOrder([newer, pinned])

        #expect(ordered.map(\.chatId.roomId) == ["pinned", "newer"])
    }

    @Test func aProductTimestampPlacesTheRoomAmongTheLastMessages() {
        let newest = room(id: "newest", preview: nil, messageAt: 3_000, order: 3)
        let oldest = room(id: "oldest", preview: nil, messageAt: 1_000, order: 1)
        let product = room(id: "product", preview: preview(timestamp: 2_000), messageAt: 500, order: 4)

        let ordered = ChatsComparator.chatListOrder([oldest, product, newest])

        #expect(ordered.map(\.chatId.roomId) == ["newest", "product", "oldest"])
    }

    @Test func chatsWithoutAProductPreviewKeepTheOrderOfTheirLastMessages() {
        let first = room(id: "first", preview: nil, messageAt: 1_000, order: 2)
        let second = room(id: "second", preview: nil, messageAt: 2_000, order: 1)

        let ordered = ChatsComparator.chatListOrder([second, first])

        #expect(ordered.map(\.chatId.roomId) == ["first", "second"])
    }
}

// MARK: - Write path

extension ContactsListPreviewTests {
    @Test func aPreviewThatTheProductSetsReachesTheChatList() async throws {
        let facade = UserDataStorageTestFacade()
        let context = makeContext(facade)
        let surface = ProductChatSurface()
        surface.bind(.init(bot: bot, context: context), owner: bot)
        _ = try await context.createRoom(for: bot, roomId: "room", name: "Game", icon: nil)
        let stored = preview(text: "Your turn", ongoing: true, badge: .unreadCount(1), timestamp: 42)

        try await surface.setRoomPreview(roomId: "room", preview: stored)

        let chat = try #require(try await AnyDataProviderRepository(
            facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
        )
        .fetchOperation(
            by: { Chat.Id.chatExtension(bot.identifier, roomId: "room").rawRepresentation },
            options: RepositoryFetchOptions()
        )
        .asyncExecute())
        #expect(chat.roomDeclarations?.preview == stored)
        #expect(chat.roomMetadata?.name == "Game")

        let row = try row(for: chat)
        #expect(row.message == "Your turn")
        #expect(row.badge == .counter(1))
    }

    @Test func aPreviewForAnUnknownRoomIsRefused() async throws {
        let context = makeContext(UserDataStorageTestFacade())

        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomPreview(for: bot, roomId: "missing", preview: preview(text: "x"))
        }
    }
}

private let messageTimestamp: UInt64 = 1_700_000_000_000

private extension ContactsListPreviewTests {
    func preview(
        order: Chat.RoomPreview.Order = .byTimestamp,
        text: String? = nil,
        ongoing: Bool = false,
        badge: Chat.RoomBadge = .hostDefault,
        timestamp: Int64? = nil
    ) -> Chat.RoomPreview {
        Chat.RoomPreview(order: order, timestamp: timestamp, badge: badge, text: text, ongoing: ongoing)
    }

    func room(
        id: String = "room",
        preview: Chat.RoomPreview?,
        unread: Int = 0,
        hasReaction: Bool = false,
        messageAt timestamp: UInt64 = messageTimestamp,
        order: UInt64 = 1
    ) -> Chat.LocalModel {
        let chatId = Chat.Id.chatExtension("game.dot", roomId: id)
        let message = Chat.LocalMessage(
            messageId: "message-\(id)",
            chatId: chatId,
            origin: .chatExtension("game.dot"),
            creationSource: .localDevice,
            status: .incoming(.seen),
            timestamp: timestamp,
            content: .text("hello"),
            reactions: [],
            compactionId: nil,
            relatedMessages: [],
            order: order
        )

        return Chat.LocalModel(
            peer: .chatExtension("game.dot", roomId: id),
            message: message,
            unreadDisplayMessageCount: unread,
            hasIncomingReaction: hasReaction,
            createdAt: nil,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: id, name: "Game", icon: nil),
            roomDeclarations: preview.map(declarations(with:))
        )
    }

    func declarations(with preview: Chat.RoomPreview) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: nil,
            configGeneration: 0,
            preview: preview,
            footerConfig: nil
        )
    }

    func row(
        for chat: Chat.LocalModel,
        calls: ChatRoomCallStoring = ChatRoomCallStore()
    ) throws -> DSChatListItemConfiguration {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let factory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: FixedChatMessageDecoderFactory(decoders: []),
            productIconViewModelFactory: RecordingProductIconViewModelFactory(),
            calls: calls,
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )
        let peerMetadata = Chat.PeerMetadata(name: "Game", icon: .bot, input: .empty, moreActions: [])

        let viewModel = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: [ChatWithPeerMetadata(chat: chat, peerMetadata: peerMetadata.list)],
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0
            )
        )

        return try #require(viewModel.contactsById.first).configuration
    }

    func makeContext(_ facade: UserDataStorageTestFacade) -> ChatExtensionDiscoverContext {
        ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: StubLogger(),
            messageOrderAllocator: InMemoryChatMessageOrderAllocator()
        )
    }
}
