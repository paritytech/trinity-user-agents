import Foundation
import KeyDerivation

/// Ensures a product has a valid bulletin slot key with enough on-chain
/// capacity for the given data. Returns a wallet suitable for signing
/// the `transactionStorage.store` extrinsic on the Bulletin chain.
public protocol PreimageSubmitSponsoring {
    func sponsor(productId: ProductId, data: Data) async throws -> any WalletManaging
}
