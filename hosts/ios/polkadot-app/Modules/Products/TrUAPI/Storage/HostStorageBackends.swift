import Foundation
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

/// Adapts core-private SCALE keys. Existing slots retain their UserDefaults
/// backing; wallet-state slots use atomic, synced files outside product storage.
/// Plain Swift errors (including ambiguous durable writes) become HostRejection.
final class CoreStorageBackend: HostCoreStorageBackend, @unchecked Sendable {
    private let storage: TrUAPILocalStoring

    init(storage: TrUAPILocalStoring) {
        self.storage = storage
    }

    func read(key: Data) throws -> Data? {
        if TrUAPIWalletStorage.owns(key) {
            return try withHostRejection { try TrUAPIWalletStorage.shared.read(key: key) }
        }
        return try withHostRejection { try storage.read(key: key.toHex()) }
    }

    func write(key: Data, value: Data) throws {
        if TrUAPIWalletStorage.owns(key) {
            return try withHostRejection { try TrUAPIWalletStorage.shared.write(key: key, value: value) }
        }
        try withHostRejection { try storage.write(key: key.toHex(), value: value) }
    }

    func keys() throws -> [Data] {
        try withHostRejection {
            try storage.keys().map { try Data(hexString: $0) }
        }
    }

    func clear(key: Data) throws {
        if TrUAPIWalletStorage.owns(key) {
            return try withHostRejection { try TrUAPIWalletStorage.shared.clear(key: key) }
        }
        try withHostRejection { try storage.clear(key: key.toHex()) }
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
