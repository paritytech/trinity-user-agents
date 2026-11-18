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
/// `sent` and `claimed` are best-head readings; `sentFinalized` means every coin reached the finalized chain
/// and only `claimedFinalized` and `failed` are terminal.
struct OutgoingTransferState: Equatable, Sendable {
    enum Status: Int16, Sendable {
        case sending = 0
        case sent = 1
        case claimed = 2
        case failed = 3
        case sentFinalized = 4
        case claimedFinalized = 5

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
