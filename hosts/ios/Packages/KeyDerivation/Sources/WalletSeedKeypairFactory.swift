import Foundation
import SubstrateSdk
import NovaCrypto

public enum WalletSeedKeypairFactoryError: Error {
    case invalidSeedLength(Int)
}

public final class WalletSeedKeypairFactory: @unchecked Sendable {
    public static let expectedSeedLength = 32

    private let seed: Data
    private let chaincodeList: [Chaincode]

    private lazy var keypairFactory: KeypairFactoryProtocol = SR25519KeypairFactory()
    private var publicKey: IRPublicKeyProtocol?

    private let mutex = NSLock()

    public init(seed: Data, chaincodeList: [Chaincode] = []) throws {
        guard seed.count == Self.expectedSeedLength else {
            throw WalletSeedKeypairFactoryError.invalidSeedLength(seed.count)
        }
        self.seed = seed
        self.chaincodeList = chaincodeList
    }
}

extension WalletSeedKeypairFactory: WalletKeypairFactoryProtocol {
    public func deriveKeypair() throws -> IRCryptoKeypairProtocol {
        mutex.lock()
        defer { mutex.unlock() }

        let keypair = try keypairFactory.createKeypairFromSeed(
            seed,
            chaincodeList: chaincodeList
        )

        publicKey = keypair.publicKey()

        return keypair
    }

    public func derivePublicKey() throws -> IRPublicKeyProtocol {
        if let publicKey { return publicKey }

        return try deriveKeypair().publicKey()
    }
}

extension WalletSeedKeypairFactory: SigningSecretProviding {
    public func fetchSignerSecret(for _: SignerProviding) throws -> Data {
        try deriveKeypair().privateKey().rawData()
    }
}
