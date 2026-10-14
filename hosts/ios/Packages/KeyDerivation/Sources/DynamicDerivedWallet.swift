import Foundation
import SubstrateSdk
import SubstrateSdkExt
import NovaCrypto

public enum DynamicDerivedWalletError: Error {
    case unexpectedAccount
}

public struct DynamicDerivedWallet {
    private let keypairFactory: WalletKeypairFactoryProtocol

    public init(
        derivationPath: String?,
        entropyManager: RootEntropyManaging
    ) {
        keypairFactory = WalletMnemonicKeypairFactory(
            derivationPath: derivationPath,
            entropyManager: entropyManager
        )
    }

    public init(mnemonic: String, derivationPath: String? = nil) {
        keypairFactory = WalletMnemonicKeypairFactory(mnemonic: mnemonic, derivationPath: derivationPath)
    }

    public init(seedBytes: Data) throws {
        keypairFactory = try WalletSeedKeypairFactory(seed: seedBytes)
    }

    public init(secretKeyProvider: @Sendable @escaping () -> Data) {
        keypairFactory = WalletSecretKeypairFactory(secretProvider: secretKeyProvider)
    }

    public init(keypairFactory: WalletKeypairFactoryProtocol) {
        self.keypairFactory = keypairFactory
    }
}

extension DynamicDerivedWallet: WalletManaging {
    public func getRawPublicKey() throws -> Data {
        try keypairFactory.derivePublicKey().rawData()
    }

    public func fetchRawSecretKey() throws -> Data {
        let keypair = try keypairFactory.deriveKeypair()

        return keypair.privateKey().rawData()
    }

    public func fetchAccount(for chain: ChainProtocol) throws -> AccountProtocol {
        let rawPublicKey = try keypairFactory.derivePublicKey()
        let publicKey = try SNPublicKey(rawData: rawPublicKey.rawData())
        return DynamicDerivedAccount(snPublicKey: publicKey, chain: chain)
    }

    public func hasAccount(in _: ChainProtocol) -> Bool {
        true
    }

    public func fetchSignerSecret(for signer: SignerProviding) throws -> Data {
        let keypair = try keypairFactory.deriveKeypair()

        guard signer.account?.accountId == keypair.publicKey().rawData() else {
            throw DynamicDerivedWalletError.unexpectedAccount
        }

        return keypair.privateKey().rawData()
    }

    public func getMultiSigner() throws -> MultiSigner {
        let publicKey = try getRawPublicKey()
        return .sr25519(publicKey)
    }

    public func sign(data: Data) throws -> MultiSignature {
        let rawPublicKey = try getRawPublicKey()
        let publicKey = try SNPublicKey(rawData: rawPublicKey)

        let rawSignature = try DefaultSigningWrapper(
            secretProvider: self
        )
        .sign(
            data,
            context: .rawBytes(publicKey)
        )
        .rawData()

        return .sr25519(data: rawSignature)
    }
}

private struct DynamicDerivedAccount {
    let snPublicKey: SNPublicKey
    let chain: ChainProtocol
}

extension DynamicDerivedAccount: AccountProtocol {
    var accountId: AccountId { snPublicKey.accountId }
    var publicKey: Data { snPublicKey.rawData() }
    var signatureFormat: ExtrinsicSignatureFormat { snPublicKey.signatureFormat }
    var signaturePayloadFormat: ExtrinsicSignaturePayloadFormat { snPublicKey.signaturePayloadFormat }
    var signatureType: CryptoType { snPublicKey.signatureType }

    func toAddress() throws -> AccountAddress {
        try SS58AddressFactory().address(fromAccountId: accountId, type: chain.base58Prefix)
    }
}
