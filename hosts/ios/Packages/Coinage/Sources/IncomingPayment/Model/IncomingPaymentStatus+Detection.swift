import Foundation

public extension IncomingPaymentStatus {
    /// Maps a claim's `CoinageTransferDetection` (the shared claim-progress verdict derived from the
    /// durability group) onto the host-facing top-up status. `claimingRest` is still in progress, so
    /// it reports as `.claiming` — a partial figure is only ever final via `.claimedPartially`.
    init(detection: CoinageTransferDetection) {
        switch detection {
        case .detecting:
            self = .detecting
        case .claiming,
             .claimingRest:
            self = .claiming
        case let .claimed(_, finalized):
            self = .claimed(finalized: finalized)
        case let .claimedPartially(claimed):
            self = .claimedPartially(actualClaimed: claimed)
        case .notClaimed:
            self = .notClaimed
        }
    }
}
