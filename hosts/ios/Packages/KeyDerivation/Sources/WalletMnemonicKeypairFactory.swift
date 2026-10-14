import Foundation
import SubstrateSdk
import NovaCrypto

public final class WalletMnemonicKeypairFactory: @unchecked Sendable {
    let derivationPath: String?
    let mnemonicProvider: WalletMnemonicProviding

    private lazy var junctionFactory: JunctionFactoryProtocol = SubstrateJunctionFactory()
    private lazy var seedFactory: SeedFactoryProtocol = SeedFactory()
    private lazy var keypairFactory: KeypairFactoryProtocol = SR25519KeypairFactory()

    // cache public key to access secrets only once to get it
    private var publicKey: IRPublicKeyProtocol?
    private let mutex = NSLock()

    public init(derivationPath: String?, entropyManager: RootEntropyManaging) {
        self.derivationPath = derivationPath
        mnemonicProvider = WalletKeystoreMnemonicProvider(entropyManager: entropyManager)
    }

    public init(mnemonic: String, derivationPath: String?) {
        self.derivationPath = derivationPath
        mnemonicProvider = WalletMnemonicProvider(mnemonic: mnemonic)
    }
}

private extension WalletMnemonicKeypairFactory {
    func resolveDerivation() throws -> (chaincodes: [Chaincode], password: String) {
        guard let derivationPath else {
            return ([], "")
        }

        let junctionResult = try junctionFactory.parse(path: derivationPath)
        return (junctionResult.chaincodes, junctionResult.password ?? "")
    }

    func performKeypairDerivation() throws -> IRCryptoKeypairProtocol {
        let mnemonic = try mnemonicProvider.fetchMnemonic()
        let derivation = try resolveDerivation()

        let seedResult = try seedFactory.deriveSeed(from: mnemonic, password: derivation.password)

        let keypair = try keypairFactory.createKeypairFromSeed(
            seedResult.seed.miniSeed,
            chaincodeList: derivation.chaincodes
        )

        publicKey = keypair.publicKey()

        return keypair
    }
}

extension WalletMnemonicKeypairFactory: WalletKeypairFactoryProtocol {
    public func deriveKeypair() throws -> IRCryptoKeypairProtocol {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return try performKeypairDerivation()
    }

    public func derivePublicKey() throws -> IRPublicKeyProtocol {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let publicKey {
            return publicKey
        }

        return try performKeypairDerivation().publicKey()
    }
}

extension WalletMnemonicKeypairFactory: SigningSecretProviding {
    public func fetchSignerSecret(for _: SignerProviding) throws -> Data {
        try deriveKeypair().privateKey().rawData()
    }
}
