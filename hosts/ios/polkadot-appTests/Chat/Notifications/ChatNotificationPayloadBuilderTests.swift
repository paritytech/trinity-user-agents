import Foundation
import Testing
import SubstrateSdk

@testable import polkadot_app

/// Mirrors Android's `ChatNotificationPayloadEncoderTest` so both
/// senders strip under the same conditions.
struct ChatNotificationPayloadBuilderTests {
    fileprivate typealias Content = Chat.RemoteMessageContentV1.MessageContent

    private let builder = ChatNotificationPayloadBuilder(logger: MockLogger())

    @Test func smallTextIsSentInFull() throws {
        let message = makeMessage(.richText(.init(text: "hi", attachments: nil)))

        let payload = try builder.makePayload(for: message)

        #expect(payload.fullMessage == message)
    }

    @Test func paymentWithFewCoinsIsSentInFullWithKeys() throws {
        let message = try makeMessage(.coinageSend(makePayment(coins: 3)))

        let payload = try builder.makePayload(for: message)

        #expect(payload.fullMessage == message)
    }

    @Test func paymentWithManyCoinsIsStrippedToItsTotal() throws {
        let payment = try makePayment(coins: 60)
        let message = makeMessage(.coinageSend(payment))

        let payload = try builder.makePayload(for: message)

        #expect(payload.messageId == message.messageId)
        #expect(payload.timestamp == message.timestamp)
        #expect(payload.versioned == .v1(.stripped(.coinageSend(.init(totalValue: payment.totalValue)))))
    }

    @Test func oversizedCallOfferIsStrippedAndKeepsItsPurpose() throws {
        let offer = try Content.DataChannelOfferContent(sdp: Data.randomOrError(of: 2_000), purpose: .video)
        let message = makeMessage(.dataChannelOffer(offer))

        let payload = try builder.makePayload(for: message)

        #expect(payload.fullMessage == nil)
        #expect(payload.dataChannelOffer == offer)
    }

    @Test func oversizedTextIsStillSentStripped() throws {
        let text = String(repeating: "a", count: 2_000)
        let message = makeMessage(.richText(.init(text: text, attachments: nil)))

        let payload = try builder.makePayload(for: message)

        #expect(payload.versioned == .v1(.stripped(.richText(.init(text: text, attachments: nil)))))
    }

    @Test func oversizedMessageWithoutStrippedFormIsSentInFull() throws {
        let text = String(repeating: "a", count: 2_000)
        let message = makeMessage(.edited(.init(messageId: "e", newContent: .init(text: text, attachments: nil))))

        let payload = try builder.makePayload(for: message)

        #expect(payload.fullMessage == message)
    }

    @Test func budgetBoundaryIsInclusive() throws {
        let atBudget = makeMessage(.text(String(repeating: "b", count: 1_784)))
        let overBudget = makeMessage(.text(String(repeating: "b", count: 1_785)))
        try #require(Chat.NotificationPayload(full: atBudget).scaleEncoded().count == 1_800)

        #expect(try builder.makePayload(for: atBudget).fullMessage == atBudget)
        #expect(try builder.makePayload(for: overBudget).fullMessage == nil)
    }

    @Test func unsupportedVersionIsRejected() {
        let message = Chat.RemoteMessage(messageId: "m1", timestamp: 1, versioned: .unsupported(Data()))

        #expect(throws: Chat.RemoteCodingError.self) {
            try builder.makePayload(for: message)
        }
    }
}

private extension ChatNotificationPayloadBuilderTests {
    func makeMessage(_ content: Content) -> Chat.RemoteMessage {
        Chat.RemoteMessage(messageId: "m1", timestamp: 1, versioned: .v1(.init(content: content)))
    }

    func makePayment(coins: Int) throws -> Content.SendContent.Coinage {
        let keys = try (0 ..< coins).map { _ in try Data.randomOrError(of: 32) }
        return Content.SendContent.Coinage(totalValue: Balance(1_000_000_000_000), coinKeys: keys)
    }
}
