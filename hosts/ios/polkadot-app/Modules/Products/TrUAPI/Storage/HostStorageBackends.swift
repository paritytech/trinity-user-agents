import Foundation
import Keystore_iOS
import TrUAPIHost
import SubstrateSdk

/// Adapts a product-scoped ``TrUAPILocalStoring`` (String keys) to the
/// TrUAPIHost `HostStorageBackend`. Plain Swift errors surface as the
/// FFI `HostLocalStorageReadError` the rust core expects.
final class ProductStorageBackend: HostStorageBackend, @unchecked Sendable {
    private let storage: TrUAPILocalStoring

    init(storage: TrUAPILocalStoring) {
        self.storage = storage
    }

    func read(key: String) throws -> Data? {
        try withStorageError { try storage.read(key: key) }
    }

    func write(key: String, value: Data) throws {
        try withStorageError { try storage.write(key: key, value: value) }
    }

    func clear(key: String) throws {
        try withStorageError { try storage.clear(key: key) }
    }
}

final class CoreStorageBackend: HostCoreStorageBackend, @unchecked Sendable {
    private static let lock = NSLock()
    private let storage: TrUAPILocalStoring
    private let keychain: KeystoreProtocol

    private init(storage: TrUAPILocalStoring, keychain: KeystoreProtocol) {
        self.storage = storage
        self.keychain = keychain
    }

    static func create(
        defaults: UserDefaults = .standard,
        keychain: KeystoreProtocol = Keychain()
    ) -> CoreStorageBackend {
        CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults), keychain: keychain)
    }

    func read(key: Data) throws -> Data? {
        try withStorage {
            guard try isProtected(key) else { return try storage.read(key: key.toHex()) }
            do {
                return try keychain.fetchKey(for: identifier(key))
            } catch KeystoreError.noKeyFound {
                return nil
            }
        }
    }

    func write(key: Data, value: Data) throws {
        try withStorage {
            if try isProtected(key) {
                try keychain.saveKey(value, with: identifier(key))
            } else {
                try storage.write(key: key.toHex(), value: value)
            }
        }
    }

    func clear(key: Data) throws {
        try withStorage {
            if try isProtected(key) {
                try keychain.deleteKeyIfExists(for: identifier(key))
            } else {
                try storage.clear(key: key.toHex())
            }
        }
    }

    private func withStorage<T>(_ operation: () throws -> T) throws -> T {
        try withHostRejection {
            try Self.lock.withLock {
                try Task.checkCancellation()
                return try operation()
            }
        }
    }

    private func isProtected(_ key: Data) throws -> Bool {
        switch try coreStorageKeyKind(encoded: key) {
        case "AuthSession",
             "PairingDeviceIdentity",
             "AllowanceKeys",
             "AutoSigningKey",
             "AutoSigningKeys",
             "DeviceEncryptionKey",
             "NativeAllowanceKeys": true
        default: false
        }
    }

    private func identifier(_ key: Data) -> String {
        "io.polkadotapp.truapi.core.\(key.toHex())"
    }
}

/// No-op product storage for host-level bridges that have no product scope.
/// The process-wide runtime only owns core storage; product KV is opened
/// per execution, so these calls never carry product data.
final class EmptyHostStorageBackend: HostStorageBackend, @unchecked Sendable {
    func read(key _: String) throws -> Data? { nil }
    func write(key _: String, value _: Data) throws {}
    func clear(key _: String) throws {}
}

// MARK: - FFI error mapping

private func withHostRejection<T>(_ body: () throws -> T) throws -> T {
    do {
        return try body()
    } catch let error as HostRejection {
        throw error
    } catch {
        throw HostRejection.Rejected(reason: "\(error)")
    }
}

private func withStorageError<T>(_ body: () throws -> T) throws -> T {
    do {
        return try body()
    } catch let error as HostLocalStorageReadError {
        throw error
    } catch {
        throw HostLocalStorageReadError.Unknown(reason: "\(error)")
    }
}
