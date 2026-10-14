import Foundation
import os
import Testing
@testable import Coinage

struct CoinageBackupRecoveryServiceTests {
    private static let batchSize: DerivationIndex = 500
    private static let emptyBatches: DerivationIndex = 4
    private static let deepBatches: DerivationIndex = 10
    private static let previous = CoinageInstallationId.fixed(0x01)
    private static let anotherPrevious = CoinageInstallationId.fixed(0x02)

    private let installations = InMemoryInstallations()
    private let dataStore = StubDataStoreRepository()
    private let scanner = StubScanner()
    private let assetStore = RecordingAssetStore()
    private let service: CoinageBackupRecoveryService

    init() {
        service = CoinageBackupRecoveryService(
            currentInstallationStore: StubCurrentInstallationStore(current: .test),
            installationRepository: installations,
            configProvider: StubDataStoreConfig(),
            dataStoreRepository: dataStore,
            scanner: scanner,
            assetStore: assetStore,
            discoveryRetryDelay: .milliseconds(1),
            logger: nil
        )
    }

    @Test("start runs the launch pass once and ignores later calls")
    func startRunsOnce() async throws {
        dataStore.listed([Self.previous])

        await service.start()
        await service.start()
        await service.awaitLaunchPass()

        #expect(dataStore.reads == 1)
        #expect(scanner.scannedInstallations == [Self.previous])
    }

    @Test("the current installation is never scanned")
    func currentNeverScanned() async throws {
        dataStore.listed([.test, Self.previous])

        await service.runLaunchPass()

        #expect(scanner.scannedInstallations == [Self.previous])
    }

    @Test("vouchers found under a previous installation keep that installation and their absolute index")
    func recoveredVouchersKeepIndex() async throws {
        dataStore.listed([Self.previous])
        scanner.vouchersOnChain = [CoinageKeyIndex(installation: Self.previous, item: 77)]

        await service.runLaunchPass()

        #expect(assetStore.savedVouchers.map(\.derivationIndex) == [CoinageKeyIndex(
            installation: Self.previous,
            item: 77
        )])
        #expect(assetStore.savedCoins.isEmpty)
    }

    @Test("coins found under a previous installation keep that installation and their absolute index")
    func recoveredCoinsKeepIndex() async throws {
        dataStore.listed([Self.previous])
        scanner.coinsOnChain = [CoinageKeyIndex(installation: Self.previous, item: 1_234)]

        await service.runLaunchPass()

        #expect(assetStore.savedCoins.map(\.derivationIndex) == [CoinageKeyIndex(
            installation: Self.previous,
            item: 1_234
        )])
    }

    @Test("the scan stops after four empty batches in a row, not four in total")
    func gapRule() async throws {
        dataStore.listed([Self.previous])
        // Non-empty batches 0, 3 and 6; with a counter that never reset, batch 6 would be missed.
        scanner.coinsOnChain = Set([10, 1_600, 3_100].map { CoinageKeyIndex(installation: Self.previous, item: $0) })

        await service.runLaunchPass()

        #expect(assetStore.savedCoins.map(\.derivationIndex.item) == [10, 1_600, 3_100])
        let expectedNext = 3_100 / Self.batchSize * Self.batchSize + (Self.emptyBatches + 1) * Self.batchSize
        #expect(installations.previous(Self.previous)?.coinScanNextIndex == expectedNext)
    }

    @Test("a scanned installation is not scanned again on the next launch")
    func scannedOnce() async throws {
        dataStore.listed([Self.previous])
        await service.runLaunchPass()
        scanner.clearScanned()

        await service.runLaunchPass()

        #expect(scanner.scannedInstallations.isEmpty)
    }

    @Test("an installation registered after the last launch is scanned on this one")
    func newInstallationScanned() async throws {
        dataStore.listed([Self.previous])
        await service.runLaunchPass()
        scanner.clearScanned()

        dataStore.listed([Self.previous, Self.anotherPrevious])
        await service.runLaunchPass()

        #expect(scanner.scannedInstallations == [Self.anotherPrevious])
    }

    @Test("a failed contract read still scans installations already known")
    func failedContractRead() async throws {
        try await installations.addPrevious([Self.previous])
        dataStore.failing()

        await service.runLaunchPass()

        #expect(scanner.scannedInstallations == [Self.previous])
        #expect(dataStore.reads == CoinageBackupRecoveryService.Config.discoveryAttempts)
    }

    @Test("a contract that could not be read, with nothing known, reports failure rather than completion")
    func failedContractReadWithNothingKnown() async throws {
        dataStore.failing()

        await service.runLaunchPass()

        let reported = try #require(await progress())
        #expect(reported == .initial(.failed))
        #expect(!reported.awaitsAcknowledgement)
        #expect(dataStore.reads == CoinageBackupRecoveryService.Config.discoveryAttempts)
    }

    @Test("a listing that fails once is read again before the launch gives up on it")
    func transientContractReadFailure() async throws {
        dataStore.listed([Self.previous])
        dataStore.failingTransiently(times: 1)

        await service.runLaunchPass()

        #expect(dataStore.reads == 2)
        #expect(scanner.scannedInstallations == [Self.previous])
        #expect(try await progress() == .initial(.completed))
    }

    @Test("a contract address still on its way is not a failed read")
    func contractAddressUnavailable() async throws {
        let service = CoinageBackupRecoveryService(
            currentInstallationStore: StubCurrentInstallationStore(current: .test),
            installationRepository: installations,
            configProvider: StubDataStoreConfig(contract: nil),
            dataStoreRepository: dataStore,
            scanner: scanner,
            assetStore: assetStore,
            discoveryRetryDelay: .milliseconds(1),
            logger: nil
        )

        await service.runLaunchPass()

        #expect(dataStore.reads == 0)
        for try await progress in service.subscribeProgress() {
            #expect(progress == .completed)
            break
        }
    }

    @Test("an installation whose scan failed is retried on the next launch")
    func failedScanRetried() async throws {
        dataStore.listed([Self.previous])
        scanner.failing = true
        await service.runLaunchPass()
        #expect(installations.previous(Self.previous)?.initialScanCompleted == false)
        #expect(try await progress() == .initial(.failed))

        scanner.failing = false
        await service.runLaunchPass()

        #expect(installations.previous(Self.previous)?.initialScanCompleted == true)
    }

    @Test("a deep search that could not read the chain reports the failure instead of a balance")
    func failedDeepSearch() async throws {
        dataStore.listed([Self.previous])
        await service.runLaunchPass()
        scanner.failing = true

        await service.deepSearch()

        let reported = try #require(await progress())
        #expect(reported == .deep(.failed))
        #expect(!reported.awaitsAcknowledgement)
    }

    @Test("with previous installations recovered the user is asked to confirm the balance")
    func asksToConfirm() async throws {
        dataStore.listed([Self.previous])

        await service.runLaunchPass()

        #expect(try await progress() == .initial(.completed))
    }

    @Test("with nothing to recover there is nothing to confirm")
    func nothingToConfirm() async throws {
        dataStore.listed([.test])

        await service.runLaunchPass()

        #expect(try await progress() == .completed)
        #expect(assetStore.savedCoins.isEmpty)
    }

    @Test("deep search continues every previous installation from where it stopped")
    func deepSearch() async throws {
        dataStore.listed([Self.previous, Self.anotherPrevious])
        await service.runLaunchPass()
        let resumeFrom = try #require(installations.previous(Self.previous)?.coinScanNextIndex)

        await service.deepSearch()

        let expected = resumeFrom + Self.deepBatches * Self.batchSize
        #expect(installations.previous(Self.previous)?.coinScanNextIndex == expected)
        #expect(installations.previous(Self.anotherPrevious)?.coinScanNextIndex == expected)
        #expect(try await progress() == .deep(.completed))
    }

    @Test("accepting the balance confirms every known installation and completes recovery")
    func markAsCompleted() async throws {
        dataStore.listed([Self.previous])
        await service.runLaunchPass()

        await service.markAsCompleted()

        #expect(try await progress() == .completed)
        #expect(installations.previous(Self.previous)?.isUserConfirmedCompletion == true)
    }

    @Test("an installation found after the confirmation is shown again, the confirmed one is not")
    func newInstallationAfterConfirmation() async throws {
        dataStore.listed([Self.previous])
        await service.runLaunchPass()
        await service.markAsCompleted()

        dataStore.listed([Self.previous, Self.anotherPrevious])
        await service.runLaunchPass()

        #expect(try await progress() == .initial(.completed))
        #expect(installations.previous(Self.previous)?.awaitsUserConfirmation == false)
        #expect(installations.previous(Self.anotherPrevious)?.awaitsUserConfirmation == true)
    }

    @Test("a deep search never changes what the user has to confirm")
    func deepSearchKeepsConfirmationState() async throws {
        dataStore.listed([Self.previous])
        await service.runLaunchPass()

        await service.deepSearch()

        #expect(installations.previous(Self.previous)?.awaitsUserConfirmation == true)
    }

    @Test("an installation whose first scan failed is not offered for confirmation")
    func failedScanNotOffered() async throws {
        dataStore.listed([Self.previous])
        scanner.failing = true

        await service.runLaunchPass()

        #expect(installations.previous(Self.previous)?.awaitsUserConfirmation == false)
    }
}

private extension CoinageBackupRecoveryServiceTests {
    func progress() async throws -> BackupProgress? {
        for try await progress in service.subscribeProgress() {
            return progress
        }
        return nil
    }
}

// MARK: - Fakes

private final class StubScanner: InstallationAssetScanning, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<(
        coins: Set<CoinageKeyIndex>,
        vouchers: Set<CoinageKeyIndex>,
        failing: Bool,
        scanned: [CoinageInstallationId]
    )>(
        initialState: ([], [], false, [])
    )

    var coinsOnChain: Set<CoinageKeyIndex> {
        get { state.withLock { $0.coins } }
        set { state.withLock { $0.coins = newValue } }
    }

    var vouchersOnChain: Set<CoinageKeyIndex> {
        get { state.withLock { $0.vouchers } }
        set { state.withLock { $0.vouchers = newValue } }
    }

    var failing: Bool {
        get { state.withLock { $0.failing } }
        set { state.withLock { $0.failing = newValue } }
    }

    /// Distinct, in first-seen order.
    var scannedInstallations: [CoinageInstallationId] {
        state.withLock { scanned in
            var seen: Set<CoinageInstallationId> = []
            return scanned.scanned.filter { seen.insert($0).inserted }
        }
    }

    func clearScanned() {
        state.withLock { $0.scanned = [] }
    }

    func scanCoins(
        installation: CoinageInstallationId,
        startIndex: DerivationIndex,
        count: DerivationIndex
    ) async throws -> [Coin] {
        let (coins, failing) = state.withLock { state -> (Set<CoinageKeyIndex>, Bool) in
            state.scanned.append(installation)
            return (state.coins, state.failing)
        }
        if failing { throw InstallationStubError.nodeWentAway }

        return (startIndex ..< startIndex + count)
            .map { CoinageKeyIndex(installation: installation, item: $0) }
            .filter { coins.contains($0) }
            .map { key in
                Coin(
                    exponent: 1,
                    derivationIndex: key,
                    age: 0,
                    isOnchain: true,
                    publicKey: Data([UInt8(truncatingIfNeeded: key.item)])
                )
            }
    }

    func scanVouchers(
        installation: CoinageInstallationId,
        startIndex: DerivationIndex,
        count: DerivationIndex
    ) async throws -> [Voucher] {
        let (vouchers, failing) = state.withLock { state -> (Set<CoinageKeyIndex>, Bool) in
            state.scanned.append(installation)
            return (state.vouchers, state.failing)
        }
        if failing { throw InstallationStubError.nodeWentAway }

        return (startIndex ..< startIndex + count)
            .map { CoinageKeyIndex(installation: installation, item: $0) }
            .filter { vouchers.contains($0) }
            .map { key in
                Voucher(
                    exponent: 1,
                    derivationIndex: key,
                    allocatedAt: .now,
                    readyAt: .distantPast,
                    remoteState: .onboarding,
                    publicKey: Data([UInt8(truncatingIfNeeded: key.item)])
                )
            }
    }
}

private final class RecordingAssetStore: RecoveredAssetStoring, @unchecked Sendable {
    private let coins = OSAllocatedUnfairLock<[Coin]>(initialState: [])
    private let vouchers = OSAllocatedUnfairLock<[Voucher]>(initialState: [])

    var savedCoins: [Coin] { coins.withLock { $0 } }
    var savedVouchers: [Voucher] { vouchers.withLock { $0 } }

    func saveNew(coins: [Coin]) async throws {
        self.coins.withLock { $0.append(contentsOf: coins) }
    }

    func saveNew(vouchers: [Voucher]) async throws {
        self.vouchers.withLock { $0.append(contentsOf: vouchers) }
    }
}
