import BigInt
import Coinage
import Foundation
import SubstrateSdk

extension CoinageTransferDetection {
    /// `claimingRest` is plain `claiming`: the claimed-so-far amount is deliberately not persisted.
    /// `claimedPartially` is a verdict reached only after every claim entry settled, so it is final.
    var incomingState: IncomingTransferState {
        switch self {
        case .detecting:
            IncomingTransferState(status: .detecting)
        case .claiming,
             .claimingRest:
            IncomingTransferState(status: .claiming)
        case let .claimed(amount, finalized):
            IncomingTransferState(status: finalized ? .claimedFinalized : .claimed, actualValue: amount)
        case let .claimedPartially(claimed):
            IncomingTransferState(status: .claimedFinalized, actualValue: claimed)
        case .notClaimed:
            IncomingTransferState(status: .failed)
        }
    }
}

extension [PublicKey: CoinageTransferState] {
    /// Mirrors Android's `toPaymentStatus`: any coin still to be taken keeps the message at
    /// `sending`/`sent`; once nothing is outstanding, the claimed value is final. The finalized
    /// variants are reached only when every coin's reading comes from the finalized chain.
    func outgoingState(context: DenominationBreakdownContext) -> OutgoingTransferState {
        let states = Array(values)
        guard !states.isEmpty else { return OutgoingTransferState(status: .sending) }

        let claimed = states.filter { if case .claimed = $0.status { true } else { false } }
        let awaiting = states.filter { if case .awaitingClaim = $0.status { true } else { false } }
        let outstanding = states.filter(\.status.isOutstanding)

        if !outstanding.isEmpty {
            guard !awaiting.isEmpty || !claimed.isEmpty else { return OutgoingTransferState(status: .sending) }
            let everyReadingFinal = states.allSatisfy(\.status.isFinalizedReading)
            return OutgoingTransferState(status: everyReadingFinal ? .sentFinalized : .sent)
        }
        guard !claimed.isEmpty else { return OutgoingTransferState(status: .failed) }

        let amount = claimed.reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.coin.exponent) }
        return OutgoingTransferState(status: isSettled ? .claimedFinalized : .claimed, actualValue: amount)
    }

    var isSettled: Bool {
        !isEmpty && values.allSatisfy(\.status.isTerminal)
    }
}

private extension CoinageTransferStatus {
    var isOutstanding: Bool {
        switch self {
        case .awaitingClaim,
             .detecting: true
        case .claimed,
             .failed: false
        }
    }

    /// A reading a fork can no longer undo.
    var isFinalizedReading: Bool {
        switch self {
        case .failed: true
        case let .awaitingClaim(finalized),
             let .claimed(finalized): finalized
        case .detecting: false
        }
    }
}
