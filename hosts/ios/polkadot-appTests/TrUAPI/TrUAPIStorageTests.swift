import Foundation
import Testing
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

    @Test func productStorageRoundTrip() throws {
        let storage = TrUAPILocalStorage.createProductLocalStorage(
            productId: "test.product",
            defaults: defaults
        )
        let value = Data([0x01, 0x02])

        try storage.write(key: "k", value: value)
        #expect(try storage.read(key: "k") == value)

        try storage.clear(key: "k")
        #expect(try storage.read(key: "k") == nil)
    }

    @Test func productStorageIsolatesProducts() throws {
        let first = TrUAPILocalStorage.createProductLocalStorage(productId: "a", defaults: defaults)
        let second = TrUAPILocalStorage.createProductLocalStorage(productId: "b", defaults: defaults)

        try first.write(key: "k", value: Data([0x01]))

        #expect(try second.read(key: "k") == nil)
    }

    /// The core addresses a granted foreign read with the owner's key; it must
    /// land in the owner's store, not the reader's.
    @Test func foreignReadReachesTheOwnersStorage() throws {
        let owner = TrUAPILocalStorage.createProductLocalStorage(productId: "counter.paseo", defaults: defaults)
        let reader = TrUAPILocalStorage.createProductLocalStorage(productId: "oracle.paseo", defaults: defaults)
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try owner.write(key: key, value: Data([0x07]))

        #expect(try reader.read(key: key) == Data([0x07]))
    }

    /// Only reads follow the owner in the key, so a write or clear addressed at
    /// another product can never land in that product's store.
    @Test func writesAndClearsStayInTheCallersStore() throws {
        let owner = TrUAPILocalStorage.createProductLocalStorage(productId: "counter.paseo", defaults: defaults)
        let other = TrUAPILocalStorage.createProductLocalStorage(productId: "oracle.paseo", defaults: defaults)
        let key = "truapi:product-storage:v1:13:counter.paseo:count"
        try owner.write(key: key, value: Data([0x01]))

        try other.write(key: key, value: Data([0x02]))
        try other.clear(key: key)

        #expect(try owner.read(key: key) == Data([0x01]))
    }

    @Test func ownKeysKeepTheirPhysicalKey() throws {
        let storage = TrUAPILocalStorage.createProductLocalStorage(productId: "counter.paseo", defaults: defaults)
        let key = "truapi:product-storage:v1:13:counter.paseo:count"

        try storage.write(key: key, value: Data([0x01]))

        #expect(defaults.data(forKey: "io.polkadotapp.truapi.product.store.counter.paseo.\(key)") == Data([0x01]))
    }

    /// The core lowercases the owner in the key, but the store keeps the id's
    /// original casing, so an own key must read from the caller's prefix.
    @Test func ownKeysReadFromTheCallersCasing() throws {
        let storage = TrUAPILocalStorage.createProductLocalStorage(productId: "Counter.paseo", defaults: defaults)
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
        let product = TrUAPILocalStorage.createProductLocalStorage(
            productId: "test.product",
            defaults: defaults
        )

        try core.write(key: "k", value: Data([0x01]))

        #expect(try product.read(key: "k") == nil)
    }
}
