import Foundation
import KeyDerivation
import SubstrateSdk

public final class ProductAccountHolder: @unchecked Sendable {
    private let entropyManager: RootEntropyManaging

    public init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }
}

extension ProductAccountHolder: ProductAccountHolding {
    public func deriveAccount(_ productAccountId: ProductAccountId) throws -> AccountId {
        let wallet = try DynamicDerivedWallet(
            derivationPath: productAccountId.derivationPath(),
            entropyManager: entropyManager
        )
        return try wallet.getRawPublicKey()
    }

    public func deriveProductSubtreePublicKey(for productId: ProductId) throws -> Data {
        let path = try ProductDerivationPath.productRoot(productId: productId)
        let wallet = DynamicDerivedWallet(derivationPath: path, entropyManager: entropyManager)
        return try wallet.getRawPublicKey()
    }

    public func deriveStatementStoreAccount(for productId: ProductId) throws -> any WalletManaging {
        let path = Self.allowancePath(system: "statement-store", productId: productId)
        return DynamicDerivedWallet(derivationPath: path, entropyManager: entropyManager)
    }

    public func deriveStatementStorePrivateKey(for productId: ProductId) throws -> Data {
        let path = Self.allowancePath(system: "statement-store", productId: productId)
        return try derivePrivateKey(at: path)
    }

    public func deriveBulletInAccount(for productId: ProductId) throws -> any WalletManaging {
        let path = Self.allowancePath(system: "bulletin", productId: productId)
        return DynamicDerivedWallet(derivationPath: path, entropyManager: entropyManager)
    }

    public func deriveBulletInPrivateKey(for productId: ProductId) throws -> Data {
        let path = Self.allowancePath(system: "bulletin", productId: productId)
        return try derivePrivateKey(at: path)
    }

    public func deriveSmartContractAccount(
        for productId: ProductId,
        derivationIndex: ProductAccountSelector
    ) throws -> any WalletManaging {
        let accountId = ProductAccountId(productId: productId, derivationIndex: derivationIndex)
        return try DynamicDerivedWallet(
            derivationPath: accountId.derivationPath(),
            entropyManager: entropyManager
        )
    }

    public func deriveAutoSigningSecrets(for productId: ProductId) throws -> AutoSigningSecrets {
        let path = try ProductDerivationPath.productRoot(productId: productId)
        return try AutoSigningSecrets(productRootPrivateKey: derivePrivateKey(at: path))
    }
}

private extension ProductAccountHolder {
    static func allowancePath(system: String, productId: ProductId) -> String {
        "//allowance//\(system)//\(productId)"
    }

    func derivePrivateKey(at path: String) throws -> Data {
        try WalletMnemonicKeypairFactory(derivationPath: path, entropyManager: entropyManager)
            .deriveKeypair()
            .privateKey()
            .rawData()
    }
}
