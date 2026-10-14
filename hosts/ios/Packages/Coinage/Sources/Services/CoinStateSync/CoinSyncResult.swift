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
    struct OnChainCoin: Codable {
        @StringCodable var instanceId: UInt32
        @StringCodable var value: Int8
        @StringCodable var age: Int16
    }
}
