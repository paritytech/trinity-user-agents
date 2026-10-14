import Foundation
import Testing
import SubstrateSdk
import SubstrateSdkExt

@testable import polkadot_app

/// Cross-platform vectors for the push notification payload. The hex values are frozen and shared
/// with Android (`NotificationPayloadScaleConformanceTest`), so any
/// codec drift fails here instead of as a lost push.
struct NotificationPayloadScaleConformanceTests {
    private typealias Content = Chat.RemoteMessageContentV1.MessageContent

    // id "m1", timestamp 1, V1
    private static let header = "08" + "6d31" + "0100000000000000" + "00"
    private static let fullTextHex = header + "01" + "00" + "08" + "6869"
    private static let strippedTextHex = header + "00" + "00" + "08" + "6869"
    private static let fullPaymentHex = header + "01" + "10" + "070010a5d4e8"
        + "04" + "80" + "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    private static let strippedPaymentHex = header + "00" + "10" + "070010a5d4e8"

    private let coinKey = Data((0 ..< 32).map { UInt8($0) })
    private let totalValue = Balance(1_000_000_000_000)

    @Test func fullTextEncodesToFrozenVector() throws {
        let encoded = try encode(.full(.init(content: .text("hi"))))

        #expect(encoded == Self.fullTextHex)
    }

    @Test func strippedTextEncodesToFrozenVector() throws {
        let encoded = try encode(.stripped(.text("hi")))

        #expect(encoded == Self.strippedTextHex)
    }

    @Test func fullPaymentEncodesToFrozenVector() throws {
        let payment = Content.SendContent.Coinage(totalValue: totalValue, coinKeys: [coinKey])

        let encoded = try encode(.full(.init(content: .coinageSend(payment))))

        #expect(encoded == Self.fullPaymentHex)
    }

    @Test func strippedPaymentEncodesToFrozenVector() throws {
        let encoded = try encode(.stripped(.coinageSend(.init(totalValue: totalValue))))

        #expect(encoded == Self.strippedPaymentHex)
    }

    @Test func fullPaymentDecodesFromFrozenVector() throws {
        let payload = try decode(Self.fullPaymentHex)

        #expect(payload.messageId == "m1")
        #expect(payload.timestamp == 1)
        let expected = Content.SendContent.Coinage(totalValue: totalValue, coinKeys: [coinKey])
        #expect(payload.versioned == .v1(.full(.init(content: .coinageSend(expected)))))
    }

    @Test func strippedPaymentDecodesFromFrozenVector() throws {
        let payload = try decode(Self.strippedPaymentHex)

        #expect(payload.messageId == "m1")
        #expect(payload.timestamp == 1)
        #expect(payload.versioned == .v1(.stripped(.coinageSend(.init(totalValue: totalValue)))))
    }

    @Test func strippedTextDecodesFromFrozenVector() throws {
        let payload = try decode(Self.strippedTextHex)

        #expect(payload.versioned == .v1(.stripped(.text("hi"))))
    }

    /// The pre-envelope `RemoteMessage` layout is not accepted: every client adopts the envelope.
    @Test func legacyLayoutIsRejected() throws {
        let legacy = Chat.RemoteMessage(
            messageId: "m1",
            timestamp: 1,
            versioned: .v1(.init(content: .richText(.init(text: "hi", attachments: nil))))
        )

        #expect(throws: (any Error).self) {
            try Chat.NotificationPayload.fromScaleEncoded(legacy.scaleEncoded())
        }
    }
}

private extension NotificationPayloadScaleConformanceTests {
    func encode(_ content: Chat.NotificationContentV1) throws -> String {
        let payload = Chat.NotificationPayload(messageId: "m1", timestamp: 1, versioned: .v1(content))

        return try payload.scaleEncoded().toHex()
    }

    func decode(_ hex: String) throws -> Chat.NotificationPayload {
        try Chat.NotificationPayload.fromScaleEncoded(Data(hexString: hex))
    }
}
