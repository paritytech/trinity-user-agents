import Foundation
import Testing

@testable import polkadot_app

@Suite("Chat call state")
struct ChatCallStateTests {
    private typealias MessageContent = Chat.RemoteMessageContentV1.MessageContent

    private let offerId = "offer"
    private let contactId = Data(repeating: 0x01, count: 32)

    @Test func answeredCallIsTimedFromAnswerNotFromRinging() {
        let message = makeOffer(at: 1_000, related: [
            answer(at: 11_000),
            closed(at: 71_000)
        ])

        #expect(message.resolveCallState() == .finished(durationMs: 60_000))
    }

    @Test func earliestAnswerAndCloseCountWhenSeveralArriveOutOfOrder() {
        let message = makeOffer(at: 1_000, related: [
            closed(at: 71_000),
            answer(at: 12_000),
            closed(at: 65_000),
            answer(at: 11_000)
        ])

        #expect(message.resolveCallState() == .finished(durationMs: 54_000))
    }
}

private extension ChatCallStateTests {
    func makeOffer(at timestamp: Chat.Timestamp, related: [Chat.RelatedLocalMessage]) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: offerId,
            chatId: .person(contactId),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.delivered),
            timestamp: timestamp,
            content: .call(.offer(MessageContent.DataChannelOfferContent(sdp: Data(), purpose: .audio))),
            reactions: [],
            compactionId: nil,
            relatedMessages: related
        )
    }

    func answer(at timestamp: Chat.Timestamp) -> Chat.RelatedLocalMessage {
        Chat.RelatedLocalMessage(
            messageId: "answer-\(timestamp)",
            timestamp: timestamp,
            content: .call(.answer(MessageContent.DataChannelAnswerContent(offerId: offerId, sdp: Data()))),
            status: .incoming(.new)
        )
    }

    func closed(at timestamp: Chat.Timestamp) -> Chat.RelatedLocalMessage {
        Chat.RelatedLocalMessage(
            messageId: "closed-\(timestamp)",
            timestamp: timestamp,
            content: .call(.closed(MessageContent.DataChannelClosedContent(offerId: offerId))),
            status: .incoming(.new)
        )
    }
}
