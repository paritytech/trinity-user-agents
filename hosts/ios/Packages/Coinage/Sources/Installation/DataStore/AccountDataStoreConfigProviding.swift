import Foundation
import Revive

/// The `AccountDataStore` contract address from remote config; `nil` until it is delivered.
public protocol AccountDataStoreConfigProviding: Sendable {
    func contractAddress() async -> EvmAddress?
}
