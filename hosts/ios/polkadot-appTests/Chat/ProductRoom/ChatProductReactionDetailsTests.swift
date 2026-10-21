import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Who reacted to a message of a product room (P6.5, scenario S32).
@MainActor
struct ChatProductReactionDetailsTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()

    @Test func theRouteFollowsTheDeclarationAndTheStoreOfTheRoom() throws {
        #expect(try makePresenter(Data.config()).reactionDetailsRoute == .localRows)
        #expect(try makePresenter(Data.config(productSource: true)).reactionDetailsRoute == .none)
        #expect(try makePresenter(Data.config(reactionDetails: true)).reactionDetailsRoute == .product)
    }

    @Test func theAnswerListsEachParticipantUnderItsEmoji() async throws {
        let stored = Chat.ProductMessageId.stored("host-p-0001", in: Data.chatId)
        room.reactionsAnswer = .success(.init(reactions: [
            .init(participantId: "user", emoji: "👍"),
            .init(participantId: "product", emoji: "❤️"),
            .init(participantId: "p1", emoji: "👍")
        ]))
        let presenter = try makePresenter(Data.config(reactionDetails: true), messages: [Data.message(stored)])

        presenter.showReactionDetails(for: stored)
        for _ in 0 ..< 100 where view.reactionDetails.isEmpty {
            try await Task.sleep(for: .milliseconds(5))
        }

        let details = try #require(view.reactionDetails.first)
        #expect(details.totalCount == 3)
        #expect(details.reactions.map(\.emoji) == ["👍", "❤️"])
        #expect(details.reactions.first?.reactors.map(\.username) == ["alice", "p1"])
        #expect(details.reactions.last?.reactors.map(\.username) == ["Sample"])
        #expect(room.calls.reactions.first?.messageId == "host-p-0001")
    }

    /// `NotAvailable` is not an error: the room hides the affordance and draws again.
    @Test func notAvailableHidesWhoReacted() async throws {
        room.reactionsAnswer = .failure(ChatQueryError.refused(.notAvailable))
        let presenter = try makePresenter(Data.config(reactionDetails: true))
        var redraws = 0
        presenter.onChange = { redraws += 1 }

        presenter.showReactionDetails(for: "m1")
        for _ in 0 ..< 100 where redraws == 0 {
            try await Task.sleep(for: .milliseconds(5))
        }

        #expect(presenter.reactionDetailsRoute == .none)
        #expect(view.reactionDetails.isEmpty)
        #expect(view.failures.isEmpty)
    }

    @Test func aTimeOutShowsNothingAndKeepsTheAffordance() async throws {
        room.reactionsAnswer = .failure(ChatQueryError.timedOut)
        let presenter = try makePresenter(Data.config(reactionDetails: true))

        presenter.showReactionDetails(for: "m1")
        try await Task.sleep(for: .milliseconds(30))

        #expect(presenter.reactionDetailsRoute == .product)
        #expect(view.reactionDetails.isEmpty)
    }
}

private extension ChatProductReactionDetailsTests {
    func makePresenter(
        _ config: Chat.RoomConfig,
        messages: [Chat.LocalMessage] = []
    ) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.view = view
        presenter.update(
            listModel: MessageListModel.stub(messages),
            metadata: MessageListMetadata(
                chatMetadata: ChatMetadata(
                    chatId: Data.chatId,
                    peerMetadata: Chat.PeerMetadata(
                        name: "Sample",
                        icon: .bot,
                        input: .empty,
                        moreActions: []
                    ),
                    state: .created,
                    roomPolicy: Data.policy(config)
                ),
                myUsername: "alice"
            )
        )
        return presenter
    }
}

extension MessageListModel {
    /// A list of messages in one section, for a test that needs a list model.
    static func stub(_ messages: [Chat.LocalMessage]) -> MessageListModel {
        MessageListModel(
            allMessages: messages,
            messagesById: Dictionary(uniqueKeysWithValues: messages.enumerated().map { ($1.messageId, $0) }),
            orderedSections: messages.isEmpty ? [] : [.today],
            messagesBySection: messages.isEmpty ? [:] : [.today: messages],
            reactionsByMessageId: [:],
            latestEditByMessageId: [:],
            initiallyUnreadMessage: nil,
            firstUnreadMessageId: nil,
            oldestNewReactionTargetMessageId: nil,
            newMessageCount: 0
        )
    }
}
