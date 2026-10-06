import Foundation
import TrUAPIHost

final class StubSecretStorage: HostSecretStorageBackend, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: Data] = [:]

    func read(key: SecretCoreStorageKey) async throws -> Data? {
        lock.withLock { values[secretCoreStorageKeyIdentifier(key: key)] }
    }

    func write(key: SecretCoreStorageKey, value: Data) async throws {
        lock.withLock { values[secretCoreStorageKeyIdentifier(key: key)] = value }
    }

    func clear(key: SecretCoreStorageKey) async throws {
        lock.withLock { values[secretCoreStorageKeyIdentifier(key: key)] = nil }
    }
}
