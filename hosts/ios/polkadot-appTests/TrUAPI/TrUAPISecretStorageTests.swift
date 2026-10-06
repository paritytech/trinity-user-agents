import CryptoKit
import Foundation
import Keystore_iOS
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPISecretStorageTests {
    @Test func recreatedStorageKeepsNativeAllowancesIsolated() async throws {
        let keychain = MemoryKeychain()
        let first = storage(keychain: keychain, installation: "first")
        let second = storage(keychain: keychain, installation: "second")
        try await first.write(key: .nativeAllowanceKeys, value: Data([1]))
        try await first.write(key: .allowanceKeys(sessionId: "paired"), value: Data([3]))
        #expect(try await second.read(key: .nativeAllowanceKeys) == nil)
        try await second.write(key: .nativeAllowanceKeys, value: Data([2]))
        let recreated = storage(keychain: keychain, installation: "first")
        #expect(try await recreated.read(key: .nativeAllowanceKeys) == Data([1]))
        try await recreated.clear(key: .nativeAllowanceKeys)
        #expect(try await first.read(key: .nativeAllowanceKeys) == nil)
        #expect(try await first.read(key: .allowanceKeys(sessionId: "paired")) == Data([3]))
        #expect(try await second.read(key: .nativeAllowanceKeys) == Data([2]))
    }

    @Test func onlyMissingKeysReturnNil() async throws {
        let keychain = MemoryKeychain()
        let backend = storage(keychain: keychain)
        #expect(try await backend.read(key: .authSession) == nil)
        keychain.fetchFailure = KeystoreError.unexpectedFail
        await #expect(throws: KeystoreError.self) {
            try await backend.read(key: .authSession)
        }
        keychain.fetchFailure = nil
        try await backend.write(key: .authSession, value: Data())
        #expect(try await backend.read(key: .authSession) == Data())
    }

    @Test func deviceIdentityUsesItsSharedProviderAndCannotBeMutated() async throws {
        let keychain = MemoryKeychain()
        let device = TestDeviceKeys()
        let backend = storage(keychain: keychain, device: device)
        #expect(try await backend.read(key: .deviceEncryptionKey) == device.key.rawRepresentation)
        await #expect(throws: HostRejection.self) {
            try await backend.write(key: .deviceEncryptionKey, value: Data([1]))
        }
        await #expect(throws: HostRejection.self) {
            try await backend.clear(key: .deviceEncryptionKey)
        }
        #expect(device.reads == 1)
        #expect(keychain.snapshot.isEmpty)
    }

    @Test func cleanupWaitsForACancelledWriteAlreadyInsideKeychain() async throws {
        let lock = ObservableLock()
        let keychain = MemoryKeychain()
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let cleanupWaiting = DispatchSemaphore(value: 0)
        let cleanupFinished = DispatchSemaphore(value: 0)
        keychain.onCheck = {
            entered.signal()
            precondition(release.wait(timeout: .now() + 5) == .success)
        }
        let secret = storage(keychain: keychain, lock: lock)
        let writer = Task.detached { try await secret.write(key: .authSession, value: Data([1])) }
        #expect(await waitForSignal(entered) == .success)
        writer.cancel()
        lock.onLock = { cleanupWaiting.signal() }
        let cleanup = Task.detached {
            try await secret.clear(key: .authSession)
            cleanupFinished.signal()
        }
        #expect(await waitForSignal(cleanupWaiting) == .success)
        #expect(await waitForSignal(cleanupFinished, timeout: .now()) == .timedOut)
        keychain.onCheck = nil
        release.signal()
        _ = await writer.result
        try await cleanup.value
        #expect(try await secret.read(key: .authSession) == nil)
    }

    @Test func aCancelledTaskWaitingForTheLockCannotResurrectASecret() async throws {
        let lock = ObservableLock()
        let keychain = MemoryKeychain()
        let backend = storage(keychain: keychain, lock: lock)
        try await backend.write(key: .authSession, value: Data([1]))
        let waiting = DispatchSemaphore(value: 0)
        let writer = lock.withLock {
            lock.onLock = { waiting.signal() }
            let writer = Task.detached { try await backend.write(key: .authSession, value: Data([2])) }
            #expect(waiting.wait(timeout: .now() + 5) == .success)
            writer.cancel()
            return writer
        }
        await #expect(throws: CancellationError.self) { try await writer.value }
        try await backend.clear(key: .authSession)
        #expect(try await backend.read(key: .authSession) == nil)
    }

    private func waitForSignal(
        _ semaphore: DispatchSemaphore,
        timeout: DispatchTime = .now() + 5
    ) async -> DispatchTimeoutResult {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async {
                continuation.resume(returning: semaphore.wait(timeout: timeout))
            }
        }
    }

    private func storage(
        keychain: MemoryKeychain,
        installation: String = "test-install",
        device: TestDeviceKeys = TestDeviceKeys(),
        lock: NSLock = NSLock()
    ) -> TrUAPISecretStorage {
        TrUAPISecretStorage(
            keychain: keychain,
            storeIdProvider: StoreId(value: installation),
            deviceKeys: device,
            lock: lock
        )
    }
}

private struct StoreId: ProductResourceStoreIdProviding {
    let value: String
    func getStoreId() -> String { value }
}

final class TestDeviceKeys: DeviceEncryptionKeyManaging {
    let key = Curve25519.KeyAgreement.PrivateKey()
    private(set) var reads = 0
    func getOrCreatePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey {
        reads += 1
        return key
    }

    func getPublicKey() throws -> Data { key.publicKey.rawRepresentation }
}

private final class MemoryKeychain: KeystoreProtocol, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: Data] = [:]
    var fetchFailure: Error?
    var onCheck: (@Sendable () -> Void)?
    var snapshot: [String: Data] { lock.withLock { values } }

    func addKey(_ key: Data, with identifier: String) throws {
        lock.withLock { values[identifier] = key }
    }

    func updateKey(_ key: Data, with identifier: String) throws {
        lock.withLock { values[identifier] = key }
    }

    func fetchKey(for identifier: String) throws -> Data {
        if let fetchFailure { throw fetchFailure }
        guard let value = lock.withLock({ values[identifier] }) else { throw KeystoreError.noKeyFound }
        return value
    }

    func checkKey(for identifier: String) throws -> Bool {
        onCheck?()
        return lock.withLock { values[identifier] != nil }
    }

    func deleteKey(for identifier: String) throws {
        _ = lock.withLock { values.removeValue(forKey: identifier) }
    }
}

private final class ObservableLock: NSLock, @unchecked Sendable {
    var onLock: (@Sendable () -> Void)?
    override func lock() {
        onLock?()
        super.lock()
    }
}
