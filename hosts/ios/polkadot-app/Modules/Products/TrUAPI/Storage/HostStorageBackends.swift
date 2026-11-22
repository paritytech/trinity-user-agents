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

/// Adapts the host-global core ``TrUAPILocalStoring`` to
/// `HostCoreStorageBackend`. Core keys are SCALE-encoded `Data`; they are
/// hex-encoded for the underlying String-keyed store. Plain Swift errors
/// surface as `HostRejection`.
final class CoreStorageBackend: HostCoreStorageBackend, @unchecked Sendable {
    private let storage: TrUAPILocalStoring
    var storageIdentifier: String { storage.storageIdentifier }

    init(storage: TrUAPILocalStoring) {
        self.storage = storage
    }

    func read(key: Data) throws -> Data? {
        try withHostRejection { try storage.read(key: key.toHex()) }
    }

    func write(key: Data, value: Data) throws {
        try withHostRejection { try storage.write(key: key.toHex(), value: value) }
        notifyPermissionChange(key: key)
    }

    func clear(key: Data) throws {
        try withHostRejection { try storage.clear(key: key.toHex()) }
        notifyPermissionChange(key: key)
    }

    // UI metadata observation only; cross-core policy refresh is exclusively
    // driven by explicit policy-change intent, never raw writes or Ask stamping.
    private func notifyPermissionChange(key: Data) {
        guard let description = try? nativeDescribeCoreStorageKey(encoded: key),
              description.permissionRequest != nil, let productId = description.productId else { return }
        NotificationCenter.default.post(name: TrUAPIMediaPermissionSettings.didChange,
            object: nil, userInfo: ["productId": productId])
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
