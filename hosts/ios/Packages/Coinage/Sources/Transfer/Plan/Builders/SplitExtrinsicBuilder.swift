import DurableTransactions
import ExtrinsicService
import Foundation
import KeyDerivation
import SDKLogger
import SubstrateSdk

/// Declares a payment's `Coinage.split`: the coin it spends and the coins it mints.
///
/// Shared by the send path, which declares the split once, and by ``SplitRebuild``, which declares the
/// same split again from what the ledger recorded — so a rebuild mints exactly the coins whose keys the
/// recipient already holds.
struct SplitExtrinsicBuilder: Sendable {
    let coinKeyFactory: any CoinKeyDeriving
    /// The origin factory is a shared, stateless service, but its protocol cannot carry `Sendable`: the
    /// app's conformer inherits from a base class, which Swift forbids a `Sendable` class from doing. The
    /// reference is only read here, so it is vouched for at the property rather than for the whole type.
    nonisolated(unsafe) let originFactory: OriginCreating
    let factory: any DurableTxMaking
    let chainId: ChainId

    func request(coinToSplit: Coin, outputs: [Coin]) throws -> DurableTxRequest {
        let call = CoinagePallet.Calls.Split(
            splitInto: Self.destinations(of: outputs).sorted { $0.exponent < $1.exponent }
        )

        return try DurableTxRequest(
            builder: { try $0.adding(call: call.callAsFunction()) },
            origin: origin(spending: coinToSplit)
        )
    }

    func build(_ splits: [(coinToSplit: Coin, outputs: [Coin])]) async throws -> [ExtrinsicBuiltModel] {
        try await factory.makeExtrinsics(
            splits.map { try request(coinToSplit: $0.coinToSplit, outputs: $0.outputs) },
            chainId: chainId
        )
    }
}

private extension SplitExtrinsicBuilder {
    /// One destination per denomination, carrying every account minted at it.
    static func destinations(of coins: [Coin]) -> [CoinagePallet.Calls.Split.SplitDestination] {
        var grouped: [Int16: [Data]] = [:]

        for coin in coins {
            grouped[coin.exponent, default: []].append(coin.publicKey)
        }

        return grouped.map {
            CoinagePallet.Calls.Split.SplitDestination(exponent: $0.key, accounts: $0.value)
        }
    }

    func origin(spending coin: Coin) throws -> any ExtrinsicOriginDefining {
        let privateKey = try coinKeyFactory.derivePrivateKey(for: coin)

        return try originFactory.createAsCoinOrigin(
            for: DynamicDerivedWallet(secretKeyProvider: { privateKey })
        )
    }
}
