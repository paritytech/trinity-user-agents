import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The "who reacted" list of a product room shows when each participant reacted, from the answer of the product.
struct ProductReactionDetailsTests {
    private let contact = Chat.LocalMessage.Origin.chatExtension("chat.dot")

    @Test func theReactorsTakeTheTimesOfTheProductOldestFirst() {
        let aggregate = aggregate(emoji: "👍", reactors: [.user, contact])
        let details = [
            ChatReactionDetail(participantId: "user", emoji: "👍", timestamp: 3_000),
            ChatReactionDetail(participantId: "contact", emoji: "👍", timestamp: 2_000),
            ChatReactionDetail(participantId: "contact", emoji: "❤️", timestamp: 1_000)
        ]

        let reactors = aggregate.reactors(with: details, contactOrigin: contact)

        #expect(reactors == [
            .init(origin: contact, timestamp: 2_000),
            .init(origin: .user, timestamp: 3_000)
        ])
    }

    @Test func anEmojiThatTheAnswerMissesKeepsTheReactorsOfTheRoom() {
        let aggregate = aggregate(emoji: "🔥", reactors: [contact])

        #expect(aggregate.reactors(with: [], contactOrigin: contact) == aggregate.reactors)
    }
}

private extension ProductReactionDetailsTests {
    func aggregate(emoji: String, reactors: [Chat.LocalMessage.Origin]) -> Chat.MessageReactionAggregate {
        Chat.MessageReactionAggregate(
            messageId: "m1",
            emoji: emoji,
            count: reactors.count,
            reactedByCurrentUser: reactors.contains(.user),
            reactors: reactors.map { .init(origin: $0, timestamp: 1_000) }
        )
    }
}
