import BigInt
import Coinage
import Foundation
import SubstrateSdk

extension CoinageTransferDetection {
    /// `claimingRest` is plain `claiming`: the claimed-so-far amount is deliberately not persisted.
    /// `claimedPartially` is a verdict reached only after every claim entry settled, so it is final.
    func incomingState(windowClosed: Bool) -> IncomingTransferState {
        switch self {
        case .detecting:
            IncomingTransferState(status: .detecting)
        case .noCoinsOnChain:
            IncomingTransferState(status: windowClosed ? .returnedToSender : .detecting)
        case .claiming,
             .claimingRest:
            IncomingTransferState(status: .claiming)
        case let .claimed(amount, finalized):
            IncomingTransferState(status: finalized ? .claimedFinalized : .claimed, actualValue: amount)
        case let .claimedPartially(claimed):
            IncomingTransferState(status: .claimedFinalized, actualValue: claimed)
        case .notClaimed:
            IncomingTransferState(status: windowClosed ? .returnedToSender : .failed)
        }
    }
}

extension [PublicKey: CoinageTransferState] {
    /// a finalized variant needs every coin's reading
    /// to come from the finalized chain.
    func outgoingState(context: DenominationBreakdownContext, windowClosed: Bool) -> OutgoingTransferState {
        let states = Array(values)
        guard !states.isEmpty else { return OutgoingTransferState(status: .sending) }

        let claimed = states.filter { if case .claimed = $0.status { true } else { false } }
        let awaiting = states.filter { if case .awaitingClaim = $0.status { true } else { false } }
        let outstanding = states.filter(\.status.isOutstanding)
        let reclaiming = states.filter { $0.status == .reclaiming }
        let reclaimed = states.filter(\.status.isReclaimed)
        let reclaimFailed = states.filter { $0.status == .reclaimFailed }

        if !reclaiming.isEmpty {
            return OutgoingTransferState(status: .reclaiming)
        }
        if !reclaimFailed.isEmpty {
            return OutgoingTransferState(status: .reclaimFailed)
        }
        if windowClosed, !awaiting.isEmpty {
            return OutgoingTransferState(status: .unclaimed)
        }
        if !reclaimed.isEmpty {
            return reclaimed.reclaimedState(context: context)
        }
        if !outstanding.isEmpty {
            guard !awaiting.isEmpty || !claimed.isEmpty else { return OutgoingTransferState(status: .sending) }
            let everyReadingFinal = states.allSatisfy(\.status.isFinalizedReading)
            return OutgoingTransferState(status: everyReadingFinal ? .sentFinalized : .sent)
        }
        guard !claimed.isEmpty else { return OutgoingTransferState(status: .failed) }

        let amount = claimed.reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.coin.exponent) }
        return OutgoingTransferState(status: isSettled ? .claimedFinalized : .claimed, actualValue: amount)
    }

    /// Settled once every coin is terminal; a reclaim that is not final keeps the payment open.
    var isSettled: Bool {
        !isEmpty && values.allSatisfy(\.status.isTerminal)
    }
}

private extension [CoinageTransferState] {
    func reclaimedState(context: DenominationBreakdownContext) -> OutgoingTransferState {
        let amount = reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.coin.exponent) }
        let everyReclaimFinal = allSatisfy { $0.status == .reclaimed(finalized: true) }
        return OutgoingTransferState(status: everyReclaimFinal ? .reclaimedFinalized : .reclaimed, actualValue: amount)
    }
}

private extension CoinageTransferStatus {
    var isReclaimed: Bool {
        if case .reclaimed = self { true } else { false }
    }

    var isOutstanding: Bool {
        switch self {
        case .awaitingClaim,
             .detecting,
             .reclaimFailed: true
        case .claimed,
             .failed,
             .reclaiming,
             .reclaimed: false
        }
    }

    /// A reading a fork can no longer undo.
    var isFinalizedReading: Bool {
        switch self {
        case .failed: true
        case let .awaitingClaim(finalized),
             let .claimed(finalized),
             let .reclaimed(finalized): finalized
        case .detecting,
             .reclaiming,
             .reclaimFailed: false
        }
    }
}
