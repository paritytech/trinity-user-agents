import Testing
import Foundation
import NovaCrypto

struct SNKeyFactoryTests {
    private let keysFactory = SNKeyFactory()

    private static let secretSize = 64
    private static let seedSize = 32

    // MARK: - Length guard

    @Test("Throws on short secret (seed-sized)")
    func shortSecretThrows() {
        let secret = Data(repeating: 0, count: Self.seedSize)
        #expect(throws: (any Error).self) {
            _ = try keysFactory.createPublicKey(fromSecret: secret)
        }
    }

    @Test("Throws on long secret")
    func longSecretThrows() {
        let secret = Data(repeating: 0, count: Self.secretSize + 1)
        #expect(throws: (any Error).self) {
            _ = try keysFactory.createPublicKey(fromSecret: secret)
        }
    }

    @Test("Throws on empty secret")
    func emptySecretThrows() {
        #expect(throws: (any Error).self) {
            _ = try keysFactory.createPublicKey(fromSecret: Data())
        }
    }

    // MARK: - Seed round-trip

    @Test("Public key derived from secret matches keypair public key for same seed")
    func matchesPublicKeyDerivedFromSameSeedKeypair() throws {
        let seed = Data(repeating: 0xAB, count: Self.seedSize)

        let keypair = try keysFactory.createKeypair(fromSeed: seed)

        let secretBytes = keypair.privateKey().rawData()
        #expect(secretBytes.count == Self.secretSize)

        let derived = try keysFactory.createPublicKey(fromSecret: secretBytes)
        #expect(derived.rawData() == keypair.publicKey().rawData())
    }

    // MARK: - Non-canonical input (previously crashed inside Rust FFI)

    @Test("All-zero secret is accepted by the FFI and yields a public key without crashing")
    func allZeroSecretProducesPublicKey() throws {
        let secret = Data(repeating: 0x00, count: Self.secretSize)
        let publicKey = try keysFactory.createPublicKey(fromSecret: secret)
        #expect(publicKey.rawData().count == Self.seedSize)
    }

    @Test("All-ones secret returns error instead of crashing")
    func allOnesSecretThrows() {
        let secret = Data(repeating: 0xFF, count: Self.secretSize)
        #expect(throws: (any Error).self) {
            _ = try keysFactory.createPublicKey(fromSecret: secret)
        }
    }

    @Test("Invalid high bits in byte 31 return error instead of crashing")
    func invalidHighBitsThrows() {
        var bytes = [UInt8](repeating: 0x01, count: Self.secretSize)
        bytes[31] = 0xE0 // top 3 bits = 111, violates required 010
        let secret = Data(bytes)
        #expect(throws: (any Error).self) {
            _ = try keysFactory.createPublicKey(fromSecret: secret)
        }
    }
}
