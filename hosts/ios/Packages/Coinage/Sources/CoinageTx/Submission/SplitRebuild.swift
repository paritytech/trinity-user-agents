import AsyncExtensions
import DurableTransactions
import ExtrinsicService
import Foundation

/// A payment's `Coinage.split`, read back from the ledger: the coin it spends and the coins it mints.
///
/// A rebuild therefore mints exactly the coins whose keys the recipient already holds — which is the
/// whole point of keeping the row rather than planning the payment again.
struct SplitRebuild: CoinageRebuild {
    struct Split: Sendable {
        let coinToSplit: Coin
        let outputs: [Coin]
    }

    let coinService: any CoinServiceProtocol
    let coinQuery: any CoinOnChainQuerying
    let builder: SplitExtrinsicBuilder

    func terms(of params: Data) -> RebuildTerms? {
        guard let transfer = try? CoinageSubmissionParams.decodeTransfer(params) else { return nil }

        return RebuildTerms(deadline: transfer.buildUntil, retriesFailures: transfer.retryFailures)
    }

    func resolve(
        _ transactions: [ScheduledDurableTx],
        assets: [CoinageTxId: CoinageTxEntry]
    ) async throws -> [CoinageTxId: Split] {
        let keys = assets.values.reduce(into: Set<PublicKey>()) { keys, entry in
            keys.formUnion(entry.inputs.map(\.publicKey))
            keys.formUnion(entry.outputs.map(\.publicKey))
        }

        let coins = try await coinService.fetchCoins(publicKeys: keys)

        let byKey = coins.reduce(into: [PublicKey: Coin]()) { $0[$1.publicKey] = $1 }

        return transactions.reduce(into: [:]) { resolved, transaction in
            guard let entry = assets[transaction.id], let split = split(of: entry, coins: byKey) else {
                return
            }

            resolved[transaction.id] = split
        }
    }

    func inputs(of transaction: Split) -> Set<PublicKey> {
        [transaction.coinToSplit.publicKey]
    }

    func presence(of inputs: Set<PublicKey>) async throws -> AnyAsyncSequence<Set<PublicKey>> {
        coinPresence(of: inputs, reading: coinQuery)
    }

    func build(_ transactions: [Split]) async throws -> [ExtrinsicBuiltModel] {
        try await builder.build(transactions.map { ($0.coinToSplit, $0.outputs) })
    }
}

private extension SplitRebuild {
    /// One coin in and every recorded output resolved, or nothing: a partial split would mint
    /// something other than what the recipient was promised.
    func split(of entry: CoinageTxEntry, coins: [PublicKey: Coin]) -> Split? {
        guard entry.inputs.count == 1,
              let input = entry.inputs.first.flatMap({ coins[$0.publicKey] })
        else {
            return nil
        }

        var outputs: [Coin] = []

        for output in entry.outputs {
            guard output.isCoin, let coin = coins[output.publicKey] else { return nil }

            outputs.append(coin)
        }

        guard !outputs.isEmpty else { return nil }

        return Split(coinToSplit: input, outputs: outputs)
    }
}
