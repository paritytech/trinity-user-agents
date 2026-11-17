import BigInt
import Foundation

/// What became of one coin of a payment the user sent.
public enum CoinageTransferStatus: Equatable, Sendable {
    /// On chain: the payee has not taken it yet.
    case awaitingClaim

    /// Not on chain yet, or being taken back: it may still change.
    case detecting

    /// Never minted, or taken back: the payee will never get it.
    case failed

    /// The payee took it. `finalized` is true once the move is stable.
    case claimed(finalized: Bool)
}

public extension CoinageTransferStatus {
    var isTerminal: Bool {
        switch self {
        case .failed: true
        case let .claimed(finalized): finalized
        case .awaitingClaim,
             .detecting: false
        }
    }
}

/// One coin of a sent payment, with its value in planks.
public struct CoinagePaymentState: Equatable, Sendable {
    public let account: Data
    public let value: BigUInt
    public let status: CoinageTransferStatus

    public init(account: Data, value: BigUInt, status: CoinageTransferStatus) {
        self.account = account
        self.value = value
        self.status = status
    }
}
