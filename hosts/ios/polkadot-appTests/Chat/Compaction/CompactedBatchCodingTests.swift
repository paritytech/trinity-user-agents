@testable import polkadot_app
import XCTest
import SubstrateSdk

/// Pins the RFC-0002 compacted batch wire format: the plaintext is the bare
/// SCALE-encoded `[EncodedMessage]` list with no compaction-specific container
/// or version prefix — versioning lives in the pool envelope and in each
/// message's own encoding.
final class CompactedBatchCodingTests: XCTestCase {
    func testBatchRoundTrip() throws {
        let batch = [
            makeOpaqueMessage(content: .text("first")),
            makeOpaqueMessage(content: .text("second"))
        ]

        let encoded = try batch.scaleEncoded()
        let decoded = try [Chat.OpaqueMessage].fromScaleEncoded(encoded)

        XCTAssertEqual(decoded, batch)
    }

    func testBatchHasNoVersionPrefix() throws {
        let batch = [makeOpaqueMessage(content: .text("solo"))]

        let encoded = try batch.scaleEncoded()

        // First byte is the compact-encoded element count, not a version index
        XCTAssertEqual(encoded.first, UInt8(1 << 2))

        // Each element is the opaque per-message encoding: stripping the list
        // prefix must yield exactly the standalone message encoding
        let standalone = try batch[0].scaleEncoded()
        XCTAssertEqual(encoded.dropFirst(), standalone)
    }

    func testNestedCompactedMessageRoundTrips() throws {
        let nestedReference = ChatRemoteMessageContent.CompactedMessagesContent(
            claimIdentifier: Data(repeating: 7, count: 32),
            claimTicket: Data(repeating: 9, count: 32),
            node: .wssUrl("wss://node.example.com")
        )

        let batch = [
            makeOpaqueMessage(content: .compactedMessages(nestedReference)),
            makeOpaqueMessage(content: .text("newer"))
        ]

        let encoded = try batch.scaleEncoded()
        let decoded = try [Chat.OpaqueMessage].fromScaleEncoded(encoded)

        XCTAssertEqual(decoded, batch)
        XCTAssertEqual(
            decoded.first?.remoteMessage.versioned.ensureV1()?.content,
            .compactedMessages(nestedReference)
        )
    }
}

private extension CompactedBatchCodingTests {
    func makeOpaqueMessage(
        content: Chat.RemoteMessageContentV1.MessageContent
    ) -> Chat.OpaqueMessage {
        let remoteMessage = Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: 1_752_900_000,
            versioned: .v1(.init(content: content))
        )

        return Chat.OpaqueMessage(remoteMessage: remoteMessage)
    }
}
