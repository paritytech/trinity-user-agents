import Foundation
import Foundation_iOS
import KeyDerivation
import Keystore_iOS
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIWalletSecretProviderTests {
    @Test func selectionIsPublishedOnlyAfterTheRootExists() throws {
        let keychain = ObservedKeychain()
        let selection = ObservedSelection()
        let roots = RootEntropyManager(keychain: keychain, installationKeyIdStore: selection)
        let first = Data(repeating: 1, count: 16)
        try roots.createRootEntropy(first)
        let firstId = try #require(selection.getInstallationKeyId())
        let second = Data(repeating: 2, count: 16)
        selection.onSave = { walletId in
            let stored = try roots.fetchRootEntropy(installationKeyId: walletId)
            #expect(stored == second)
        }
        defer { selection.onSave = nil }
        try roots.createRootEntropy(second)
        let secondId = try #require(selection.getInstallationKeyId())
        #expect(firstId != secondId)
        #expect(try roots.fetchRootEntropy(installationKeyId: firstId) == first)
        keychain.failWrites = true
        #expect(throws: KeystoreError.self) { try roots.createRootEntropy(Data(repeating: 3, count: 16)) }
        #expect(selection.getInstallationKeyId() == secondId)
        #expect(try roots.fetchRootEntropy() == second)
    }

    @Test func requestedWalletCannotReadAnotherSelectedRoot() async throws {
        let keychain = ObservedKeychain()
        let selection = ObservedSelection()
        let roots = RootEntropyManager(keychain: keychain, installationKeyIdStore: selection)
        try roots.createRootEntropy(Data(repeating: 1, count: 16))
        let firstId = try #require(selection.getInstallationKeyId())
        let second = Data(repeating: 2, count: 16)
        try roots.createRootEntropy(second)
        let secondId = try #require(selection.getInstallationKeyId())
        let provider = TrUAPIWalletSecretProvider(entropyManager: roots, installationKeyIdStore: selection)
        await #expect(throws: HostRejection.self) { try await provider.readWalletRootEntropy(walletId: firstId) }
        #expect(try await provider.readWalletRootEntropy(walletId: secondId) == second)
        selection.saveInstallationKeyId("missing")
        await #expect(throws: HostRejection.self) { try await provider.readWalletRootEntropy(walletId: "missing") }
    }

    @Test func selectionChangingDuringTheProtectedReadRejectsTheResult() async throws {
        let keychain = ObservedKeychain()
        let selection = ObservedSelection()
        let roots = RootEntropyManager(keychain: keychain, installationKeyIdStore: selection)
        try roots.createRootEntropy(Data(repeating: 1, count: 16))
        let walletId = try #require(selection.getInstallationKeyId())
        keychain.onRead = { selection.saveInstallationKeyId("replacement") }
        let provider = TrUAPIWalletSecretProvider(entropyManager: roots, installationKeyIdStore: selection)
        await #expect(throws: HostRejection.self) { try await provider.readWalletRootEntropy(walletId: walletId) }
    }
}

@Suite(.serialized)
final class TrUAPIHostRuntimeReadinessTests {
    private let defaults: UserDefaults
    private let suiteName = "io.polkadotapp.tests.wallet-readiness.\(UUID().uuidString)"
    private let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    private let keychain = ObservedKeychain()

    init() throws {
        defaults = try #require(UserDefaults(suiteName: suiteName))
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    deinit {
        defaults.removePersistentDomain(forName: suiteName)
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func cancellingOneCallerDoesNotCancelSharedActivation() async throws {
        let provider = try makeProvider()
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        keychain.onRead = {
            entered.signal()
            precondition(release.wait(timeout: .now() + 10) == .success)
        }
        let first = Task { try await provider.sharedRuntime() }
        #expect(await waitForSignal(entered) == .success)
        let second = Task { try await provider.sharedRuntime() }
        first.cancel()
        release.signal()
        await #expect(throws: CancellationError.self) { try await first.value }
        let runtime = try await second.value
        #expect(try await provider.sharedRuntime() === runtime)
        #expect(keychain.reads == 1)
    }

    @Test func selectionChangeLocksTheActiveWalletBeforeReplacementFailure() async throws {
        let selection = InstallationKeyIdStore(userDefaults: defaults)
        let provider = try makeProvider(selection: selection)
        let runtime = try await provider.sharedRuntime()
        #expect(try runtime.statementRenewalOwnerKey().count == 32)
        selection.saveInstallationKeyId("missing")
        #expect(throws: HostRejection.self) { try runtime.statementRenewalOwnerKey() }
        await #expect(throws: (any Error).self) { try await provider.sharedRuntime() }
        #expect(throws: HostRejection.self) { try runtime.statementRenewalOwnerKey() }
        #expect(keychain.reads == 2)
    }

    private func makeProvider(selection: InstallationKeyIdStoring = ObservedSelection()) throws
        -> TrUAPIHostRuntimeProvider {
        let roots = RootEntropyManager(keychain: keychain, installationKeyIdStore: selection)
        try roots.createRootEntropy(Data(repeating: 7, count: 16))
        let registry = MockChainRegistry()
        for (index, chainId) in [
            AppConfig.Chains.usernameChain,
            AppConfig.Chains.bulletInChain,
            AppConfig.Chains.assethubChain
        ].enumerated() {
            registry.chainsById[chainId] = SsoTestData.makeChain(
                genesisHash: Data(repeating: UInt8(index + 1), count: 32).toHex(),
                chainId: chainId
            )
        }
        return TrUAPIHostRuntimeProvider(
            chainRegistry: registry,
            entropyManager: roots,
            settingsManager: InMemorySettingsManager(),
            coreStorage: CoreStorageBackend(
                storage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults),
                lock: NSLock()
            ),
            secretStorage: StubSecretStorage(),
            installationKeyIdStore: selection,
            confirmationRouterFacade: ProductRoutersFacade.sso(),
            tldProvider: StubDotNsTldProvider(tld: "paseo"),
            databaseDirectory: { [directory] in directory.path },
            contactDataProviderFactory: MockChatContactDataProviderFactory(),
            logger: Logger.shared
        )
    }

    private func waitForSignal(_ semaphore: DispatchSemaphore) async -> DispatchTimeoutResult {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async {
                continuation.resume(returning: semaphore.wait(timeout: .now() + 10))
            }
        }
    }
}

private final class ObservedSelection: InstallationKeyIdStoring, @unchecked Sendable {
    private var walletId: String?
    var onSave: ((String) throws -> Void)?

    func saveInstallationKeyId(_ walletId: String) {
        do { try onSave?(walletId) } catch { Issue.record(error) }
        self.walletId = walletId
    }

    func getInstallationKeyId() -> String? { walletId }
}

private final class ObservedKeychain: KeystoreProtocol {
    private let storage = InMemoryKeychain()
    var failWrites = false
    var onRead: (() -> Void)?
    private let lock = NSLock()
    private var readCount = 0
    var reads: Int { lock.withLock { readCount } }

    func addKey(_ key: Data, with identifier: String) throws {
        if failWrites { throw KeystoreError.unexpectedFail }
        try storage.addKey(key, with: identifier)
    }

    func updateKey(_ key: Data, with identifier: String) throws {
        if failWrites { throw KeystoreError.unexpectedFail }
        try storage.updateKey(key, with: identifier)
    }

    func fetchKey(for identifier: String) throws -> Data {
        lock.withLock { readCount += 1 }
        let value = try storage.fetchKey(for: identifier)
        onRead?()
        return value
    }

    func checkKey(for identifier: String) throws -> Bool { try storage.checkKey(for: identifier) }
    func deleteKey(for identifier: String) throws { try storage.deleteKey(for: identifier) }
}
