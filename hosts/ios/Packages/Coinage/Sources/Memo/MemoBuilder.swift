import Foundation
import BigInt
import SubstrateSdk

protocol MemoBuilding {
    /// Builds a transfer memo from planned memo entries.
    /// - Parameters:
    ///   - entries: Planned memo entries describing coins to include
    ///   - breakdownContext: Context for calculating denomination values
    /// - Returns: TransferMemo with SCALE-encoded private key data
    /// - Throws: MemoBuilderError if entries empty or derivation fails
    func buildMemo(
        from entries: [PlannedMemoEntry],
        breakdownContext: DenominationBreakdownContext
    ) throws -> TransferMemo
}

/// Builds TransferMemo from planned memo entries by deriving private keys.
final class MemoBuilder {
    private let privateKeyDeriver: any CoinKeyDeriving

    init(privateKeyDeriver: any CoinKeyDeriving) {
        self.privateKeyDeriver = privateKeyDeriver
    }
}

extension MemoBuilder: MemoBuilding {
    func buildMemo(
        from entries: [PlannedMemoEntry],
        breakdownContext: DenominationBreakdownContext
    ) throws -> TransferMemo {
        guard !entries.isEmpty else {
            throw MemoBuilderError.emptyCoins
        }

        var memoEntries: [Data] = []
        var totalValue = BigUInt(0)

        for entry in entries {
            let privateKey: Data
            do {
                privateKey = try privateKeyDeriver.derivePrivateKey(index: entry.coinDerivationIndex)
            } catch {
                throw MemoBuilderError.keyDerivationFailed(error)
            }

            memoEntries.append(privateKey)

            totalValue += breakdownContext.valueInPlanks(for: entry.valueExponent)
        }

        return TransferMemo(entries: memoEntries, totalValue: totalValue)
    }
}
