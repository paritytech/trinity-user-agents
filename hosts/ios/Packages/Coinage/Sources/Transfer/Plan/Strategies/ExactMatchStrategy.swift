import Foundation

/// Strategy 1: exact match. The coins are handed to the recipient as they are.
///
/// No extrinsic and therefore no entry: nothing is consumed or minted on chain. What the
/// wallet records is a handoff mark per coin, which is exactly what Appendix A reads to report
/// whether the payment was claimed.
struct ExactMatchStrategy: TransferStrategy {
    private let coins: [Coin]
    private let durability: any CoinageTxServicing

    init(coins: [Coin], durability: any CoinageTxServicing) {
        self.coins = coins
        self.durability = durability
    }

    func prepare(native: NativeTransferRequest?) async throws -> PreparedStrategy {
        guard !coins.isEmpty else {
            throw TransferStrategyError.emptyCoins
        }

        let memoEntries = coins.map {
            PlannedMemoEntry(
                coinDerivationIndex: $0.derivationIndex,
                valueExponent: $0.exponent
            )
        }

        // No entry backs these coins, so the provisional handoff mark (or, natively, the retained
        // custody) is the only thing keeping them out of a concurrent selection until the memo is durable.
        return try await durability.prepareTransfer(
            handingOff: coins,
            memoEntries: memoEntries,
            transactions: [],
            native: native
        )
    }
}
