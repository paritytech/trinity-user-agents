import AsyncExtensions
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import StructuredConcurrencyTestSupport
import Testing
@testable import Coinage

/// The gate over the real registrar: an allocation requested before the contract address is even
/// known waits through the delayed state and is released by finality, with the wait reported
/// exactly while it lasts.
@Suite(.timeLimit(.minutes(5)))
struct CoinageBackedUpInstallationFlowTests {
    private static let target = InstallationRegistrationTarget(contract: TestContracts.contract, installation: .test)
    private static let expectedRegistrationTime: Duration = .seconds(30)
    private static let initialBackoff: Duration = .seconds(5)

    private let clock = ManualClock()
    private let engine = FakeRegistrationEngine()
    private let submitter: FakeRegistrationSubmitter
    private let config = StubDataStoreConfig(contract: nil)
    private let store = StubCurrentInstallationStore(current: .test)
    private let registrar: CoinageInstallationRegistrar
    private let provider: CoinageBackedUpInstallationProvider

    init() {
        submitter = FakeRegistrationSubmitter(engine: engine)
        registrar = CoinageInstallationRegistrar(
            currentInstallationStore: store,
            configProvider: config,
            engine: engine,
            submitter: submitter,
            backgroundExecutor: CountingBackgroundExecutor(),
            timing: CoinageInstallationRegistrar.Timing(
                expectedRegistrationTime: Self.expectedRegistrationTime,
                initialBackoff: Self.initialBackoff,
                maxBackoff: .seconds(5 * 60),
                clock: clock
            ),
            logger: nil
        )
        provider = CoinageBackedUpInstallationProvider(installationStore: store, backupObserver: registrar)
    }

    @Test("an allocation waits from a missing contract address through delayed until finality")
    func waitsUntilFinality() async throws {
        registrar.start()
        let request = Task { try await provider.backedUpInstallation() }
        try await firstValue(of: provider.subscribeAwaitingBackup()) { $0 }

        await clock.resumeSleep(for: Self.expectedRegistrationTime)
        try await firstValue(of: registrar.subscribeStatus()) { $0 == .delayed }
        #expect(try await awaiting() == true)

        // The overdue timer runs apart from the run, so only a pending retry proves the address was read as missing.
        await clock.waitForSleep(for: Self.initialBackoff)
        config.contract = TestContracts.contract
        await clock.resumeSleep(for: Self.initialBackoff)
        let attempts = engine.subscribeGroupEntries(
            domain: .coinageInstallation,
            groupId: Self.target.registrationGroup
        )
        try await firstValue(of: attempts) { !$0.isEmpty }
        engine.set(
            engine.current(Self.target.registrationGroup).map { $0.changing(status: .finalizedSuccess) },
            group: Self.target.registrationGroup
        )

        #expect(try await request.value == .test)
        try await firstValue(of: provider.subscribeAwaitingBackup()) { !$0 }
    }

    @Test("a registration final on an earlier launch never reports a wait, even before the run starts")
    func noWaitBeforeStart() async throws {
        config.contract = TestContracts.contract
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)

        #expect(try await provider.backedUpInstallation() == .test)
        #expect(try await awaiting() == false)
    }

    @Test("once final, later allocations never wait")
    func noWaitAfterFinality() async throws {
        config.contract = TestContracts.contract
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)
        registrar.start()
        try await firstValue(of: registrar.subscribeStatus()) { $0 == .completed }

        #expect(try await provider.backedUpInstallation() == .test)
        #expect(try await awaiting() == false)
    }
}

private extension CoinageBackedUpInstallationFlowTests {
    func awaiting() async throws -> Bool? {
        for try await awaiting in provider.subscribeAwaitingBackup() {
            return awaiting
        }
        return nil
    }

    func firstValue<Element>(
        of sequence: AnyAsyncSequence<Element>,
        where predicate: (Element) -> Bool
    ) async throws {
        for try await value in sequence where predicate(value) {
            return
        }
        Issue.record("sequence finished before the expected value")
    }
}
