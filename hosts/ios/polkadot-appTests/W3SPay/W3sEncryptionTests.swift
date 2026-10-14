import CryptoKit
import Foundation
import MessageExchangeKit
import Testing

@testable import polkadot_app

@Suite("W3S ECIES round-trip")
struct W3sEncryptionTests {
    @Test("iOS ECIES wire format decrypts back to the same plaintext with the merchant's key")
    func eciesRoundTrip() throws {
        // 1. Simulated merchant keypair — the iOS app sees only the 32-byte public key.
        let merchantPrivate = Curve25519.KeyAgreement.PrivateKey()
        let merchantPublicRaw = merchantPrivate.publicKey.rawRepresentation

        // 2. iOS side: generate ephemeral, encrypt plaintext exactly as W3sStatementSubmitter does.
        let ephemeralPrivate = Curve25519.KeyAgreement.PrivateKey()
        let factory = X25519ChaChaPolyEncryptorFactory(privateKey: ephemeralPrivate)
        let encryptor = try factory.makeEncryptor(remotePublicKey: merchantPublicRaw)

        let plaintext = Data("hello W3S merchant".utf8)
        let ciphertext = try encryptor.encrypt(plaintext)

        // 3. Merchant side: derive the shared secret with the ephemeral pubkey we sent,
        //    re-build the symmetric encryptor, decrypt.
        let ephemeralPublicReceived = try Curve25519.KeyAgreement.PublicKey(
            rawRepresentation: factory.localPublicKey
        )
        let merchantShared = try merchantPrivate.sharedSecretFromKeyAgreement(with: ephemeralPublicReceived)
        let merchantEncryptor = ChaChaPolyEncryptor(sharedSecret: merchantShared)
        let recovered = try merchantEncryptor.decrypt(ciphertext)

        #expect(recovered == plaintext)
    }

    @Test("Ephemeral public key emitted by X25519ChaChaPolyEncryptorFactory is the raw 32-byte form")
    func ephemeralIsRaw32Bytes() {
        let ephemeralPrivate = Curve25519.KeyAgreement.PrivateKey()
        let factory = X25519ChaChaPolyEncryptorFactory(privateKey: ephemeralPrivate)

        #expect(factory.localPublicKey.count == 32)
    }

    @Test("Ciphertext layout is nonce(12) ‖ AEAD ciphertext ‖ tag(16)")
    func ciphertextLayout() throws {
        let merchantPrivate = Curve25519.KeyAgreement.PrivateKey()
        let factory = X25519ChaChaPolyEncryptorFactory(privateKey: Curve25519.KeyAgreement.PrivateKey())
        let encryptor = try factory.makeEncryptor(
            remotePublicKey: merchantPrivate.publicKey.rawRepresentation
        )

        let plaintext = Data((0 ..< 32).map { _ in UInt8.random(in: 0 ... 255) })
        let ciphertext = try encryptor.encrypt(plaintext)
        // ChaCha20-Poly1305 12-byte nonce + len(plaintext) + 16-byte tag.
        #expect(ciphertext.count == 12 + plaintext.count + 16)
    }
}
