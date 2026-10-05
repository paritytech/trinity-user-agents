import Foundation
import Testing
import SubstrateSdk
@testable import Individuality

struct DotnsGatewayTests {
    private static let fullLabel = Data("fullname".utf8)
    private static let liteLabel = Data("lite".utf8)
    private static let fullChatKey = Data("fullchatkey".utf8)
    private static let liteChatKey = Data("litechatkey".utf8)

    @Test("full label is preferred when both full and lite entries are present")
    func fullLabelPreferred() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: .init(label: Self.fullLabel, chat: nil)
        )

        #expect(record.username == Self.fullLabel)
    }

    @Test("lite label is used when full entry is absent")
    func liteLabelUsedWhenFullAbsent() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: nil
        )

        #expect(record.username == Self.liteLabel)
    }

    @Test("username is nil when both entries are absent")
    func nilWhenBothAbsent() {
        let record = DotnsGatewayPallet.AccountNameRecord(lite: nil, full: nil)

        #expect(record.username == nil)
    }

    @Test("full chat key is preferred when both entries carry one")
    func fullChatKeyPreferred() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: BytesCodable(wrappedValue: Self.liteChatKey)),
            full: .init(label: Self.fullLabel, chat: BytesCodable(wrappedValue: Self.fullChatKey))
        )

        #expect(record.chatKey == Self.fullChatKey)
    }

    @Test("full chat key falls back to lite entry when full carries none")
    func fullWithoutChatFallsToLite() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: BytesCodable(wrappedValue: Self.liteChatKey)),
            full: .init(label: Self.fullLabel, chat: nil)
        )

        #expect(record.chatKey == Self.liteChatKey)
    }

    @Test("chat key is nil when neither entry carries one")
    func chatKeyNilWhenBothAbsent() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: .init(label: Self.fullLabel, chat: nil)
        )

        #expect(record.chatKey == nil)
    }
}
