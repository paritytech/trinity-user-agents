import ChainRegistry
import Foundation
@testable import PolkadotUI
import Testing
@testable import polkadot_app

/// The mute and the archive actions of a product room (P10.10, 07-LATER §6.2): each change keeps the other half of
/// the state, reaches the product, and the chat list draws it.
struct ChatRoomUserStateServiceTests {
    private let fixture = ProductRoomFixture()
    private let publisher = RecordingRoomEventPublisher()
    private static let now = Date(timeIntervalSince1970: 1_800_000_000)

    @Test func aMuteForOneHourEndsAnHourLaterAndKeepsTheArchive() async throws {
        try await createRoom()
        let service = service()
        try await service.setArchived(true, for: fixture.chatId("lobby"))

        try await service.mute(fixture.chatId("lobby"), for: .oneHour)

        let expected = Chat.RoomUserState(mutedUntil: 1_800_003_600_000, archived: true)
        #expect(try await service.state(of: fixture.chatId("lobby")) == expected)
        #expect(publisher.events.last == .init(
            event: .userStateChanged(expected),
            productId: "game.dot",
            roomId: "lobby"
        ))
    }

    @Test func eachDurationGivesItsEnd() {
        let start = Self.now.toChatTimestamp()
        let ends = Chat.MuteDuration.allCases.map { $0.mutedUntil(from: Self.now) }

        #expect(ends == [start + 3_600_000, start + 28_800_000, start + 604_800_000, Chat.RoomUserState.mutedForever])
    }

    @Test func anUnmuteAndAnUnarchiveGiveTheDefaultStateBack() async throws {
        try await createRoom()
        let service = service()
        try await service.mute(fixture.chatId("lobby"), for: .always)
        try await service.setArchived(true, for: fixture.chatId("lobby"))

        try await service.unmute(fixture.chatId("lobby"))
        try await service.apply(.archive(false), to: fixture.chatId("lobby"))

        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.userState == .default)
        #expect(publisher.events.map(\.event) == [
            .userStateChanged(Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: false)),
            .userStateChanged(Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: true)),
            .userStateChanged(Chat.RoomUserState(mutedUntil: nil, archived: true)),
            .userStateChanged(.default)
        ])
    }

    /// PARITY row 24: a change that changes nothing stores nothing and does not tell the product.
    @Test func aChangeThatChangesNothingIsNotPublished() async throws {
        try await createRoom()
        let service = service()

        try await service.unmute(fixture.chatId("lobby"))
        try await service.setArchived(false, for: fixture.chatId("lobby"))
        try await service.setArchived(true, for: fixture.chatId("lobby"))
        try await service.setArchived(true, for: fixture.chatId("lobby"))

        #expect(publisher.events.map(\.event) == [
            .userStateChanged(Chat.RoomUserState(mutedUntil: nil, archived: true))
        ])
    }

    /// PARITY row 24: the next change removes a mute that ended, so the product does not get an old end time.
    @Test func theNextChangeRemovesAMuteThatEnded() async throws {
        try await createRoom()
        try await service(now: Self.now.addingTimeInterval(-7_200)).mute(fixture.chatId("lobby"), for: .oneHour)

        try await service().setArchived(true, for: fixture.chatId("lobby"))

        let expected = Chat.RoomUserState(mutedUntil: nil, archived: true)
        #expect(try await service().state(of: fixture.chatId("lobby")) == expected)
        #expect(publisher.events.last?.event == .userStateChanged(expected))
    }

    /// PARITY row 24: changes that start together run one after the other, so no change loses the other half.
    @Test func changesThatStartTogetherKeepEachOther() async throws {
        try await createRoom()
        let service = service()
        let chatId = fixture.chatId("lobby")

        async let muted: Void = service.mute(chatId, for: .always)
        async let archived: Void = service.setArchived(true, for: chatId)
        _ = try await (muted, archived)

        let expected = Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: true)
        #expect(try await service.state(of: chatId) == expected)
        #expect(publisher.events.count == 2)
    }

    @Test func aRoomThatTheAppDoesNotStoreHasTheDefaultState() async throws {
        #expect(try await service().state(of: fixture.chatId("missing")) == .default)
    }

    /// A mute ends by itself: the row draws the muted state only until the end.
    @Test func aMuteThatEndedIsNotMuted() {
        let state = Chat.RoomUserState(mutedUntil: Self.now.toChatTimestamp(), archived: false)

        #expect(state.isMuted(at: Self.now.addingTimeInterval(-1)))
        #expect(!state.isMuted(at: Self.now.addingTimeInterval(1)))
    }

    @Test func theMainListShowsWhatIsNotArchivedAndTheArchiveTheRest() {
        let archived = room(id: "a", state: Chat.RoomUserState(mutedUntil: nil, archived: true))
        let muted = room(
            id: "m",
            state: Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: false)
        )

        #expect([archived, muted].filter(ChatListScope.main.shows).map(\.chatId.roomId) == ["m"])
        #expect([archived, muted].filter(ChatListScope.archived.shows).map(\.chatId.roomId) == ["a"])
    }

    @Test func aMutedRoomDrawsTheMuteIconAndTheArchivedRowCountsTheArchive() throws {
        let muted = room(
            id: "m",
            state: Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: false)
        )
        let plain = room(id: "p", state: .default)

        let viewModel = try listViewModel([muted, plain], archivedCount: 2)

        #expect(viewModel.contactsById.map(\.configuration.isMuted) == [true, false])
        #expect(viewModel.archivedRow?.count == 2)
        #expect(try listViewModel([plain], archivedCount: 0).archivedRow == nil)
    }

    @Test func theScheduledNotificationOfAMutedRoomShowsNothing() async throws {
        try await createRoom()
        try await service().mute(fixture.chatId("lobby"), for: .always)
        let added = AddedNotifications()
        let scheduler = ProductNotificationScheduler(
            storageFacade: fixture.storageFacade,
            notificationAdding: { added.append($0.identifier) },
            logger: StubLogger()
        )
        let link = "polkadot://game.dot/-/chat/lobby"

        _ = try await scheduler.schedule(
            productId: "game.dot",
            request: .init(text: "now", deeplink: link, scheduledAtMs: nil)
        )
        _ = try await scheduler.schedule(
            productId: "game.dot",
            request: .init(text: "later", deeplink: link, scheduledAtMs: Date().toChatTimestamp() + 60_000)
        )

        #expect(added.identifiers.isEmpty)
    }

    /// Only the product of the room is silenced. A notification without a room link and one into a room that is not
    /// muted show.
    @Test func otherNotificationsShow() async throws {
        try await createRoom()
        try await service().mute(fixture.chatId("lobby"), for: .always)
        let added = AddedNotifications()
        let scheduler = ProductNotificationScheduler(
            storageFacade: fixture.storageFacade,
            notificationAdding: { added.append($0.identifier) },
            logger: StubLogger()
        )

        for (productId, deeplink) in [
            ("game.dot", nil),
            ("game.dot", "polkadot://game.dot/-/chat/other"),
            ("other.dot", "polkadot://game.dot/-/chat/lobby")
        ] {
            _ = try await scheduler.schedule(
                productId: productId,
                request: .init(text: "x", deeplink: deeplink, scheduledAtMs: nil)
            )
        }

        #expect(added.identifiers.count == 3)
    }
}

private extension ChatRoomUserStateServiceTests {
    func createRoom() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
    }

    func service(now: Date = Self.now) -> ChatRoomUserStateService {
        ChatRoomUserStateService(
            reader: ChatRoomUserStateReader(storageFacade: fixture.storageFacade),
            recorder: ChatRoomUserStateRecorder(
                storageFacade: fixture.storageFacade,
                publisher: publisher,
                logger: StubLogger()
            ),
            now: { now }
        )
    }

    func room(id: String, state: Chat.RoomUserState) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension("game.dot", roomId: id),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: id, name: id, icon: nil),
            roomDeclarations: Chat.RoomDeclarations(
                description: nil,
                avatar: nil,
                config: nil,
                configGeneration: 0,
                preview: .hostDefault,
                toolbar: nil,
                suggestions: nil,
                commands: nil,
                footerConfig: nil,
                features: .hostDefault,
                userState: state
            )
        )
    }

    func listViewModel(_ chats: [Chat.LocalModel], archivedCount: Int) throws -> ContactsListViewLayout.ViewModel {
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
        let peerMetadata = Chat.PeerMetadata(name: "Game", icon: .bot, input: .empty, moreActions: [])

        return factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: chats.map { ChatWithPeerMetadata(chat: $0, peerMetadata: peerMetadata) },
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0,
                archivedChatCount: archivedCount
            )
        )
    }
}

/// The identifiers of the notifications that the scheduler gave to the OS.
private final class AddedNotifications: @unchecked Sendable {
    private let lock = NSLock()
    private var added: [String] = []

    var identifiers: [String] {
        lock.withLock { added }
    }

    func append(_ identifier: String) {
        lock.withLock { added.append(identifier) }
    }
}
