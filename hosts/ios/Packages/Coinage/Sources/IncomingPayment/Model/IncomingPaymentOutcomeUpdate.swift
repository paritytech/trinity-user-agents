import Foundation
import Operation_iOS

/// A write-only partial update that writes the terminal `outcome` on an existing incoming-payment
/// record, leaving everything else untouched. Keyed by `groupId`, mirroring `CoinPresenceUpdate` — so
/// settling never fetch-modify-saves the whole record.
public struct IncomingPaymentOutcomeUpdate: Equatable, Sendable {
    public let groupId: CoinageTxGroupId
    public let outcome: IncomingPaymentTerminalOutcome

    public init(groupId: CoinageTxGroupId, outcome: IncomingPaymentTerminalOutcome) {
        self.groupId = groupId
        self.outcome = outcome
    }
}

extension IncomingPaymentOutcomeUpdate: Operation_iOS.Identifiable {
    public var identifier: String { groupId }
}
