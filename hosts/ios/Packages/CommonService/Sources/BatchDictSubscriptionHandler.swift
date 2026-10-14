import Foundation
import SubstrateSdk
import SubstrateStorageSubscription

public struct BatchDictSubscriptionChange: BatchStorageSubscriptionResult {
    public let values: [String: JSON]
    public let blockHash: BlockHashData?
    public let context: [CodingUserInfoKey: Any]?

    public init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        self.values = values.reduce(into: [String: JSON]()) {
            if let mappingKey = $1.mappingKey {
                $0[mappingKey] = $1.value
            }
        }

        blockHash = try blockHashJson.map(to: BlockHashData?.self, with: context)
        self.context = context
    }
}

public struct BatchDictSubscriptionState: ObservableSubscriptionStateProtocol {
    public typealias TChange = BatchDictSubscriptionChange

    public let values: [String: JSON]
    public let lastBlockHash: BlockHashData?
    public let context: [CodingUserInfoKey: Any]?

    public init(
        values: [String: JSON],
        lastBlockHash: BlockHashData?,
        context: [CodingUserInfoKey: Any]?
    ) {
        self.values = values
        self.lastBlockHash = lastBlockHash
        self.context = context
    }

    public init(change: BatchDictSubscriptionChange) {
        values = change.values
        lastBlockHash = change.blockHash
        context = change.context
    }

    public func merging(change: BatchDictSubscriptionChange) -> Self {
        let newValues = values.keys.reduce(
            into: [String: JSON]()
        ) { accum, key in
            accum[key] = change.values[key] ?? values[key]
        }

        return .init(
            values: newValues,
            lastBlockHash: change.blockHash,
            context: context
        )
    }

    public func decode<T: Decodable>(for key: String) throws -> T? {
        guard let json = values[key] else {
            return nil
        }

        return try json.map(to: T?.self, with: context)
    }
}
