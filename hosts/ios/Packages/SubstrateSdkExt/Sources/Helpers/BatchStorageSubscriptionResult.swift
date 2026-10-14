import Foundation
import SubstrateSdk
import SubstrateStorageSubscription

public struct BatchStorageSubscriptionSingleResult<T: Decodable>: BatchStorageSubscriptionResult {
    public let value: T
    public let blockHashJson: JSON

    public init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        value = try values[0].value.map(to: T.self, with: context)
        self.blockHashJson = blockHashJson
    }
}
