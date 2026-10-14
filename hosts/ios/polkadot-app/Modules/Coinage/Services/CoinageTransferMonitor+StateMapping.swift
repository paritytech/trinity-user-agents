import BigInt
import Coinage
import Foundation
import SubstrateSdk

extension CoinageTransferDetection {
    /// `claimingRest` is plain `claiming`: the claimed-so-far amount is deliberately not persisted.
    var incomingState: IncomingTransferState {
        switch self {
        case .detecting:
            IncomingTransferState(status: .detecting)
        case .claiming,
             .claimingRest:
            IncomingTransferState(status: .claiming)
        case let .claimed(amount, _):
            IncomingTransferState(status: .claimed, actualValue: amount)
        case let .claimedPartially(claimed):
            IncomingTransferState(status: .claimed, actualValue: claimed)
        case .notClaimed:
            IncomingTransferState(status: .failed)
        }
    }
}

extension [PublicKey: CoinageTransferState] {
    /// Mirrors Android's `toPaymentStatus`: any coin still to be taken keeps the message at
    /// `sending`/`sent`; once nothing is outstanding, the claimed value is final.
    func outgoingState(context: DenominationBreakdownContext) -> OutgoingTransferState {
        let states = Array(values)
        guard !states.isEmpty else { return OutgoingTransferState(status: .sending) }

        let claimed = states.filter { if case .claimed = $0.status { true } else { false } }
        let awaiting = states.filter { $0.status == .awaitingClaim }
        let outstanding = states.filter { $0.status == .awaitingClaim || $0.status == .detecting }

        if !outstanding.isEmpty {
            return OutgoingTransferState(status: awaiting.isEmpty && claimed.isEmpty ? .sending : .sent)
        }
        guard !claimed.isEmpty else { return OutgoingTransferState(status: .failed) }

        let amount = claimed.reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.coin.exponent) }
        return OutgoingTransferState(status: .claimed, actualValue: amount)
    }

    var isSettled: Bool {
        !isEmpty && values.allSatisfy(\.status.isTerminal)
    }
}
