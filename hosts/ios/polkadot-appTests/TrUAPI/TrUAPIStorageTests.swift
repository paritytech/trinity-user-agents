import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Class suite: a fresh instance per test gives each test its own defaults
/// suite (safe under parallel execution); deinit removes the domain.
final class TrUAPIStorageTests {
    private let defaults: UserDefaults
    private let suiteName: String

    init() throws {
        suiteName = "io.polkadotapp.tests.truapi-storage.\(UUID().uuidString)"
        defaults = try #require(UserDefaults(suiteName: suiteName))
    }

    deinit {
        defaults.removePersistentDomain(forName: suiteName)
    }

    @Test func coreEnumerationFindsExistingNamespaceKeysOnly() throws {
        // Seed the actual persisted namespace, not a newly-maintained catalog.
        defaults.set(Data([1]), forKey: "io.polkadotapp.truapi.core.\(Data([1, 2]).toHex())")
        defaults.set(Data([2]), forKey: "io.polkadotapp.truapi.core.\(Data([3]).toHex())")
        defaults.set(Data([3]), forKey: "io.polkadotapp.truapi.core-other.0x04")
        defaults.set(Data([4]), forKey: "io.polkadotapp.truapi.product.store.demo.0x05")
        let backend = CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults))
        #expect(Set(try backend.keys()) == Set([Data([1, 2]), Data([3])]))
        try backend.clear(key: Data([3]))
        #expect(try backend.keys() == [Data([1, 2])])
    }

    @Test func productStorageRoundTrip() throws {
        let storage = TrUAPILocalStorage.createProductLocalStorage(
            productId: "test.product",
            defaults: defaults, storageDomain: suiteName
        )
        let value = Data([0x01, 0x02])

        try storage.write(key: "k", value: value)
        #expect(try storage.read(key: "k") == value)

        try storage.clear(key: "k")
        #expect(try storage.read(key: "k") == nil)
    }

    @Test func productStorageIsolatesProducts() throws {
        let first = TrUAPILocalStorage.createProductLocalStorage(productId: "a", defaults: defaults, storageDomain: suiteName)
        let second = TrUAPILocalStorage.createProductLocalStorage(productId: "b", defaults: defaults, storageDomain: suiteName)

        try first.write(key: "k", value: Data([0x01]))

        #expect(try second.read(key: "k") == nil)
    }

    /// The core addresses a granted foreign read with the owner's key; it must
    /// land in the owner's store, not the reader's.
    @Test func foreignReadReachesTheOwnersStorage() throws {
        let owner = TrUAPILocalStorage.createProductLocalStorage(productId: "counter.paseo", defaults: defaults, storageDomain: suiteName)
        let reader = TrUAPILocalStorage.createProductLocalStorage(productId: "oracle.paseo", defaults: defaults, storageDomain: suiteName)
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try owner.write(key: key, value: Data([0x07]))

        #expect(try reader.read(key: key) == Data([0x07]))
    }

    /// Only reads follow the owner in the key, so a write or clear addressed at
    /// another product can never land in that product's store.
    @Test func writesAndClearsStayInTheCallersStore() throws {
        let owner = TrUAPILocalStorage.createProductLocalStorage(productId: "counter.paseo", defaults: defaults, storageDomain: suiteName)
        let other = TrUAPILocalStorage.createProductLocalStorage(productId: "oracle.paseo", defaults: defaults, storageDomain: suiteName)
        let key = "truapi:product-storage:v1:13:counter.paseo:count"
        try owner.write(key: key, value: Data([0x01]))

        try other.write(key: key, value: Data([0x02]))
        try other.clear(key: key)

        #expect(try owner.read(key: key) == Data([0x01]))
    }

    @Test func ownKeysKeepTheirPhysicalKey() throws {
        let storage = TrUAPILocalStorage.createProductLocalStorage(productId: "counter.paseo", defaults: defaults, storageDomain: suiteName)
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try storage.write(key: key, value: Data([0x01]))

        #expect(defaults.data(forKey: "io.polkadotapp.truapi.product.store.counter.paseo.\(key)") == Data([0x01]))
    }

    /// The core lowercases the owner in the key, but the store keeps the id's
    /// original casing, so an own key must read from the caller's prefix.
    @Test func ownKeysReadFromTheCallersCasing() throws {
        let storage = TrUAPILocalStorage.createProductLocalStorage(productId: "Counter.paseo", defaults: defaults, storageDomain: suiteName)
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
        let storage = TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults, storageDomain: suiteName)
        let key = Data([0x00]).toHex() // CoreStorageKey.AuthSession
        let value = Data([0xAA])

        try storage.write(key: key, value: value)
        #expect(try storage.read(key: key) == value)

        try storage.clear(key: key)
        #expect(try storage.read(key: key) == nil)
    }

    @Test func coreStorageIsolatedFromProductStorage() throws {
        let core = TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults, storageDomain: suiteName)
        let product = TrUAPILocalStorage.createProductLocalStorage(
            productId: "test.product",
            defaults: defaults, storageDomain: suiteName
        )

        try core.write(key: "k", value: Data([0x01]))

        #expect(try product.read(key: "k") == nil)
    }

    @Test func independentNativeWalletRefusesRustPurseCustody() throws {
        let storage = CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults, storageDomain: suiteName))
        // MainPurseCoinage is wallet-root/network scoped. Refusing its read is
        // essential: nil would authorize Core to create a competing allocator.
        let key = Data([13]) + Data(repeating: 0x42, count: 64)
        #expect(throws: HostRejection.self) { try storage.read(key: key) }
        #expect(throws: HostRejection.self) { try storage.write(key: key, value: Data([1])) }
        #expect(throws: HostRejection.self) { try storage.clear(key: key) }
    }

    @Test func nativeChatSnapshotsSurviveReadThenRepeatedReplacement() throws {
        let nonce = withUnsafeBytes(of: UUID().uuid) { Data($0) }
        let key = Data([16]) + nonce + nonce + Data(repeating: 0, count: 32)
        let storage = CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults, storageDomain: suiteName))
        defer { try? storage.clear(key: key) }

        #expect(try storage.read(key: key) == nil)
        try storage.write(key: key, value: Data([1, 2]))
        #expect(try storage.read(key: key) == Data([1, 2]))
        try storage.write(key: key, value: Data([3, 4]))
        #expect(try storage.read(key: key) == Data([3, 4]))
        try storage.clear(key: key)
        #expect(try storage.read(key: key) == nil)
    }
}
