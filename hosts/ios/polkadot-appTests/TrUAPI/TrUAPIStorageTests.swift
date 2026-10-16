import Foundation
import Testing
@testable import polkadot_app

/// Class suite: a fresh instance per test gives each test its own defaults
/// suite and product directory (safe under parallel execution); deinit
/// removes both.
final class TrUAPIStorageTests {
    private let defaults: UserDefaults
    private let suiteName: String
    private let directory: URL

    init() throws {
        suiteName = "io.polkadotapp.tests.truapi-storage.\(UUID().uuidString)"
        defaults = try #require(UserDefaults(suiteName: suiteName))
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-storage-tests-\(UUID().uuidString)", isDirectory: true)
    }

    deinit {
        defaults.removePersistentDomain(forName: suiteName)
        try? FileManager.default.removeItem(at: directory)
    }

    private func product(_ productId: String) -> TrUAPILocalStorage {
        TrUAPILocalStorage.createProductLocalStorage(
            productId: productId,
            defaults: defaults,
            directory: directory
        )
    }

    @Test func productStorageRoundTrip() throws {
        let storage = product("test.product")
        let value = Data([0x01, 0x02])

        try storage.write(key: "k", value: value)
        #expect(try storage.read(key: "k") == value)

        try storage.clear(key: "k")
        #expect(try storage.read(key: "k") == nil)
    }

    @Test func productStorageIsolatesProducts() throws {
        let first = product("a")
        let second = product("b")

        try first.write(key: "k", value: Data([0x01]))

        #expect(try second.read(key: "k") == nil)
    }

    /// The core addresses a granted foreign read with the owner's key; it must
    /// land in the owner's store, not the reader's.
    @Test func foreignReadReachesTheOwnersStorage() throws {
        let owner = product("counter.paseo")
        let reader = product("oracle.paseo")
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try owner.write(key: key, value: Data([0x07]))

        #expect(try reader.read(key: key) == Data([0x07]))
    }

    /// Only reads follow the owner in the key, so a write or clear addressed at
    /// another product can never land in that product's store.
    @Test func writesAndClearsStayInTheCallersStore() throws {
        let owner = product("counter.paseo")
        let other = product("oracle.paseo")
        let key = "truapi:product-storage:v1:13:counter.paseo:count"
        try owner.write(key: key, value: Data([0x01]))

        try other.write(key: key, value: Data([0x02]))
        try other.clear(key: key)

        #expect(try owner.read(key: key) == Data([0x01]))
    }

    /// A product value goes to a file, never to `UserDefaults`: CFPreferences
    /// refuses each write once the domain passes 4 MiB, without an error.
    /// Product files stay out of device backups: a restore must not give a
    /// product an earlier copy of its state (a chat ratchet, for example).
    @Test func productFilesAreExcludedFromBackup() throws {
        let storage = product("test.product")
        try storage.write(key: "k", value: Data([0x01]))

        let values = try directory.resourceValues(forKeys: [.isExcludedFromBackupKey])
        #expect(values.isExcludedFromBackup == true)
    }

    /// A directory that is gone when a value is written comes back out of
    /// device backups too.
    @Test func aRecreatedDirectoryIsExcludedFromBackup() throws {
        let storage = product("test.product")
        try FileManager.default.removeItem(at: directory)

        try storage.write(key: "k", value: Data([0x01]))

        let values = try directory.resourceValues(forKeys: [.isExcludedFromBackupKey])
        #expect(values.isExcludedFromBackup == true)
        #expect(try storage.read(key: "k") == Data([0x01]))
    }

    @Test func productValuesDoNotGoToUserDefaults() throws {
        let storage = product("counter.paseo")
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try storage.write(key: key, value: Data([0x01]))

        #expect(defaults.object(forKey: "io.polkadotapp.truapi.product.store.counter.paseo.\(key)") == nil)
        #expect(try product("counter.paseo").read(key: key) == Data([0x01]))
    }

    /// The image of a database is many values of 64 KiB: far past the 4 MiB
    /// at which `UserDefaults` dropped the writes of a worker (and its next
    /// start then found a damaged image).
    @Test func valuesPastFourMebibytesAllReadBack() throws {
        let storage = product("verbosity.dot")
        let chunk = Data(repeating: 0x5A, count: 64 * 1_024)
        for index in 0 ..< 160 {
            var value = chunk
            value[0] = UInt8(index & 0xFF)
            value[1] = UInt8(index >> 8)
            try storage.write(key: "c/\(index)", value: value)
        }

        let reopened = product("verbosity.dot")
        for index in 0 ..< 160 {
            let value = try #require(try reopened.read(key: "c/\(index)"))
            #expect(value.count == chunk.count)
            #expect(value[0] == UInt8(index & 0xFF) && value[1] == UInt8(index >> 8))
        }
    }

    /// A value that an earlier build kept in `UserDefaults` moves to a file,
    /// and leaves `UserDefaults`.
    @Test func legacyValuesMoveToFiles() throws {
        let key = "truapi:product-storage:v1:13:counter.paseo:count"
        let legacy = "io.polkadotapp.truapi.product.store.counter.paseo.\(key)"
        defaults.set(Data([0x09]), forKey: legacy)

        let storage = product("counter.paseo")

        #expect(defaults.object(forKey: legacy) == nil)
        #expect(try storage.read(key: key) == Data([0x09]))
    }

    /// A clear removes the value from the file and from `UserDefaults`, so a
    /// legacy copy cannot come back.
    @Test func clearRemovesEveryCopy() throws {
        let storage = product("counter.paseo")
        let key = "truapi:product-storage:v1:13:counter.paseo:count"
        try storage.write(key: key, value: Data([0x01]))
        defaults.set(Data([0x02]), forKey: "io.polkadotapp.truapi.product.store.counter.paseo.\(key)")

        try storage.clear(key: key)

        #expect(try storage.read(key: key) == nil)
    }

    /// The core lowercases the owner in the key, but the store keeps the id's
    /// original casing, so an own key must read from the caller's prefix.
    @Test func ownKeysReadFromTheCallersCasing() throws {
        let storage = product("Counter.paseo")
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try storage.write(key: key, value: Data([0x01]))

        #expect(try storage.read(key: key) == Data([0x01]))
    }

    @Test func ownerIsReadFromTheCoreKeyFormat() {
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:13:counter.paseo:a:b") == "counter.paseo")
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:7:caf\u{e9}.p:k") == "caf\u{e9}.p")
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:13:counter.paseo") == nil)
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:12:counter.paseo:k") == nil)
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:99:counter.paseo:k") == nil)
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:x:counter.paseo:k") == nil)
        #expect(ProductStorageKey.owner(of: "truapi:product-storage:v1:4:caf\u{e9}.p:k") == nil)
        #expect(ProductStorageKey.owner(of: "k") == nil)
    }

    @Test func coreStorageRoundTrip() throws {
        let storage = TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults)
        let key = Data([0x00]).toHex() // CoreStorageKey.AuthSession
        let value = Data([0xAA])

        try storage.write(key: key, value: value)
        #expect(try storage.read(key: key) == value)

        try storage.clear(key: key)
        #expect(try storage.read(key: key) == nil)
    }

    @Test func coreStorageIsolatedFromProductStorage() throws {
        let core = TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults)
        let storage = product("test.product")

        try core.write(key: "k", value: Data([0x01]))

        #expect(try storage.read(key: "k") == nil)
    }
}
