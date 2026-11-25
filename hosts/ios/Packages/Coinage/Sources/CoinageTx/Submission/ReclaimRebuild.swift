import AsyncExtensions
import DurableTransactions
import ExtrinsicService
import Foundation
import KeyDerivation

/// A reclaim of one of our own handed-off coins, rebuilt into the fresh coin its first attempt recorded.
///
/// Signed with the coin's own derived key, which is why the rebuild reads the coin back from the ledger
/// rather than trusting the payment memo.
struct ReclaimRebuild: CoinageRebuild {
    struct Reclaim: Sendable {
        let coin: Coin
        let destination: PublicKey
    }

    let coinService: any CoinServiceProtocol
    let coinQuery: any CoinOnChainQuerying
    let coinKeyFactory: any CoinKeyDeriving
    let builder: ClaimExtrinsicBuilder

    func terms(of params: Data) -> RebuildTerms? {
        guard let transfer = try? CoinageSubmissionParams.decodeTransfer(params) else { return nil }

        return RebuildTerms(deadline: transfer.buildUntil, retriesFailures: transfer.retryFailures)
    }

    func resolve(
        _ transactions: [ScheduledDurableTx],
        assets: [CoinageTxId: CoinageTxEntry]
    ) async throws -> [CoinageTxId: Reclaim] {
        let keys = transactions.reduce(into: Set<PublicKey>()) { keys, transaction in
            keys.formUnion(assets[transaction.id]?.inputs.map(\.publicKey) ?? [])
        }

        let coins = try await coinService.fetchCoins(publicKeys: keys)

        let byKey = coins.reduce(into: [PublicKey: Coin]()) { $0[$1.publicKey] = $1 }

        return transactions.reduce(into: [:]) { resolved, transaction in
            guard let entry = assets[transaction.id], let reclaimed = reclaim(of: entry, coins: byKey) else {
                return
            }

            resolved[transaction.id] = reclaimed
        }
    }

    func inputs(of transaction: Reclaim) -> Set<PublicKey> {
        [transaction.coin.publicKey]
    }

    func presence(of inputs: Set<PublicKey>) async throws -> AnyAsyncSequence<Set<PublicKey>> {
        coinPresence(of: inputs, reading: coinQuery)
    }

    func build(_ transactions: [Reclaim]) async throws -> [ExtrinsicBuiltModel] {
        let claims = try transactions.map { reclaim in
            try (coinKey: coinKeyFactory.derivePrivateKey(for: reclaim.coin), destination: reclaim.destination)
        }

        return try await builder.build(claims)
    }
}

private extension ReclaimRebuild {
    /// One reclaimed coin in and one coin out, or nothing: a reclaim with any other shape is not one this
    /// rebuild can sign.
    func reclaim(of entry: CoinageTxEntry, coins: [PublicKey: Coin]) -> Reclaim? {
        guard entry.inputs.count == 1,
              let input = entry.inputs.first,
              input.isReclaimed,
              let coin = coins[input.publicKey],
              entry.outputs.count == 1,
              let output = entry.outputs.first,
              output.isCoin
        else {
            return nil
        }

        return Reclaim(coin: coin, destination: output.publicKey)
    }
}
