import ChainRegistry
import Foundation
import Keystore_iOS
import Operation_iOS
@testable import PolkadotUI
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The chat-list row of a room of the chat product: the contact and the preview that the product sets draw the row
/// of the native person chat, with its preview line, its icon, its date and its badges (HX2).
struct ContactRoomListTests {
    /// The binding of the chat surface holds the bot weakly, so the suite keeps it.
    private let bot = ChatSampleExtension(logger: StubLogger())
    private let account = Data(repeating: 7, count: 32)

    @Test func aTextShowsAsTheNativePreview() async throws {
        let row = try await row(lastMessage: message(.text(text: "Hello")), badge: .unreadCount(count: 2))

        #expect(row.sender == "alice")
        #expect(row.message == "Hello")
        #expect(row.messageKind == .default)
        #expect(row.unreadCount == 2)
        #expect(row.date == Date.fromChatTimestamp(1_000))
    }

    @Test func anEditedTextShowsEdited() async throws {
        var edited = message(.text(text: "Fixed"))
        edited.editedAt = 2_000

        let row = try await row(lastMessage: edited)

        #expect(row.message == String(localized: .chatLastMessageEdited(text: "Fixed")))
    }

    @Test func aPhotoShowsTheMediaLine() async throws {
        let photo = message(.richText(text: nil, media: [
            .image(
                resource: .hop(identifier: Data([1]), ticket: Data([2]), node: "wss://hop", size: 3),
                meta: ChatMediaMeta(
                    mimeType: "image/png",
                    sizeBytes: 3,
                    width: 1,
                    height: 1,
                    placeholder: nil,
                    alt: nil,
                    durationMs: nil
                )
            )
        ]))

        let row = try await row(lastMessage: photo)

        #expect(row.message == String(localized: .chatLastMessageMediaFile))
        #expect(row.messageKind == .media)
    }

    @Test func anAcceptedRequestShowsTheNativeLine() async throws {
        let row = try await row(lastMessage: message(.event(.requestAccepted)))

        #expect(row.message == String(localized: .chatListRequestAcceptedByPeer))
    }

    @Test func aMissedCallShowsTheCallLineAndIcon() async throws {
        let call = message(.call(ChatCallRecord(
            video: false,
            answeredAt: nil,
            closed: ChatCallClose(at: 9_000, byUser: false)
        )))

        let row = try await row(lastMessage: call)

        #expect(row.message == ChatCallMessageConfiguration.title(
            direction: .incoming,
            callType: .audio,
            state: .missed
        ))
        #expect(row.messageKind == .voiceCall)
    }

    /// The native chat keeps the end of a call as the last message of its chat, so the row shows its time.
    @Test func anEndedCallShowsTheTimeOfItsEnd() async throws {
        let call = message(.call(ChatCallRecord(
            video: false,
            answeredAt: nil,
            closed: ChatCallClose(at: 61_000, byUser: false)
        )))

        let row = try await row(lastMessage: call)

        #expect(row.date == Date.fromChatTimestamp(61_000))
        #expect(row.message == ChatCallMessageConfiguration.title(
            direction: .incoming,
            callType: .audio,
            state: .missed
        ))
    }

    @Test func aCanceledCallKeepsItsRingDuration() async throws {
        let call = message(.call(ChatCallRecord(
            video: true,
            answeredAt: nil,
            closed: ChatCallClose(at: 9_000, byUser: true)
        )), from: "user")

        let row = try await row(lastMessage: call)

        #expect(row.date == Date.fromChatTimestamp(9_000))
        #expect(row.messageKind == .videoCall)
        #expect(row.message == ChatCallMessageConfiguration.title(
            direction: .outgoing,
            callType: .video,
            state: .cancelled(ringDuration: DateComponentsFormatter.secondsMinutesAbbreviated.string(from: 8) ?? "")
        ))
    }

    @Test func aRunningCallShowsTheTimeOfItsAnswer() async throws {
        let call = message(.call(ChatCallRecord(video: false, answeredAt: 5_000, closed: nil)))

        let row = try await row(lastMessage: call)

        #expect(row.date == Date.fromChatTimestamp(5_000))
        #expect(row.message == ChatCallMessageConfiguration.title(
            direction: .incoming,
            callType: .audio,
            state: .active
        ))
    }

    @Test func aRingingCallShowsTheTimeOfItsOffer() async throws {
        let call = message(.call(ChatCallRecord(video: false, answeredAt: nil, closed: nil)))

        let row = try await row(lastMessage: call)

        #expect(row.date == Date.fromChatTimestamp(1_000))
    }

    /// The chat list orders the rooms by the time of their last message, which is the end of a call.
    @Test func theListOrdersAnEndedCallByItsEnd() throws {
        let call = message(.call(ChatCallRecord(
            video: false,
            answeredAt: 4_000,
            closed: ChatCallClose(at: 70_000, byUser: true)
        )))
        let preview = Chat.RoomPreview(ChatRoomPreview(
            timestamp: call.timestamp,
            badge: nil,
            lastMessage: call,
            lastReaction: nil,
            unseenReactions: 0
        ))

        let last = try #require(preview.lastMessage?.localMessage(chatId: .chatExtension("chat.dot", roomId: "dm")))

        #expect(last.timestamp == 70_000)
        #expect(last.callOffer?.timestamp == 1_000)
        #expect(last.resolveCallState() == .finished(durationMs: 66_000))
    }

    /// The native chat keeps each signal of a call as a message of its chat: a call that ends after a text sent
    /// during it is the last message of the list again.
    @Test func aCallThatEndsAfterTheLastMessageShowsTheCall() async throws {
        let call = message(.call(ChatCallRecord(
            video: false,
            answeredAt: 2_000,
            closed: ChatCallClose(at: 62_000, byUser: true)
        )), id: "call")
        let text = message(.text(text: "During the call"), from: "user", at: 30_000)

        let row = try await row(lastMessage: text, lastCall: call)

        #expect(row.date == Date.fromChatTimestamp(62_000))
        #expect(row.messageKind == .voiceCall)
        #expect(row.message == ChatCallMessageConfiguration.title(
            direction: .incoming,
            callType: .audio,
            state: .finished(duration: DateComponentsFormatter.secondsMinutesAbbreviated.string(from: 60) ?? "")
        ))
    }

    @Test func aTextAfterTheEndOfTheCallShowsTheText() async throws {
        let call = message(.call(ChatCallRecord(
            video: false,
            answeredAt: 2_000,
            closed: ChatCallClose(at: 20_000, byUser: false)
        )), id: "call")
        let text = message(.text(text: "Bye"), at: 30_000)

        let row = try await row(lastMessage: text, lastCall: call)

        #expect(row.date == Date.fromChatTimestamp(30_000))
        #expect(row.message == "Bye")
        #expect(row.messageKind == .default)
    }

    @Test func aCallAnsweredAfterTheLastMessageShowsTheRunningCall() async throws {
        let call = message(.call(ChatCallRecord(video: true, answeredAt: 8_000, closed: nil)), id: "call")
        let text = message(.text(text: "Calling you"), from: "user", at: 3_000)

        let row = try await row(lastMessage: text, lastCall: call)

        #expect(row.date == Date.fromChatTimestamp(8_000))
        #expect(row.messageKind == .videoCall)
        #expect(row.message == ChatCallMessageConfiguration.title(
            direction: .incoming,
            callType: .video,
            state: .active
        ))
    }

    /// A ringing call has no end or answer, so a message after its offer stays the last message.
    @Test func aMessageAfterARingingCallStaysTheLastMessage() async throws {
        let call = message(.call(ChatCallRecord(video: false, answeredAt: nil, closed: nil)), id: "call")
        let text = message(.text(text: "Pick up"), at: 4_000)

        let row = try await row(lastMessage: text, lastCall: call)

        #expect(row.date == Date.fromChatTimestamp(4_000))
        #expect(row.message == "Pick up")
    }

    /// The list orders the room by the end of the call that it shows.
    @Test func theListOrdersACallShownOverTheLastMessageByItsEnd() throws {
        let call = message(.call(ChatCallRecord(
            video: false,
            answeredAt: 4_000,
            closed: ChatCallClose(at: 70_000, byUser: true)
        )), id: "call")
        let text = message(.text(text: "During the call"), at: 10_000)
        let preview = Chat.RoomPreview(ChatRoomPreview(
            timestamp: text.timestamp,
            badge: nil,
            lastMessage: text,
            lastReaction: nil,
            unseenReactions: 0,
            lastCall: call
        ))

        let last = try #require(preview.lastMessage?.localMessage(chatId: .chatExtension("chat.dot", roomId: "dm")))

        #expect(last.timestamp == 70_000)
        #expect(last.callOffer?.messageId == "call")
        #expect(last.resolveCallState() == .finished(durationMs: 66_000))
    }

    @Test func theWelcomeOfARequestShowsItsText() async throws {
        var welcome = message(.text(text: "Hi, it's Bob"), from: "user")
        welcome.request = true

        let row = try await row(lastMessage: welcome, state: .outgoing)

        #expect(row.message == "Hi, it's Bob")
    }

    @Test func anUnseenReactionShowsTheHeart() async throws {
        let row = try await row(lastMessage: message(.text(text: "Hello")), unseenReaction: true)

        #expect(row.hasReaction)
    }

    @Test func theAvatarHasTheColorOfTheNativeChatWithTheAccount() async throws {
        let row = try await row(lastMessage: message(.text(text: "Hello")))

        #expect(row.avatarViewModel == .colored(text: "a", colorSeed: account.toHex()))
    }

    @Test func aRoomWithoutAUsernameShowsTheNameOfTheRoom() async throws {
        let row = try await row(lastMessage: message(.text(text: "Hello")), username: nil)

        #expect(row.sender == "Game")
    }

    @Test(arguments: [
        (Chat.RoomContact.State.invite, false),
        (.outgoing, true),
        (.incoming, true),
        (.established, true),
        (.blocked, false),
        (.peerLeft, true)
    ])
    func theListShowsNoInviteAndNoBlockedContact(state: Chat.RoomContact.State, isListed: Bool) {
        #expect(Chat.RoomContact(account: account, username: "alice", state: state).isListed == isListed)
    }

    @Test func aPreviewForAnUnknownRoomIsRefused() async throws {
        let context = makeContext(UserDataStorageTestFacade())
        let preview = Chat.RoomPreview(timestamp: nil, badge: .hostDefault, lastMessage: nil, unseenReaction: false)

        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomPreview(for: bot, roomId: "missing", preview: preview)
        }
    }
}

private extension ContactRoomListTests {
    func message(
        _ body: ChatMessageBody,
        from participantId: String = "contact",
        id: String = "m1",
        at timestamp: UInt64 = 1_000
    ) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: participantId),
            timestamp: timestamp,
            body: body,
            status: .read,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
    }

    /// The row of a room that the product created, with its contact and its preview, as the store gives it.
    func row(
        lastMessage: ChatMessage,
        lastCall: ChatMessage? = nil,
        badge: ChatRoomBadge? = nil,
        unseenReaction: Bool = false,
        state: Chat.RoomContact.State = .established,
        username: String? = "alice"
    ) async throws -> DSChatListItemConfiguration {
        let facade = UserDataStorageTestFacade()
        let context = makeContext(facade)
        _ = try await context.createRoom(for: bot, roomId: "dm", name: "Game", icon: nil)
        try await context.setRoomContact(
            for: bot,
            roomId: "dm",
            contact: Chat.RoomContact(account: account, username: username, state: state)
        )
        try await context.setRoomPreview(for: bot, roomId: "dm", preview: Chat.RoomPreview(ChatRoomPreview(
            timestamp: lastMessage.timestamp,
            badge: badge,
            lastMessage: lastMessage,
            lastReaction: nil,
            unseenReactions: unseenReaction ? 1 : 0,
            lastCall: lastCall
        )))

        let chat = try #require(try await AnyDataProviderRepository(
            facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
        )
        .fetchOperation(
            by: { Chat.Id.chatExtension(bot.identifier, roomId: "dm").rawRepresentation },
            options: RepositoryFetchOptions()
        )
        .asyncExecute())

        return try row(for: chat.chatWithPeerMetadata(using: SingleBotRegistry(bot: bot)))
    }

    func row(for chat: ChatWithPeerMetadata) throws -> DSChatListItemConfiguration {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let factory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: FixedChatMessageDecoderFactory(decoders: []),
            productIconViewModelFactory: RecordingProductIconViewModelFactory(),
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )

        let viewModel = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(establishedChats: [chat], pendingIncomingRequestCount: 0, newIncomingRequestCount: 0)
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
