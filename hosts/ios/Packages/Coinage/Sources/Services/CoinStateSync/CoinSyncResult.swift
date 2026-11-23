import Foundation
import SubstrateSdk
import SubstrateStorageSubscription

struct CoinSyncResult: BatchStorageSubscriptionResult {
    let updates: [String: OnChainCoin?]

    init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson _: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        var parsedUpdates: [String: OnChainCoin?] = [:]

        for value in values {
            guard let mappingKey = value.mappingKey else { continue }

            let coin = try value.value.map(to: OnChainCoin?.self, with: context)
            parsedUpdates[mappingKey] = coin
        }

        updates = parsedUpdates
    }
}

extension CoinSyncResult {
    /// Representation of the Coin struct stored on-chain in CoinagePallet.
    struct OnChainCoin: Codable, Equatable {
        @StringCodable var instanceId: UInt32
        @StringCodable var value: Int8
        @StringCodable var age: Int16
    }
}

/// What `CoinsByOwner` holds for one key, judged against the app's Coinage instance.
///
/// `CoinsByOwner` is keyed by public key alone, so a key can hold a foreign coin: one of another
/// Coinage instance (another unit or asset).
enum OnChainCoinRead: Equatable {
    case absent
    case present(CoinSyncResult.OnChainCoin)
    case foreign(CoinageInstanceId)

    /// The coin as the app's own Coinage instance sees it: a foreign coin reads as absent.
    var coin: CoinSyncResult.OnChainCoin? {
        if case let .present(coin) = self { return coin }
        return nil
    }
}

extension CoinSyncResult.OnChainCoin? {
    /// The only place a coin's Coinage instance is compared with the app's.
    func read(in instanceId: CoinageInstanceId) -> OnChainCoinRead {
        guard let coin = self else { return .absent }
        return coin.instanceId == instanceId ? .present(coin) : .foreign(coin.instanceId)
    }
}

extension CoinSyncResult {
    func reads(in instanceId: CoinageInstanceId) -> [String: OnChainCoinRead] {
        updates.mapValues { $0.read(in: instanceId) }
    }
}
