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

    @Test("lite chat key is used even when the full entry carries one")
    func liteChatKeyUsedOverFull() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: BytesCodable(wrappedValue: Self.liteChatKey)),
            full: .init(label: Self.fullLabel, chat: BytesCodable(wrappedValue: Self.fullChatKey))
        )

        #expect(record.chatKey == Self.liteChatKey)
    }

    @Test("chat key is nil when only the full entry carries one")
    func chatKeyNilWhenOnlyFullCarriesOne() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: .init(label: Self.fullLabel, chat: BytesCodable(wrappedValue: Self.fullChatKey))
        )

        #expect(record.chatKey == nil)
    }

    @Test("chat key is nil when neither entry carries one")
    func chatKeyNilWhenBothAbsent() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: .init(label: Self.fullLabel, chat: nil)
        )

        #expect(record.chatKey == nil)
    }

    @Test("a full name the backend holds lists an account whose cache holds only its lite name")
    func backendFullNameListedOverCachedLiteName() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Data("thegeorge.06".utf8), chat: nil),
            full: nil
        )

        let listed = record.listedLabel(among: [Data("therealgeorge".utf8)], assigned: [Data("therealgeorge".utf8)])

        #expect(listed == Data("therealgeorge".utf8))
    }

    @Test("a cached lite name is listed when the backend holds it")
    func cachedLiteNameListed() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: nil
        )

        #expect(record.listedLabel(among: [Self.liteLabel], assigned: [Self.liteLabel]) == Self.liteLabel)
    }

    @Test("a cached full name is listed when the backend holds either cached name")
    func cachedFullNameListed() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: .init(label: Self.fullLabel, chat: nil)
        )

        #expect(record.listedLabel(among: [Self.liteLabel], assigned: []) == Self.fullLabel)
        #expect(record.listedLabel(among: [Self.fullLabel], assigned: [Self.fullLabel]) == Self.fullLabel)
        #expect(record.listedLabel(among: [Data("othername".utf8)], assigned: [Data("othername".utf8)]) == nil)
    }

    @Test("a backend name the cache does not hold is not listed")
    func unheldBackendNameNotListed() {
        let liteOnly = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: nil
        )
        let empty = DotnsGatewayPallet.AccountNameRecord(lite: nil, full: nil)
        let otherLite: Set<Data> = [Data("other.01".utf8)]
        let malformed: Set<Data> = [Data("Name".utf8), Data("name-x".utf8)]
        let fullName: Set<Data> = [Data("therealgeorge".utf8)]

        #expect(liteOnly.listedLabel(among: otherLite, assigned: otherLite) == nil)
        #expect(liteOnly.listedLabel(among: malformed, assigned: malformed) == nil)
        #expect(empty.listedLabel(among: fullName, assigned: fullName) == nil)
    }

    @Test("a reserved full name is not listed until it is assigned")
    func reservedFullNameNotListed() {
        let liteOnly = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel, chat: nil),
            full: nil
        )
        let reserved = Data("newname".utf8)

        #expect(liteOnly.listedLabel(among: [reserved], assigned: []) == nil)
        #expect(liteOnly.listedLabel(among: [reserved, Self.liteLabel], assigned: [Self.liteLabel]) == Self.liteLabel)
    }
}
