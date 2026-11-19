import CryptoKit
import Foundation
@testable import polkadot_app

struct StubDeviceEncryptionKeyManager: DeviceEncryptionKeyManaging {
    let privateKey = Curve25519.KeyAgreement.PrivateKey()

    func getOrCreatePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey {
        privateKey
    }

    func getPublicKey() throws -> Data {
        privateKey.publicKey.rawRepresentation
    }
}
