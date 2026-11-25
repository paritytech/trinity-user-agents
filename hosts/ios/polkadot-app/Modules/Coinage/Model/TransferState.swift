import BigInt
import Foundation
import SubstrateSdk

/// A partial claim is `claimed` with `actualValue` short of the message total; there is no partial status.
/// `claimed` is a best-head reading a fork can still undo; only `claimedFinalized` and `failed` are terminal.
struct IncomingTransferState: Equatable, Sendable {
    enum Status: Int16, Sendable {
        case detecting = 0
        case claiming = 1
        case claimed = 2
        case failed = 3
        case claimedFinalized = 4
        /// The coins are not on chain and the claim window has closed. Not terminal: the claim keeps
        /// watching, so a late mint still turns it into a claim.
        case returnedToSender = 5

        static let terminal: [Status] = [.claimedFinalized, .failed]
        static var terminalRawValues: [Int16] { terminal.map(\.rawValue) }

        var isTerminal: Bool { Self.terminal.contains(self) }
    }

    let status: Status
    let actualValue: Balance?

    init(status: Status, actualValue: Balance? = nil) {
        self.status = status
        self.actualValue = actualValue
    }

    var isTerminal: Bool { status.isTerminal }
}

/// A partial claim is `claimed` with `actualValue` short of the message total; there is no partial status.
/// `sent`, `claimed` and `reclaimed` are best-head readings; `sentFinalized` means every coin reached the
/// finalized chain and only `claimedFinalized`, `failed` and `reclaimedFinalized` are terminal.
struct OutgoingTransferState: Equatable, Sendable {
    enum Status: Int16, Sendable {
        case sending = 0
        case sent = 1
        case claimed = 2
        case failed = 3
        case sentFinalized = 4
        case claimedFinalized = 5
        case unclaimed = 6
        /// Our reclaim of the unclaimed coins is in flight. Not terminal: it may still fail or be reclaimed.
        case reclaiming = 7
        /// The reclaim landed on the best head. `actualValue` is the reclaimed amount, which may be short of
        /// the message total.
        case reclaimed = 8
        /// The reclaim failed and the sender may try again.
        case reclaimFailed = 9
        case reclaimedFinalized = 10

        static let terminal: [Status] = [.claimedFinalized, .failed, .reclaimedFinalized]
        static var terminalRawValues: [Int16] { terminal.map(\.rawValue) }

        var isTerminal: Bool { Self.terminal.contains(self) }

        var isReclaimed: Bool { self == .reclaimed || self == .reclaimedFinalized }
    }

    let status: Status
    let actualValue: Balance?

    init(status: Status, actualValue: Balance? = nil) {
        self.status = status
        self.actualValue = actualValue
    }

    var isTerminal: Bool { status.isTerminal }
}
