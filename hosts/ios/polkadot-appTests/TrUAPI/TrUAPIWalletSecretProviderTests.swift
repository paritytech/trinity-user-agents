import Foundation
import Foundation_iOS
import KeyDerivation
import Keystore_iOS
import Testing
import Products
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

    @Test func constructionSurvivesCallerCancellationAndSelectionReplacement() async throws {
        let selection = InstallationKeyIdStore(userDefaults: defaults)
        let protectedKeys = ObservedKeychain()
        let secrets = makeSecretStorage(keychain: protectedKeys)
        let provider = try makeProvider(selection: selection, secretStorage: secrets)
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        protectedKeys.onWrite = {
            if protectedKeys.writes == 1 {
                entered.signal()
                precondition(release.wait(timeout: .now() + 10) == .success)
            }
        }
        defer { protectedKeys.onWrite = nil }
        let first = Task { try await provider.sharedRuntime() }
        #expect(await waitForSignal(entered) == .success)
        first.cancel()
        let roots = RootEntropyManager(keychain: keychain, installationKeyIdStore: selection)
        try roots.createRootEntropy(Data(repeating: 8, count: 16))
        let second = Task { try await provider.sharedRuntime() }
        release.signal()
        await #expect(throws: CancellationError.self) { try await first.value }
        let runtime = try await second.value
        #expect(try runtime.statementRenewalOwnerKey().count == 32)
        #expect(try await provider.sharedRuntime() === runtime)
        #expect(protectedKeys.reads == 1)
        #expect(protectedKeys.writes == 1)
        #expect(keychain.reads == 1)
    }

    @Test func failedConstructionRetriesAfterProtectedWriteFailure() async throws {
        let protectedKeys = ObservedKeychain()
        let provider = try makeProvider(secretStorage: makeSecretStorage(keychain: protectedKeys))
        protectedKeys.failWrites = true
        await #expect {
            try await provider.sharedRuntime()
        } throws: { error in
            guard case let NativeRuntimeConfigError.DatabaseUnavailable(reason) = error else { return false }
            return reason == "database protection failed: KeystoreError"
        }
        #expect(keychain.reads == 0)
        protectedKeys.failWrites = false
        let runtime = try await provider.sharedRuntime()
        #expect(try runtime.statementRenewalOwnerKey().count == 32)
        #expect(try await provider.sharedRuntime() === runtime)
        #expect(protectedKeys.reads == 2)
        #expect(protectedKeys.writes == 2)
        #expect(keychain.reads == 1)
    }

    @Test func resetClosesRetainedRuntimeBeforeReopeningAnEmptyInstallation() async throws {
        let protectedKeys = ObservedKeychain()
        let secrets = makeSecretStorage(keychain: protectedKeys)
        let selection = InstallationKeyIdStore(userDefaults: defaults)
        let provider = try makeProvider(selection: selection, secretStorage: secrets)
        let walletId = try #require(selection.getInstallationKeyId())
        let runtime = try await provider.sharedRuntime()
        try await runtime.setPermissionRecord(
            productId: "calendar.paseo", request: .device(.camera), status: .authorized
        )
        provider.lockWallet()
        var saved = runtime.permissionRecords().makeAsyncIterator()
        #expect(try await saved.next()?.count == 1)
        try await provider.resetData()
        #expect(!FileManager.default.fileExists(atPath: directory.path))
        await #expect(throws: TrUAPIRuntimeConfigError.self) { try await provider.constructedRuntime() }
        await #expect(throws: HostRejection.self) {
            try await runtime.activateWallet(walletId: walletId, liteUsername: nil)
        }
        await #expect(throws: HostRejection.self) {
            try await runtime.setPermissionRecord(
                productId: "calendar.paseo", request: .device(.camera), status: .authorized
            )
        }
        await #expect(throws: HostRejection.self) { try await secrets.read(key: .storageEncryptionKey) }
        defaults.removePersistentDomain(forName: suiteName)
        let fresh = try makeProvider()
        let freshRuntime = try await fresh.sharedRuntime()
        await #expect(throws: HostRejection.self) {
            try await secrets.write(key: .storageEncryptionKey, value: Data(repeating: 1, count: 32))
        }
        #expect(try freshRuntime.statementRenewalOwnerKey().count == 32)
        var records = freshRuntime.permissionRecords().makeAsyncIterator()
        #expect(try await records.next() == [])
        try await provider.resetData()
        #expect(FileManager.default.fileExists(atPath: directory.appendingPathComponent("core.sqlite3").path))
        try await fresh.resetData()
    }

    @Test func resetWithoutConstructionDoesNotReadProtectedStorage() async throws {
        let protectedKeys = ObservedKeychain()
        let provider = try makeProvider(secretStorage: makeSecretStorage(keychain: protectedKeys))
        try await provider.resetData()
        #expect(protectedKeys.reads == 0)
        #expect(protectedKeys.writes == 0)
        #expect(keychain.reads == 0)
        #expect(!FileManager.default.fileExists(atPath: directory.path))
        await #expect(throws: TrUAPIRuntimeConfigError.self) { try await provider.sharedRuntime() }
    }

    @Test(arguments: [false, true])
    func resetWaitsForConstructionAndCannotPublishTheRetiredRuntime(failWrite: Bool) async throws {
        let protectedKeys = ObservedKeychain()
        let secrets = makeSecretStorage(keychain: protectedKeys)
        let provider = try makeProvider(secretStorage: secrets)
        protectedKeys.failWrites = failWrite
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        protectedKeys.onWrite = {
            entered.signal()
            precondition(release.wait(timeout: .now() + 10) == .success)
        }
        let construction = Task { try await provider.constructedRuntime() }
        #expect(await waitForSignal(entered) == .success)
        defer { release.signal() }
        provider.lockWallet()
        let reset = Task { try await provider.resetData() }
        let deadline = Date().addingTimeInterval(5)
        var retired = false
        while !retired, Date() < deadline {
            do {
                _ = try await provider.sharedRuntime()
                Issue.record("A locked provider must not activate during reset")
                break
            } catch TrUAPIRuntimeConfigError.walletLocked {
                await Task.yield()
            } catch TrUAPIRuntimeConfigError.runtimeRetired {
                retired = true
            }
        }
        #expect(retired)
        reset.cancel()
        release.signal()
        try await reset.value
        _ = await construction.result
        #expect(protectedKeys.writes == 1)
        #expect(keychain.reads == 0)
        #expect(!FileManager.default.fileExists(atPath: directory.path))
        await #expect(throws: TrUAPIRuntimeConfigError.self) { try await provider.constructedRuntime() }
        await #expect(throws: HostRejection.self) {
            try await secrets.write(key: .storageEncryptionKey, value: Data(repeating: 1, count: 32))
        }
    }

    @Test func resetCanRetryDirectoryFailureWithoutReactivatingTheRuntime() async throws {
        let failure = ResetDirectoryFailure()
        let provider = try makeProvider(databaseDirectory: { [directory] in
            try failure.resolve(directory.path)
        })
        let runtime = try await provider.sharedRuntime()
        failure.fail = true
        await #expect(throws: CocoaError.self) { try await provider.resetData() }
        #expect(FileManager.default.fileExists(atPath: directory.path))
        #expect(throws: HostRejection.self) { try runtime.statementRenewalOwnerKey() }
        await #expect(throws: TrUAPIRuntimeConfigError.self) { try await provider.constructedRuntime() }
        failure.fail = false
        try await provider.resetData()
        #expect(!FileManager.default.fileExists(atPath: directory.path))
    }

    @Test(.timeLimit(.minutes(1)))
    @MainActor
    func lockedSettingsObserveAndResetWithoutWalletActivation() async throws {
        let provider = try makeProvider()
        provider.lockWallet()
        let runtime = try await provider.constructedRuntime()
        let permissions = try AppPermissionSettings(runtimeEnabled: true, runtimeProvider: provider)
        let notification = PermissionRecord(
            productId: "unresolved.paseo",
            request: .device(.notifications),
            status: .authorized
        )
        let account = PermissionRecord(
            productId: "unresolved.paseo",
            request: .accountAccess(targetProductId: "other"),
            status: .authorized
        )
        try await runtime.setPermissionRecord(
            productId: notification.productId, request: notification.request, status: notification.status
        )
        try await runtime.setPermissionRecord(
            productId: account.productId,
            request: account.request,
            status: account.status
        )
        try await runtime.setPermissionRecord(productId: "denied.paseo", request: .device(.camera), status: .denied)
        try await runtime.setPermissionRecord(productId: "z.paseo", request: .device(.camera), status: .authorized)
        let apps = AppsListInteractor(permissions: permissions, productResolver: FailingSettingsProductResolver())
        let appsOutput = SettingsAppsOutput()
        apps.presenter = appsOutput
        defer { withExtendedLifetime(apps) {} }
        var appUpdates = appsOutput.updates.makeAsyncIterator()
        apps.setup()
        #expect(await appUpdates.next() == ["unresolved.paseo", "z.paseo"])
        #expect(await appUpdates.next() == ["unresolved.paseo", "z.paseo"])

        let scheduler = MockNotificationScheduler()
        let (cancelled, cancellation) = AsyncStream.makeStream(of: String.self)
        scheduler.onCancelAll = { cancellation.yield($0) }
        var cancellations = cancelled.makeAsyncIterator()
        let interactor = AppPermissionsInteractor(
            productId: "unresolved.paseo", permissions: permissions, notificationScheduler: scheduler
        )
        let output = SettingsPermissionsOutput()
        interactor.presenter = output
        var updates = output.updates.makeAsyncIterator()
        interactor.setup()
        let records = try #require(await updates.next())
        let saved = records.compactMap { record -> PermissionRecord? in
            guard case let .rust(record) = record else { return nil }
            return record
        }
        #expect(Set(saved) == Set([notification, account]))
        interactor.revokeOnDisappear(records: [.rust(notification)])
        #expect(await cancellations.next() == "unresolved.paseo")
        #expect(await appUpdates.next() == ["unresolved.paseo", "z.paseo"])
        #expect(await appUpdates.next() == ["unresolved.paseo", "z.paseo"])
        var persisted = runtime.permissionRecords(productId: "unresolved.paseo").makeAsyncIterator()
        #expect(try await persisted.next() == [account])
        scheduler.onCancelAll = { _ in Issue.record("Account reset must not cancel notifications") }
        interactor.revokeOnDisappear(records: [.rust(account)])
        #expect(try await persisted.next() == [])
        #expect(await appUpdates.next() == ["z.paseo"])
        #expect(keychain.reads == 0)
        #expect(throws: HostRejection.self) { try runtime.statementRenewalOwnerKey() }
        #expect(try await provider.constructedRuntime() === runtime)
        await #expect(throws: TrUAPIRuntimeConfigError.self) { try await provider.sharedRuntime() }
    }

    @Test(.timeLimit(.minutes(1)))
    @MainActor
    func settingsSurfaceConstructionFailureWithoutReadingWalletSecrets() async throws {
        let protectedKeys = ObservedKeychain()
        protectedKeys.failWrites = true
        let provider = try makeProvider(secretStorage: makeSecretStorage(keychain: protectedKeys))
        provider.lockWallet()
        let permissions = try AppPermissionSettings(runtimeEnabled: true, runtimeProvider: provider)
        let scheduler = MockNotificationScheduler()
        scheduler.onCancelAll = { _ in Issue.record("A failed reset must not cancel notifications") }
        let interactor = AppPermissionsInteractor(
            productId: "product",
            permissions: permissions,
            notificationScheduler: scheduler
        )
        defer { withExtendedLifetime(interactor) {} }
        let output = SettingsPermissionsOutput()
        interactor.presenter = output
        var errors = output.errors.makeAsyncIterator()
        interactor.setup()
        let error = try #require(await errors.next())
        guard case NativeRuntimeConfigError.DatabaseUnavailable = error else {
            Issue.record("Unexpected construction failure: \(error)")
            return
        }
        interactor.revokeOnDisappear(records: [.rust(.init(
            productId: "product",
            request: .device(.notifications),
            status: .authorized
        ))])
        let resetError = try #require(await errors.next())
        guard case NativeRuntimeConfigError.DatabaseUnavailable = resetError else {
            Issue.record("Unexpected reset failure: \(resetError)")
            return
        }
        #expect(keychain.reads == 0)
    }

    private func makeSecretStorage(keychain: ObservedKeychain) -> TrUAPISecretStorage {
        TrUAPISecretStorage(
            keychain: keychain,
            storeIdProvider: ProductResourceStoreIdStore(userDefaults: defaults),
            deviceKeys: TestDeviceKeys(),
            lock: NSLock()
        )
    }

    private func makeProvider(
        selection: InstallationKeyIdStoring = ObservedSelection(),
        secretStorage: TrUAPISecretStorage? = nil,
        databaseDirectory: (@Sendable () throws -> String)? = nil
    ) throws -> TrUAPIHostRuntimeProvider {
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
            secretStorage: secretStorage ?? makeSecretStorage(keychain: ObservedKeychain()),
            installationKeyIdStore: selection,
            confirmationRouterFacade: ProductRoutersFacade.sso(),
            tldProvider: StubDotNsTldProvider(tld: "paseo"),
            databaseDirectory: databaseDirectory ?? { [directory] in directory.path },
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
    var onWrite: (() -> Void)?
    private let lock = NSLock()
    private var readCount = 0
    private var writeCount = 0
    var reads: Int { lock.withLock { readCount } }
    var writes: Int { lock.withLock { writeCount } }

    func addKey(_ key: Data, with identifier: String) throws {
        lock.withLock { writeCount += 1 }
        onWrite?()
        if failWrites { throw KeystoreError.unexpectedFail }
        try storage.addKey(key, with: identifier)
    }

    func updateKey(_ key: Data, with identifier: String) throws {
        lock.withLock { writeCount += 1 }
        onWrite?()
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

@MainActor
private final class SettingsAppsOutput: AppsListInteractorOutputProtocol {
    let updates: AsyncStream<[String]>
    private let continuation: AsyncStream<[String]>.Continuation

    init() {
        (updates, continuation) = AsyncStream.makeStream(of: [String].self)
    }

    func didReceive(products: [ResolvedProduct]) { continuation.yield(products.map(\.id)) }
    func didReceive(error: Error) { Issue.record(error) }
}

@MainActor
private final class SettingsPermissionsOutput: AppPermissionsInteractorOutputProtocol {
    let updates: AsyncStream<[AppPermissionRecord]>
    let errors: AsyncStream<Error>
    private let continuation: AsyncStream<[AppPermissionRecord]>.Continuation
    private let errorContinuation: AsyncStream<Error>.Continuation

    init() {
        (updates, continuation) = AsyncStream.makeStream(of: [AppPermissionRecord].self)
        (errors, errorContinuation) = AsyncStream.makeStream(of: Error.self)
    }

    func didReceive(grants: [AppPermissionRecord]) { continuation.yield(grants) }
    func didReceive(error: Error) { errorContinuation.yield(error) }
}

private final class FailingSettingsProductResolver: ProductResolving, Sendable {
    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        throw HostRejection.Rejected(reason: "No manifest for \(productId)")
    }
}

private final class ResetDirectoryFailure: @unchecked Sendable {
    var fail = false

    func resolve(_ path: String) throws -> String {
        if fail { throw CocoaError(.fileWriteNoPermission) }
        return path
    }
}
