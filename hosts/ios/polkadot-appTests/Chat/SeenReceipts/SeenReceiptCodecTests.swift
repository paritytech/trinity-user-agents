import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

struct SeenReceiptCodecTests {
    @Test func roundTripsSeenContent() throws {
        let content = makeSeenContent()
        let remoteMessage = makeRemoteMessage(content: content)

        let encoded = try remoteMessage.scaleEncoded()
        let decoded = try Chat.RemoteMessage.fromScaleEncoded(encoded)

        #expect(decoded.versioned.ensureV1()?.content == content)
    }

    @Test func seenContentDeclaresIndexTwentyTwo() throws {
        let content = makeSeenContent()

        let encoded = try content.scaleEncoded()

        #expect(encoded.first == 22)
    }

    @Test func seenContentIsNeverDisplayedOrNotified() throws {
        let content = makeSeenContent()
        let remoteMessage = makeRemoteMessage(content: content)

        #expect(Chat.LocalMessage.Content(remoteContent: .init(content: content)) == nil)
        #expect(!Chat.LocalMessage.supportsRemote(remoteMessage))
        #expect(!remoteMessage.supportsNotification())
    }
}

private extension SeenReceiptCodecTests {
    func makeSeenContent() -> ChatRemoteMessageContent {
        .seen(.init(messageId: UUID().uuidString, timestamp: 1_752_900_000))
    }

    func makeRemoteMessage(content: ChatRemoteMessageContent) -> Chat.RemoteMessage {
        Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: 1_752_900_000,
            versioned: .v1(.init(content: content))
        )
    }
}
