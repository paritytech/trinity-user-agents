import Foundation
import KeyDerivation
import SubstrateSdk

public protocol ProductAccountHolding: Sendable {
    func deriveAccount(_ productAccountId: ProductAccountId) throws -> AccountId

    /// Public key of `//product//{productId}` — the product subtree root.
    func deriveProductSubtreePublicKey(for productId: ProductId) throws -> Data

    func deriveStatementStoreAccount(for productId: ProductId) throws -> any WalletManaging
    func deriveBulletInAccount(for productId: ProductId) throws -> any WalletManaging

    func deriveSmartContractAccount(
        for productId: ProductId,
        derivationIndex: ProductAccountSelector
    ) throws -> any WalletManaging

    func deriveAutoSigningSecrets(for productId: ProductId) throws -> AutoSigningSecrets
}
