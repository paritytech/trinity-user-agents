import CryptoKit
import Foundation
import MessageExchangeKit
import SubstrateSdk
import Testing

/// Normative negative tests from chat-spec RFC-0004: small-order public keys MUST abort
/// key agreement (all-zero shared secret, RFC 7748), and truncated or bit-flipped
/// ciphertexts MUST fail authentication (RFC 8439).
@Suite("X25519 + ChaCha20-Poly1305 negative tests")
struct EncryptionNegativeTests {
    /// The canonical small-order Curve25519 points: neutral elements, the order-8
    /// points, and p−1 (order 2). DH against any of these yields the all-zero secret.
    private static let smallOrderPointsHex = [
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0100000000000000000000000000000000000000000000000000000000000000",
        "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
        "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f1157",
        "eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f"
    ]

    @Test("Key agreement against a small-order public key aborts", arguments: smallOrderPointsHex)
    func smallOrderKeyAborts(pointHex: String) throws {
        let factory = X25519ChaChaPolyEncryptorFactory(
            privateKey: Curve25519.KeyAgreement.PrivateKey()
        )
        let smallOrderKey = try Data(hexString: pointHex)

        #expect(throws: Error.self) {
            _ = try factory.makeEncryptor(remotePublicKey: smallOrderKey)
        }
    }

    @Test("Bit-flipped ciphertext fails authentication")
    func bitFlippedCiphertextFails() throws {
        let (encryptor, ciphertext) = try makeEncryptedSample()

        for index in [0, 12, ciphertext.count - 1] { // nonce, ciphertext body, tag
            var tampered = ciphertext
            tampered[index] ^= 0x01
            #expect(throws: Error.self, "flipped byte \(index)") {
                _ = try encryptor.decrypt(tampered)
            }
        }
    }

    @Test("Truncated ciphertext fails authentication")
    func truncatedCiphertextFails() throws {
        let (encryptor, ciphertext) = try makeEncryptedSample()

        #expect(throws: Error.self) {
            _ = try encryptor.decrypt(ciphertext.dropLast())
        }
        #expect(throws: Error.self) {
            _ = try encryptor.decrypt(ciphertext.prefix(12))
        }
    }
}

private extension EncryptionNegativeTests {
    func makeEncryptedSample() throws -> (MessageExchangeEncrypting, Data) {
        let factory = X25519ChaChaPolyEncryptorFactory(
            privateKey: Curve25519.KeyAgreement.PrivateKey()
        )
        let peerKey = Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation
        let encryptor = try factory.makeEncryptor(remotePublicKey: peerKey)
        let ciphertext = try encryptor.encrypt(Data("negative test payload".utf8))
        return (encryptor, ciphertext)
    }
}
