import Foundation
import Keystore_iOS
import Testing
@testable import polkadot_app

struct TrUAPIEncryptedStorageTests {
    private let keychain = InMemoryKeychain()

    /// The core reads a secret back by its key, so a written value must read
    /// back, and a later write replaces it.
    @Test func writtenValueReadsBackAndALaterWriteReplacesIt() throws {
        let storage = TrUAPIEncryptedStorage(keychain: keychain)

        try storage.write(key: Data([0x01]), value: Data([0xAA]))
        try storage.write(key: Data([0x01]), value: Data([0xBB]))

        #expect(try storage.read(key: Data([0x01])) == Data([0xBB]))
    }

    /// A missing secret is `nil`, not an error, so the core can tell an
    /// absent secret from an unreadable Keychain.
    @Test func missingKeyReadsAsNil() throws {
        #expect(try TrUAPIEncryptedStorage(keychain: keychain).read(key: Data([0x02])) == nil)
    }

    /// A deleted value reads as `nil`, others stay, and deleting a missing
    /// key succeeds.
    @Test func deletedValueReadsAsNilAndDeletingNoneSucceeds() throws {
        let storage = TrUAPIEncryptedStorage(keychain: keychain)
        try storage.write(key: Data([0x01]), value: Data([0xAA]))
        try storage.write(key: Data([0x02]), value: Data([0xBB]))

        try storage.delete(key: Data([0x01]))
        try storage.delete(key: Data([0x03]))

        #expect(try storage.read(key: Data([0x01])) == nil)
        #expect(try storage.read(key: Data([0x02])) == Data([0xBB]))
    }
}
