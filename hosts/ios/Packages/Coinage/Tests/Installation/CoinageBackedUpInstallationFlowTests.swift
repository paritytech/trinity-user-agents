import Clocks
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import Coinage

/// The gate over the real registrar: an allocation requested before the contract address is even
/// known waits through the delayed state and is released by finality, with the wait reported
/// exactly while it lasts.
struct CoinageBackedUpInstallationFlowTests {
    private static let target = InstallationRegistrationTarget(contract: TestContracts.contract, installation: .test)
    private static let timing = CoinageInstallationRegistrar.Timing(
        expectedRegistrationTime: .seconds(30),
        initialBackoff: .seconds(5),
        maxBackoff: .seconds(5 * 60),
        clock: TestClock<Duration>()
    )

    private let clock: TestClock<Duration>
    private let engine = FakeRegistrationEngine()
    private let submitter: FakeRegistrationSubmitter
    private let config = StubDataStoreConfig(contract: nil)
    private let store = StubCurrentInstallationStore(current: .test)
    private let registrar: CoinageInstallationRegistrar
    private let provider: CoinageBackedUpInstallationProvider

    init() {
        clock = Self.timing.clock as! TestClock<Duration>
        submitter = FakeRegistrationSubmitter(engine: engine)
        registrar = CoinageInstallationRegistrar(
            currentInstallationStore: store,
            configProvider: config,
            engine: engine,
            submitter: submitter,
            backgroundExecutor: CountingBackgroundExecutor(),
            timing: Self.timing,
            logger: nil
        )
        provider = CoinageBackedUpInstallationProvider(installationStore: store, backupObserver: registrar)
    }

    @Test("an allocation waits from a missing contract address through delayed until finality")
    func waitsUntilFinality() async throws {
        registrar.start()
        let request = Task { try await provider.backedUpInstallation() }
        try await settle(until: { (try? await awaiting()) == true })

        await clock.advance(by: Self.timing.expectedRegistrationTime + .milliseconds(1))
        try await settle(until: { await (try? status()) == .delayed })
        #expect(try await awaiting() == true)
        #expect(request.isCancelled == false)

        config.contract = TestContracts.contract
        await clock.advance(by: Self.timing.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 1 })
        engine.set(
            engine.current(Self.target.registrationGroup).map { $0.changing(status: .finalizedSuccess) },
            group: Self.target.registrationGroup
        )

        try await settle(until: { await (try? status()) == .completed })
        #expect(try await request.value == .test)
        try await settle(until: { (try? await awaiting()) == false })
    }

    @Test("once final, later allocations never wait")
    func noWaitAfterFinality() async throws {
        config.contract = TestContracts.contract
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)
        registrar.start()
        try await settle(until: { await (try? status()) == .completed })

        #expect(try await provider.backedUpInstallation() == .test)
        #expect(try await awaiting() == false)
    }
}

private extension CoinageBackedUpInstallationFlowTests {
    func status() async throws -> CoinageAccountBackupStatus? {
        for try await status in registrar.subscribeStatus() {
            return status
        }
        return nil
    }

    func awaiting() async throws -> Bool? {
        for try await awaiting in provider.subscribeAwaitingBackup() {
            return awaiting
        }
        return nil
    }

    func settle(until condition: @escaping @Sendable () async -> Bool) async throws {
        for _ in 0 ..< 200 {
            for _ in 0 ..< 20 {
                await Task.yield()
            }
            if await condition() { return }
        }
        throw Stalled()
    }

    struct Stalled: Error {}
}
