import Clocks
import Foundation
import FoundationExt
import Individuality
import IssueMonitoring
import MessageExchangeKit
import Testing
@testable import polkadot_app

@Suite(.timeLimit(.minutes(10)))
struct ChatAccountFundingTests {
    private let accountId = Data(repeating: 0x01, count: 32)
    private let funding = FakeStatementAccountFunding()
    private let dateProvider = MockDateProvider()
    private let peerAccountId = Data(repeating: 0x0A, count: 32)
    private let signerManager = ChatSignerManager(
        entropyManager: FixedRootEntropyManager(),
        tldProvider: StubDotNsTldProvider(tld: "dot")
    )
    private let fallback = FakeIdentitySigningFallback(
        signer: StubStatementSigner(accountId: Data(repeating: 0x0C, count: 32))
    )

    @Test func signsWithFundedPerChatAccountOfThePeer() async throws {
        try await funding.recordOnFund(period: currentPeriod())
        let expected = try signerManager.makeSigner(with: peerAccountId).accountId

        let signer = try await makeGate().signer(for: sessionRequest())

        #expect(signer.accountId == expected)
        #expect(funding.calls == [expected])
    }

    @Test func fallsBackToIdentityWhenNoSlotCanBeEvicted() async throws {
        funding.fail(times: 1, with: StatementStoreAllowanceError.noEvictableSlots)

        let signer = try await makeGate().signer(for: sessionRequest())

        #expect(signer.accountId == fallback.signer.accountId)
        #expect(fallback.contacts == [peerAccountId])
    }

    @Test func skipsAllocationForRestOfPeriodOnceNoSlotCanBeEvicted() async throws {
        funding.fail(times: 1, with: StatementStoreAllowanceError.noEvictableSlots)
        let gate = makeGate()

        _ = try await gate.signer(for: sessionRequest())
        _ = try await gate.signer(for: sessionRequest())

        #expect(funding.calls.count == 1)
        #expect(fallback.contacts.count == 2)
    }

    @Test func cooldownKeepsBackoffWithoutFallback() async throws {
        try await funding.recordOnFund(period: currentPeriod())
        funding.fail(times: 2, with: StatementStoreAllowanceError.noSlotsAvailable(secsToWait: 60))

        let signer = try await makeGate(clock: ImmediateClock()).signer(for: sessionRequest())

        #expect(try signer.accountId == signerManager.makeSigner(with: peerAccountId).accountId)
        #expect(funding.calls.count == 3)
        #expect(fallback.contacts.isEmpty)
    }

    @Test func skipsFundingWhileRecordCoversCurrentPeriod() async throws {
        try await funding.setRecord(period: currentPeriod())

        try await makeGate().awaitFunded(accountId: accountId)

        #expect(funding.calls.isEmpty)
    }

    @Test func fundsOnceWhenRecordIsStale() async throws {
        let period = try await currentPeriod()
        funding.setRecord(period: period - 1)
        funding.recordOnFund(period: period)
        let gate = makeGate()

        try await gate.awaitFunded(accountId: accountId)
        try await gate.awaitFunded(accountId: accountId)

        #expect(funding.calls.count == 1)
        #expect(funding.priorities == [.critical])
    }

    @Test func concurrentSubmissionsShareOneFunding() async throws {
        try await funding.recordOnFund(period: currentPeriod())
        let gate = makeGate()
        funding.holdNext()

        async let first: Void = gate.awaitFunded(accountId: accountId)
        async let second: Void = gate.awaitFunded(accountId: accountId)

        while funding.calls.isEmpty {
            await Task.yield()
        }
        funding.release()

        _ = try await (first, second)
        #expect(funding.calls.count == 1)
    }

    @Test func retriesWithBackoffUntilFunded() async throws {
        try await funding.recordOnFund(period: currentPeriod())
        funding.fail(times: 2)

        try await makeGate(clock: ImmediateClock()).awaitFunded(accountId: accountId)

        #expect(funding.calls.count == 3)
    }

    @Test func cancellationStopsRetrying() async throws {
        funding.fail(times: .max)
        let gate = makeGate()

        let submission = Task { [accountId] in try await gate.awaitFunded(accountId: accountId) }
        while funding.calls.isEmpty {
            await Task.yield()
        }
        submission.cancel()

        await #expect(throws: CancellationError.self) { try await submission.value }
        #expect(funding.calls.count == 1)
    }
}

private extension ChatAccountFundingTests {
    func currentPeriod() async throws -> UInt32 {
        await UInt32(dateProvider.read().timeIntervalSince1970 / .secondsInDay)
    }

    func sessionRequest() -> MessageExchange.SessionRequest {
        MessageExchange.SessionRequest(
            own: .init(signKeyId: "identity", encryptionKeyId: "", pin: nil),
            peer: .init(accountId: peerAccountId, publicKey: Data(repeating: 0x0B, count: 32), pin: nil, devices: [])
        )
    }

    func makeGate(clock: any Clock<Duration> = ContinuousClock()) -> ChatAccountFunding {
        ChatAccountFunding(
            funding: funding,
            signerManager: signerManager,
            identityFallback: fallback,
            dateProvider: dateProvider,
            issueDiagnostics: .make(reporter: NoopIssueReporter()),
            clock: clock
        )
    }
}
