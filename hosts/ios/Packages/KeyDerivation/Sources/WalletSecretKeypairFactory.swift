import Foundation
import SubstrateSdk
import NovaCrypto

public final class WalletSecretKeypairFactory: @unchecked Sendable {
    private let secretProvider: @Sendable () throws -> Data
    private lazy var keypairFactory = SR25519KeypairFactory()

    public init(secretProvider: @Sendable @escaping () throws -> Data) {
        self.secretProvider = secretProvider
    }
}

extension WalletSecretKeypairFactory: WalletKeypairFactoryProtocol {
    public func deriveKeypair() throws -> IRCryptoKeypairProtocol {
        let secretKey = try secretProvider()

        let publicKey = try keypairFactory.createPublicKeyFromSecret(secretKey)

        let privateKey = try SNPrivateKey(rawData: secretKey)

        return IRCryptoKeypair(publicKey: publicKey, privateKey: privateKey)
    }

    public func derivePublicKey() throws -> IRPublicKeyProtocol {
        try deriveKeypair().publicKey()
    }
}

extension WalletSecretKeypairFactory: SigningSecretProviding {
    public func fetchSignerSecret(for _: SignerProviding) throws -> Data {
        try secretProvider()
    }
}
