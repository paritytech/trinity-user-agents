import Foundation
import SubstrateSdk

public enum CoinageTransferStatus: Equatable, Sendable {
    /// Present on chain: the peer has not taken it yet. `finalized` once the finalized chain holds it too.
    case awaitingClaim(finalized: Bool)

    /// Absent, and its minter has not resolved: it may simply not exist yet.
    case detecting

    /// Absent because it was never minted. The key controls nothing, and nothing can change that.
    case failed

    /// Absent after having existed, so the peer took it.
    ///
    /// `finalized` separates a guess from a proof: true only when the mint finalized *and* the coin
    /// is absent from the finalized chain. Until then it is inferred from the best head, where a fork
    /// can put the coin back.
    case claimed(finalized: Bool)

    /// Our own reclaim of the handed-off coin is in flight.
    case reclaiming

    /// Absent after our reclaim landed. `finalized` as for ``claimed(finalized:)``.
    case reclaimed(finalized: Bool)

    /// Still on chain, and our last reclaim of it failed with no later attempt.
    case reclaimFailed
}

public extension CoinageTransferStatus {
    var isTerminal: Bool {
        switch self {
        case .failed: true
        case let .claimed(finalized): finalized
        case let .reclaimed(finalized): finalized
        case .awaitingClaim,
             .detecting,
             .reclaiming,
             .reclaimFailed: false
        }
    }
}

public struct CoinageTransferState: Equatable, Sendable {
    public let coin: Coin
    public let status: CoinageTransferStatus

    public init(coin: Coin, status: CoinageTransferStatus) {
        self.coin = coin
        self.status = status
    }
}
