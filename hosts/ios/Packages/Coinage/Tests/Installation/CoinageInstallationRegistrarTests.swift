import Clocks
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import Coinage

struct CoinageInstallationRegistrarTests {
    private static let initialBackoff: Duration = .seconds(5)
    private static let maxBackoff: Duration = .seconds(5 * 60)
    private static let expectedRegistrationTime: Duration = .seconds(30)
    private static let target = InstallationRegistrationTarget(contract: TestContracts.contract, installation: .test)

    private let clock = TestClock<Duration>()
    private let engine = FakeRegistrationEngine()
    private let submitter: FakeRegistrationSubmitter
    private let config = StubDataStoreConfig()
    private let backgroundExecutor = CountingBackgroundExecutor()
    private let registrar: CoinageInstallationRegistrar

    init() {
        submitter = FakeRegistrationSubmitter(engine: engine)
        registrar = CoinageInstallationRegistrar(
            currentInstallationStore: StubCurrentInstallationStore(current: .test),
            configProvider: config,
            engine: engine,
            submitter: submitter,
            backgroundExecutor: backgroundExecutor,
            timing: CoinageInstallationRegistrar.Timing(
                expectedRegistrationTime: Self.expectedRegistrationTime,
                initialBackoff: Self.initialBackoff,
                maxBackoff: Self.maxBackoff,
                clock: clock
            ),
            logger: nil
        )
    }

    @Test("a registration already final submits nothing and completes")
    func alreadyFinal() async throws {
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)

        registrar.start()
        try await settle(until: { await (try? status()) == .completed })

        #expect(submitter.attempts.isEmpty)
    }

    @Test("nothing registered yet submits exactly one attempt for this installation")
    func firstAttempt() async throws {
        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })
        await settle()

        #expect(submitter.attempts == [Self.target])
    }

    @Test("a live attempt is never joined by a second one")
    func liveAttempt() async throws {
        let live = DurableTxEntry.registration(Self.target, status: .pending)
        engine.set([live], group: Self.target.registrationGroup)

        registrar.start()
        await settle()
        engine.set([live.changing(status: .pendingSuccess)], group: Self.target.registrationGroup)
        await settle()

        #expect(submitter.attempts.isEmpty)
    }

    @Test("identical group re-emissions do not restart an attempt in flight")
    func reEmissions() async throws {
        submitter.holdAttempts()

        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })
        engine.reEmit(group: Self.target.registrationGroup)
        await settle()
        engine.reEmit(group: Self.target.registrationGroup)
        await settle()
        submitter.openGate()
        await settle()

        #expect(submitter.attempts.count == 1)
    }

    @Test("starting more than once still runs one registration")
    func startTwice() async throws {
        submitter.holdAttempts()

        registrar.start()
        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })
        await settle()
        submitter.openGate()
        await settle()

        #expect(submitter.attempts.count == 1)
    }

    @Test("failures from earlier runs do not delay this run's first attempt")
    func earlierFailures() async throws {
        engine.set(
            (0 ..< 3).map { _ in DurableTxEntry.registration(Self.target, status: .failure) },
            group: Self.target.registrationGroup
        )

        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })

        #expect(submitter.attempts.count == 1)
    }

    @Test("an attempt that fails on chain is followed by exactly one more after a backoff")
    func failureOnChain() async throws {
        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })

        engine.set(
            engine.current(Self.target.registrationGroup).map { $0.changing(status: .failure) },
            group: Self.target.registrationGroup
        )
        await settle()
        #expect(submitter.attempts.count == 1)

        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 2 })

        await clock.advance(by: Self.maxBackoff)
        await settle()
        #expect(submitter.attempts.count == 2)
    }

    @Test("the backoff doubles after each failed attempt")
    func backoffDoubles() async throws {
        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })

        failCurrentAttempts()
        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 2 })

        failCurrentAttempts()
        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        await settle()
        #expect(submitter.attempts.count == 2)

        await clock.advance(by: Self.initialBackoff)
        try await settle(until: { submitter.attempts.count == 3 })
    }

    @Test("each attempt runs under its own background assertion, not the whole run")
    func assertionPerAttempt() async throws {
        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })
        #expect(backgroundExecutor.executions == 1)

        failCurrentAttempts()
        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 2 })

        #expect(backgroundExecutor.executions == 2)
    }

    @Test("an expired background window costs one attempt and the backoff keeps growing")
    func expiredWindowKeepsBackoff() async throws {
        backgroundExecutor.expireNextExecutions(1)

        registrar.start()
        try await settle(until: { backgroundExecutor.executions == 1 })
        #expect(submitter.attempts.isEmpty)

        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 1 })

        // Two attempts have been made, so the next wait is the doubled backoff, not the initial one.
        failCurrentAttempts()
        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        await settle()
        #expect(submitter.attempts.count == 1)

        await clock.advance(by: Self.initialBackoff)
        try await settle(until: { submitter.attempts.count == 2 })
    }

    @Test("an attempt that could not be submitted is retried")
    func submissionFailed() async throws {
        submitter.failNextAttempts(1)

        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })

        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 2 })
    }

    @Test("a changed contract starts a registration of its own")
    func changedContract() async throws {
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)
        config.contract = TestContracts.otherContract

        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })

        #expect(submitter.attempts == [
            InstallationRegistrationTarget(contract: TestContracts.otherContract, installation: .test)
        ])
        #expect(try await status() == .registering)
    }

    @Test("a contract address not yet available is waited for")
    func waitsForContract() async throws {
        config.contract = nil

        registrar.start()
        await settle()
        #expect(submitter.attempts.isEmpty)

        config.contract = TestContracts.contract
        await clock.advance(by: Self.initialBackoff + .milliseconds(1))
        try await settle(until: { submitter.attempts.count == 1 })

        #expect(submitter.attempts == [Self.target])
    }

    @Test("a contract address that never arrives reads as delayed")
    func neverArrives() async throws {
        config.contract = nil

        registrar.start()
        await settle()
        await clock.advance(by: Self.expectedRegistrationTime + .milliseconds(1))
        try await settle(until: { await (try? status()) == .delayed })
    }

    @Test("a registration not final within the expected time reads as delayed")
    func delayed() async throws {
        engine.set([.registration(Self.target, status: .pending)], group: Self.target.registrationGroup)
        registrar.start()
        await settle()

        await clock.advance(by: Self.expectedRegistrationTime - .milliseconds(1))
        await settle()
        #expect(try await status() == .registering)

        await clock.advance(by: .milliseconds(2))
        try await settle(until: { await (try? status()) == .delayed })
    }

    @Test("finality ends the run and clears the warning")
    func finalityClears() async throws {
        let live = DurableTxEntry.registration(Self.target, status: .pending)
        engine.set([live], group: Self.target.registrationGroup)
        registrar.start()
        await settle()
        await clock.advance(by: Self.expectedRegistrationTime + .milliseconds(1))
        try await settle(until: { await (try? status()) == .delayed })

        engine.set([live.changing(status: .finalizedSuccess)], group: Self.target.registrationGroup)
        try await settle(until: { await (try? status()) == .completed })
    }

    @Test("recovery is started so attempts left by a previous process get decided")
    func recoveryStarted() async throws {
        registrar.start()
        try await settle(until: { submitter.attempts.count == 1 })

        #expect(engine.recoveryStarts == 1)
    }
}

private extension CoinageInstallationRegistrarTests {
    func status() async throws -> CoinageAccountBackupStatus? {
        for try await status in registrar.subscribeStatus() {
            return status
        }
        return nil
    }

    /// Lets the registrar's tasks run; every sleep is on the test clock, so this spends no wall time.
    func settle() async {
        for _ in 0 ..< 20 {
            await Task.yield()
        }
    }

    func failCurrentAttempts() {
        engine.set(
            engine.current(Self.target.registrationGroup).map { $0.changing(status: .failure) },
            group: Self.target.registrationGroup
        )
    }

    func settle(until condition: @escaping @Sendable () async -> Bool) async throws {
        for _ in 0 ..< 200 {
            await settle()
            if await condition() { return }
        }
        throw Stalled()
    }

    struct Stalled: Error {}
}
