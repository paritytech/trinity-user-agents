import Foundation
import Keystore_iOS
import SubstrateSdk
import Testing
import TrUAPIHost
@testable import polkadot_app

struct CoreStorageBackendTests {
    @Test func protectedGrantsUseKeychainAcrossBackendInstances() throws {
        let suite = "io.polkadotapp.tests.core-storage.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let keychain = InMemoryKeychain()
        let storage = CoreStorageBackend.create(defaults: defaults, keychain: keychain)
        let secret = Data([9])
        let ordinary = Data([4])
        let value = Data([1, 2, 3])
        try storage.write(key: secret, value: value)
        try storage.write(key: ordinary, value: value)
        let reopened = CoreStorageBackend.create(defaults: defaults, keychain: keychain)
        #expect(try reopened.read(key: secret) == value)
        #expect(defaults.data(forKey: "io.polkadotapp.truapi.core.\(secret.toHex())") == nil)
        #expect(defaults.data(forKey: "io.polkadotapp.truapi.core.\(ordinary.toHex())") == value)
        try reopened.clear(key: secret)
        #expect(try storage.read(key: secret) == nil)
        #expect(try storage.read(key: ordinary) == value)
    }

    @Test func reinstallCannotRestoreAnotherInstallSecret() throws {
        let keychain = InMemoryKeychain()
        let suite = "io.polkadotapp.tests.core-storage.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let original = CoreStorageBackend.create(defaults: defaults, keychain: keychain)
        let key = Data([9])
        try original.write(key: key, value: Data([1]))
        defaults.removePersistentDomain(forName: suite)
        let reinstalled = CoreStorageBackend.create(defaults: defaults, keychain: keychain)
        #expect(try reinstalled.read(key: key) == nil)
        #expect(try original.read(key: key) == Data([1]))
    }

    @Test func enumerationIncludesProtectedKeysWithoutReadingTheirValues() throws {
        let suite = "io.polkadotapp.tests.core-storage.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let storage = CoreStorageBackend.create(defaults: defaults)
        let key = Data([9])
        try storage.write(key: key, value: Data([255]))
        defer { try? storage.clear(key: key) }
        #expect(try storage.keys() == [key])
    }

    @Test func keychainFailuresCannotBecomeMissingOrSuccessfulWrites() {
        let storage = CoreStorageBackend.create(keychain: UnavailableKeychain())
        let key = Data([255])
        #expect(throws: HostRejection.self) { try storage.read(key: key) }
        #expect(throws: HostRejection.self) { try storage.write(key: key, value: Data([1])) }
        #expect(throws: HostRejection.self) { try storage.clear(key: key) }
    }
}

private final class UnavailableKeychain: KeystoreProtocol {
    enum Failure: Error { case unavailable }
    func addKey(_: Data, with _: String) throws { throw Failure.unavailable }
    func updateKey(_: Data, with _: String) throws { throw Failure.unavailable }
    func fetchKey(for _: String) throws -> Data { throw Failure.unavailable }
    func checkKey(for _: String) throws -> Bool { throw Failure.unavailable }
    func deleteKey(for _: String) throws { throw Failure.unavailable }
}
