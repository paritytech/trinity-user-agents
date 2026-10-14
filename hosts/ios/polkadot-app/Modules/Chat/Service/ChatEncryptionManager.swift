import Foundation
import MessageExchangeKit
import Keystore_iOS
import KeyDerivation

final class ChatEncryptionManager {
    let entropyManager: RootEntropyManaging

    init(entropyManager: RootEntropyManaging = RootEntropyManager.shared) {
        self.entropyManager = entropyManager
    }
}

extension ChatEncryptionManager: MessageExchangeEncryptionManaging {
    func makeEncryptorFactory(ownEncryptionKeyId: String) throws -> MessageExchangeEncryptionMaking {
        let privateKey = try ChatPrivateKeyFactory(
            encryptionKeyId: ownEncryptionKeyId,
            entropyManager: entropyManager
        ).derivePrivateKey()

        return X25519ChaChaPolyEncryptorFactory(privateKey: privateKey)
    }
}
