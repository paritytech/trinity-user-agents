import Foundation
import Testing
@testable import Individuality

struct DotnsGatewayTests {
    private static let fullLabel = Data("fullname".utf8)
    private static let liteLabel = Data("lite".utf8)

    @Test("full label is preferred when both full and lite entries are present")
    func fullLabelPreferred() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel),
            full: .init(label: Self.fullLabel)
        )

        #expect(record.username == Self.fullLabel)
    }

    @Test("lite label is used when full entry is absent")
    func liteLabelUsedWhenFullAbsent() {
        let record = DotnsGatewayPallet.AccountNameRecord(
            lite: .init(label: Self.liteLabel),
            full: nil
        )

        #expect(record.username == Self.liteLabel)
    }

    @Test("username is nil when both entries are absent")
    func nilWhenBothAbsent() {
        let record = DotnsGatewayPallet.AccountNameRecord(lite: nil, full: nil)

        #expect(record.username == nil)
    }
}
