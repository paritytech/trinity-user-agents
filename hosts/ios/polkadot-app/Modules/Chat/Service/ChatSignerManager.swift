import Foundation
import StatementStore
import Keystore_iOS
import NovaCrypto
import KeyDerivation

final class ChatSignerManager {
    let entropyManager: RootEntropyManaging

    init(entropyManager: RootEntropyManaging = RootEntropyManager.shared) {
        self.entropyManager = entropyManager
    }
}

extension ChatSignerManager: StatementStoreSignerManaging {
    func makeSigner(for signerKeyId: String) throws -> StatementStoreSigning {
        let keypairFactory = WalletMnemonicKeypairFactory(derivationPath: signerKeyId, entropyManager: entropyManager)

        let keypair = try keypairFactory.deriveKeypair()

        let publicKey = try SNPublicKey(rawData: keypair.publicKey().rawData())
        let privateKey = try SNPrivateKey(rawData: keypair.privateKey().rawData())

        return StatementStoreKeypairSigner(keypair: SNKeypair(privateKey: privateKey, publicKey: publicKey))
    }
}
