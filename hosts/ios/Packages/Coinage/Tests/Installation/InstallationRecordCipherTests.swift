import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

/// Pinned against values computed outside this codebase (Python `hashlib.blake2b`, Node
/// `chacha20-poly1305`) and shared with Android, so a change here is a change to the on-chain format.
struct InstallationRecordCipherTests {
    private static let sr25519Secret = Data((0 ..< 64).map { UInt8($0) })
    private static let encryptionKey = "0xfc7bd73006ab990f9649eebec3aab66023af1f064d33a694d4d7a4f3bef86748"

    // nonce = blake2b256(key = blake2b256(key = encryptionKey, "nonce-key"), "nonce" || installation)[0..12]
    private static let record = "0x2cb9ac950a3a69cfa00480e7e221d461069884d050d21b2ba1c05e0e9bd08b3f"
        + "e0edf24784976d3bd6e5652bb755ca90d48906c10e655eb8f3acc4c6"
    private static let recordWithAnotherNonce = "0x000102030405060708090a0bcdc58e6066449725c690c637add5e75d5fae9ea0e7c5"
        + "df58e99ac4e0923ba664ba2d2aa75adc69e254a3e31eb2df5595"

    private let key = try! DataStoreAccountKeys.deriveEncryptionKey(sr25519Secret: sr25519Secret)

    @Test("the encryption key is the context keyed by the whole 64-byte secret")
    func encryptionKey() {
        #expect(key.toHex(includePrefix: true) == Self.encryptionKey)
    }

    @Test("a sealed installation is the pinned record")
    func sealedRecord() throws {
        #expect(try InstallationRecordCipher.seal(.test, key: key).toHex(includePrefix: true) == Self.record)
    }

    @Test("sealing the same installation twice gives the same bytes")
    func deterministic() throws {
        #expect(try InstallationRecordCipher.seal(.test, key: key) == InstallationRecordCipher.seal(.test, key: key))
    }

    @Test("a sealed record opens back to its installation")
    func roundTrip() throws {
        let record = try InstallationRecordCipher.seal(.test, key: key)
        #expect(InstallationRecordCipher.open(record, key: key) == .test)
    }

    @Test("a record sealed with a nonce of its own still opens")
    func foreignNonce() throws {
        let record = try Data(hexString: Self.recordWithAnotherNonce)
        #expect(InstallationRecordCipher.open(record, key: key) == .test)
    }

    @Test("a tampered record does not open")
    func tampered() throws {
        var record = try Data(hexString: Self.record)
        record[20] ^= 1
        #expect(InstallationRecordCipher.open(record, key: key) == nil)
    }

    @Test("a record sealed under another seed does not open")
    func foreignKey() throws {
        let otherKey = try DataStoreAccountKeys.deriveEncryptionKey(sr25519Secret: Data(repeating: 7, count: 64))
        let record = try InstallationRecordCipher.seal(.test, key: otherKey)
        #expect(InstallationRecordCipher.open(record, key: key) == nil)
    }

    @Test("bytes of the wrong length are not a record")
    func wrongLength() {
        #expect(InstallationRecordCipher.open(Data([1, 2, 3]), key: key) == nil)
    }
}
