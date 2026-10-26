import AsyncExtensions
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import StructuredConcurrencyTestSupport
import Testing
@testable import Coinage

@Suite(.timeLimit(.minutes(5)))
struct CoinageInstallationRegistrarTests {
    private static let initialBackoff: Duration = .seconds(5)
    private static let maxBackoff: Duration = .seconds(5 * 60)
    private static let expectedRegistrationTime: Duration = .seconds(30)
    private static let target = InstallationRegistrationTarget(contract: TestContracts.contract, installation: .test)

    private let clock = ManualClock()
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
        try await waitForStatus(.completed)

        #expect(submitter.attempts.isEmpty)
    }

    @Test("a registration already final reads completed first, before the run starts")
    func alreadyFinalBeforeStart() async throws {
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)

        #expect(try await status() == .completed)
    }

    @Test("nothing registered yet reads registering before the run starts")
    func nothingRegisteredBeforeStart() async throws {
        #expect(try await status() == .registering)
    }

    @Test("a registration final under a contract that then changes registers again")
    func contractChangedAfterEarlierRead() async throws {
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)
        #expect(try await status() == .completed)

        config.contract = TestContracts.otherContract
        registrar.start()
        try await waitForAttempts(1)

        #expect(try await status() == .registering)
    }

    @Test("nothing registered yet submits exactly one attempt for this installation")
    func firstAttempt() async throws {
        registrar.start()
        try await waitForAttempts(1)

        try await finalizeAfterSubscriptions(2)

        #expect(submitter.attempts == [Self.target])
    }

    @Test("a live attempt is never joined by a second one")
    func liveAttempt() async throws {
        let live = DurableTxEntry.registration(Self.target, status: .pending)
        engine.set([live], group: Self.target.registrationGroup)

        registrar.start()
        try await waitForSubscriptions(1)
        engine.set([live.changing(status: .pendingSuccess)], group: Self.target.registrationGroup)
        engine.set([live.changing(status: .finalizedSuccess)], group: Self.target.registrationGroup)
        try await waitForStatus(.completed)

        #expect(submitter.attempts.isEmpty)
    }

    @Test("identical group re-emissions do not restart an attempt in flight")
    func reEmissions() async throws {
        submitter.holdAttempts()

        registrar.start()
        try await waitForAttempts(1)
        engine.reEmit(group: Self.target.registrationGroup)
        engine.reEmit(group: Self.target.registrationGroup)
        submitter.openGate()

        try await finalizeAfterSubscriptions(2)

        #expect(submitter.attempts.count == 1)
    }

    @Test("starting more than once still runs one registration")
    func startTwice() async throws {
        submitter.holdAttempts()

        registrar.start()
        registrar.start()
        try await waitForAttempts(1)
        submitter.openGate()

        try await finalizeAfterSubscriptions(2)

        #expect(submitter.attempts.count == 1)
    }

    @Test("failures from earlier runs do not delay this run's first attempt")
    func earlierFailures() async throws {
        engine.set(
            (0 ..< 3).map { _ in DurableTxEntry.registration(Self.target, status: .failure) },
            group: Self.target.registrationGroup
        )

        registrar.start()
        try await waitForAttempts(1)

        #expect(submitter.attempts.count == 1)
    }

    @Test("an attempt that fails on chain is followed by exactly one more after a backoff")
    func failureOnChain() async throws {
        registrar.start()
        try await waitForAttempts(1)

        try await failCurrentAttempts(afterSubscriptions: 2)
        await clock.waitForSleep(for: Self.initialBackoff)
        #expect(submitter.attempts.count == 1)

        await clock.resumeSleep(for: Self.initialBackoff)
        try await waitForAttempts(2)

        try await finalizeAfterSubscriptions(3)
        #expect(submitter.attempts.count == 2)
    }

    @Test("the backoff doubles after each failed attempt")
    func backoffDoubles() async throws {
        registrar.start()
        try await waitForAttempts(1)

        try await failCurrentAttempts(afterSubscriptions: 2)
        await clock.resumeSleep(for: Self.initialBackoff)
        try await waitForAttempts(2)

        try await failCurrentAttempts(afterSubscriptions: 3)
        await clock.waitForSleep(for: Self.initialBackoff * 2)
        #expect(submitter.attempts.count == 2)

        await clock.resumeSleep(for: Self.initialBackoff * 2)
        try await waitForAttempts(3)
    }

    @Test("each attempt runs under its own background assertion, not the whole run")
    func assertionPerAttempt() async throws {
        registrar.start()
        try await waitForAttempts(1)
        #expect(backgroundExecutor.executions == 1)

        try await failCurrentAttempts(afterSubscriptions: 2)
        await clock.resumeSleep(for: Self.initialBackoff)
        try await waitForAttempts(2)

        #expect(backgroundExecutor.executions == 2)
    }

    @Test("an expired background window costs one attempt and the backoff keeps growing")
    func expiredWindowKeepsBackoff() async throws {
        backgroundExecutor.expireNextExecutions(1)

        registrar.start()
        await clock.waitForSleep(for: Self.initialBackoff)
        #expect(backgroundExecutor.executions == 1)
        #expect(submitter.attempts.isEmpty)

        await clock.resumeSleep(for: Self.initialBackoff)
        try await waitForAttempts(1)

        // Two attempts have been made, so the next wait is the doubled backoff, not the initial one.
        try await failCurrentAttempts(afterSubscriptions: 3)
        await clock.waitForSleep(for: Self.initialBackoff * 2)
        #expect(submitter.attempts.count == 1)

        await clock.resumeSleep(for: Self.initialBackoff * 2)
        try await waitForAttempts(2)
    }

    @Test("an attempt that could not be submitted is retried")
    func submissionFailed() async throws {
        submitter.failNextAttempts(1)

        registrar.start()
        try await waitForAttempts(1)

        await clock.resumeSleep(for: Self.initialBackoff)
        try await waitForAttempts(2)
    }

    @Test("a changed contract starts a registration of its own")
    func changedContract() async throws {
        engine.set([.registration(Self.target, status: .finalizedSuccess)], group: Self.target.registrationGroup)
        config.contract = TestContracts.otherContract

        registrar.start()
        try await waitForAttempts(1)

        #expect(submitter.attempts == [
            InstallationRegistrationTarget(contract: TestContracts.otherContract, installation: .test)
        ])
        #expect(try await status() == .registering)
    }

    @Test("a contract address not yet available is waited for")
    func waitsForContract() async throws {
        config.contract = nil

        registrar.start()
        await clock.waitForSleep(for: Self.initialBackoff)
        #expect(submitter.attempts.isEmpty)

        config.contract = TestContracts.contract
        await clock.resumeSleep(for: Self.initialBackoff)
        try await waitForAttempts(1)

        #expect(submitter.attempts == [Self.target])
    }

    @Test("a contract address that never arrives reads as delayed")
    func neverArrives() async throws {
        config.contract = nil

        registrar.start()
        await clock.resumeSleep(for: Self.expectedRegistrationTime)

        try await waitForStatus(.delayed)
    }

    @Test("a registration not final within the expected time reads as delayed")
    func delayed() async throws {
        engine.set([.registration(Self.target, status: .pending)], group: Self.target.registrationGroup)
        registrar.start()

        await clock.waitForSleep(for: Self.expectedRegistrationTime)
        #expect(try await status() == .registering)

        await clock.resumeSleep(for: Self.expectedRegistrationTime)
        try await waitForStatus(.delayed)
    }

    @Test("finality ends the run and clears the warning")
    func finalityClears() async throws {
        let live = DurableTxEntry.registration(Self.target, status: .pending)
        engine.set([live], group: Self.target.registrationGroup)
        registrar.start()
        await clock.resumeSleep(for: Self.expectedRegistrationTime)
        try await waitForStatus(.delayed)

        engine.set([live.changing(status: .finalizedSuccess)], group: Self.target.registrationGroup)
        try await waitForStatus(.completed)
    }

    @Test("recovery is started so attempts left by a previous process get decided")
    func recoveryStarted() async throws {
        registrar.start()
        try await waitForAttempts(1)

        #expect(engine.recoveryStarts == 1)
    }
}

private extension CoinageInstallationRegistrarTests {
    func status() async throws -> CoinageAccountBackupStatus? {
        try await registrar.subscribeStatus().first { _ in true }
    }

    func waitForStatus(_ expected: CoinageAccountBackupStatus) async throws {
        let status = try await registrar.subscribeStatus().first { $0 == expected }
        #expect(status == expected)
    }

    func waitForAttempts(_ count: Int) async throws {
        let attempts = try await submitter.subscribeAttempts().first { $0.count >= count }
        #expect(attempts?.count == count)
    }

    /// The registrar subscribes afresh after every attempt, so the count marks how far the run has got.
    func waitForSubscriptions(_ count: Int) async throws {
        let subscriptions = try await engine.subscribeSubscriptionCount().first { $0 >= count }
        #expect(subscriptions != nil)
    }

    /// Finality ends the run, so attempts counted after completion are all the run ever made.
    func finalizeAfterSubscriptions(_ count: Int) async throws {
        try await waitForSubscriptions(count)
        engine.set(
            engine.current(Self.target.registrationGroup).map { $0.changing(status: .finalizedSuccess) },
            group: Self.target.registrationGroup
        )
        try await waitForStatus(.completed)
    }

    func failCurrentAttempts(afterSubscriptions count: Int) async throws {
        try await waitForSubscriptions(count)
        engine.set(
            engine.current(Self.target.registrationGroup).map { $0.changing(status: .failure) },
            group: Self.target.registrationGroup
        )
    }
}
