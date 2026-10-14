import Foundation
import CryptoKit
import MessageExchangeKit

final class DeviceSyncEncryptionManager {
    private let privateKey: Curve25519.KeyAgreement.PrivateKey

    init(privateKey: Curve25519.KeyAgreement.PrivateKey) {
        self.privateKey = privateKey
    }
}

extension DeviceSyncEncryptionManager: MessageExchangeEncryptionManaging {
    func makeEncryptorFactory(
        ownEncryptionKeyId _: String
    ) throws -> MessageExchangeEncryptionMaking {
        X25519ChaChaPolyEncryptorFactory(privateKey: privateKey)
    }
}
