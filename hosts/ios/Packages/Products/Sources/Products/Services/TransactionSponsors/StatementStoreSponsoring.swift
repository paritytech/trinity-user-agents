import Foundation
import KeyDerivation

/// Ensures a product has a valid statement store slot key.
/// Returns a wallet suitable for creating a `StatementStoreSigning` signer.
public protocol StatementStoreSponsoring {
    func sponsor(productId: ProductId) async throws -> any WalletManaging
}
