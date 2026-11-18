import BigInt
import Foundation
import TrUAPIHost

extension Balance {
    func coinageBalance(units: CoinageUnits, strategy: RecyclingStrategyType) -> CoinageBalance {
        CoinageBalance(
            availablePrivate: units.planks(ofCents: spendable),
            gainingPrivacy: CoinageBalance.GainingPrivacy(
                amount: units.planks(ofCents: heldBack),
                canSpendWithConfirmation: strategy.allowsConfirmedSpend
            ),
            pending: units.planks(ofCents: arriving)
        )
    }
}

extension Holdings {
    var coinageHoldings: CoinageHoldings {
        CoinageHoldings(rows: rows.compactMap(\.coinageHolding))
    }
}

extension Holding {
    /// A peer's coin is not ours until a claim moves it into one of our own, so it has no row.
    var coinageHolding: CoinageHolding? {
        let kind: CoinageHolding.Kind
        let installation: Data
        let index: UInt32

        switch asset {
        case let .coin(_, coinInstallation, coinIndex):
            (kind, installation, index) = (.coin, coinInstallation, coinIndex)
        case let .entry(_, entryInstallation, entryIndex):
            (kind, installation, index) = (.voucher, entryInstallation, entryIndex)
        case .peerCoin:
            return nil
        }

        return CoinageHolding(
            kind: kind,
            installation: installation,
            index: index,
            exponent: Int16(exponent),
            availability: ready ? .availableNow : .notReady,
            hops: batchUnloaded ? 0 : Int(age ?? 0),
            isBatchUnloaded: batchUnloaded,
            fungibility: fungibility
        )
    }
}

extension RecyclingStrategyType {
    var privacyPreset: PrivacyPreset {
        switch self {
        case .minPrivacy: .fastest
        case .balanced: .balanced
        case .maxPrivacy: .mostPrivate
        }
    }
}

extension Status {
    var isTerminal: Bool {
        switch self {
        case .done,
             .partiallyDone,
             .failed: true
        case .preparing,
             .waiting,
             .submitted,
             .settling: false
        }
    }
}

extension StatusItem {
    /// How a claim or top-up the app follows stands, as its screens show it.
    func transferDetection(units: CoinageUnits) -> CoinageTransferDetection {
        let settledValue = units.planks(ofCents: progress.settled)
        let nothingYet = progress.settled == 0 && progress.inFlight == 0

        switch status {
        case .done:
            return .claimed(amount: settledValue, finalized: true)
        case .partiallyDone:
            return .claimedPartially(claimed: settledValue)
        case .failed:
            return .notClaimed
        case .settling where progress.inFlight == progress.provisional:
            return .claimed(amount: units.planks(ofCents: progress.settled + progress.provisional), finalized: false)
        case .waiting where nothingYet:
            return .detecting
        default:
            return .claiming
        }
    }

    func externalPaymentStatus(units: CoinageUnits) -> ExternalPaymentStatus {
        switch status {
        case .done:
            .completed
        case .partiallyDone:
            .partiallyCompleted(settledInPlanks: units.planks(ofCents: progress.settled))
        case let .failed(failure):
            .failed(reason: String(describing: failure))
        case .preparing,
             .waiting,
             .submitted,
             .settling:
            .processing
        }
    }

    func incomingPaymentStatus(units: CoinageUnits) -> IncomingPaymentStatus {
        switch transferDetection(units: units) {
        case .detecting:
            .detecting
        case .claiming,
             .claimingRest:
            .claiming
        case let .claimed(_, finalized):
            .claimed(finalized: finalized)
        case let .claimedPartially(claimed):
            .claimedPartially(actualClaimed: claimed)
        case .notClaimed:
            .notClaimed
        }
    }
}

extension ExportedCoinStatus {
    func paymentState(units: CoinageUnits) -> CoinagePaymentState {
        let transferStatus: CoinageTransferStatus =
            switch status {
            case .notYetMinted: .detecting
            case .awaitingPayee: .awaitingClaim
            case let .claimed(stable, _): .claimed(finalized: stable)
            case let .reclaimed(stable): stable ? .failed : .detecting
            case .neverMinted: .failed
            }

        return CoinagePaymentState(
            account: account,
            value: units.planks(ofExponent: Int16(exponent)),
            status: transferStatus
        )
    }
}
