import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import DurableTransactions

/// The one place a failure is allowed to become a retry instead of a terminal row. What matters is the
/// asymmetry: a policy has to actively say yes, and everything that could stop it answering — no policy
/// on the row, an unregistered id, an unreadable read — leaves the failure standing.
@Suite("Verdict Writer")
struct VerdictWriterTests {
    private let policyId = SubmissionPolicyId("test-policy")

    @Test("a non-failure verdict is written as given")
    func successPassesThrough() async throws {
        let store = InMemoryDurableTxRepository()
        let writer = makeWriter(store: store)
        let entry = try store.givenPending()

        let wrote = try await writer.write(entry, Verdict(status: .finalizedSuccess, successDetectedAt: .fixture(101)))

        #expect(wrote)
        #expect(store.statusSnapshot(of: entry.id) == .finalizedSuccess)
    }

    @Test("a failure on a row with no policy stays a failure")
    func failureWithoutPolicyIsTerminal() async throws {
        let store = InMemoryDurableTxRepository()
        let writer = makeWriter(store: store)
        let entry = try store.givenPending()

        let wrote = try await writer.write(entry, .dispatchFailure)

        #expect(wrote)
        #expect(store.statusSnapshot(of: entry.id) == .failure)
    }

    @Test("a policy attached at registration survives, so an eager submission can still retry")
    func policyOnRegistrationIsPersisted() async throws {
        // A claim is built and submitted eagerly — it is registered, never scheduled — and still carries a
        // policy. Dropping it at the persistence boundary makes every one of those failures terminal.
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        policy.answerRetry(true)
        let writer = makeWriter(store: store, policy: policy)
        let params = Data([4, 2])

        let id = try await store.registerOne(policyId: policyId, params: params)
        let entry = try #require(try await store.getEntry(id: id))

        #expect(try await store.getSubmissionPolicy(id: id) == SubmissionPolicy(id: policyId, params: params))

        let wrote = try await writer.write(entry, .dispatchFailure)

        #expect(wrote)
        #expect(store.statusSnapshot(of: id) == .pendingSubmission)
    }

    @Test("a policy that wants it back sends it to pendingSubmission instead")
    func retryDefersToPendingSubmission() async throws {
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        policy.answerRetry(true)
        let writer = makeWriter(store: store, policy: policy)
        let entry = try await store.givenBuiltFromSchedule(policyId: policyId)

        let wrote = try await writer.write(entry, .dispatchFailure)

        #expect(wrote)
        #expect(store.statusSnapshot(of: entry.id) == .pendingSubmission)
    }

    @Test("a policy that declines lets the failure stand")
    func declinedRetryIsTerminal() async throws {
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        policy.answerRetry(false)
        let writer = makeWriter(store: store, policy: policy)
        let entry = try await store.givenBuiltFromSchedule(policyId: policyId)

        let wrote = try await writer.write(entry, .dispatchFailure)

        #expect(wrote)
        #expect(store.statusSnapshot(of: entry.id) == .failure)
    }

    @Test("a row naming an unregistered policy fails rather than waiting for one")
    func unregisteredPolicyIsTerminal() async throws {
        let store = InMemoryDurableTxRepository()
        // Registry left empty: the row names a policy nothing can answer for.
        let writer = DurableVerdictWriter(
            store: store,
            policies: DurableSubmissionPolicyRegistry(),
            logger: nil
        )
        let entry = try await store.givenBuiltFromSchedule(policyId: policyId)

        let wrote = try await writer.write(entry, .dispatchFailure)

        #expect(wrote)
        #expect(store.statusSnapshot(of: entry.id) == .failure)
    }

    @Test("the policy is asked about the failure it actually suffered", arguments: DurableFailureKind.allCases)
    func retryQuestionCarriesFailureAndParams(_ failure: DurableFailureKind) async throws {
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        policy.answerRetry(false)
        let writer = makeWriter(store: store, policy: policy)
        let params = Data([7, 8, 9])
        let entry = try await store.givenBuiltFromSchedule(policyId: policyId, params: params)

        _ = try await writer.write(entry, Verdict(status: .failure, successDetectedAt: nil, failure: failure))

        let question = try #require(policy.retryQuestions.first)
        #expect(question.id == entry.id)
        #expect(question.failure == failure)
        #expect(question.params == params)
    }

    @Test("a failure with no kind is never offered to a policy")
    func failureWithoutKindSkipsThePolicy() async throws {
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        policy.answerRetry(true)
        let writer = makeWriter(store: store, policy: policy)
        let entry = try await store.givenBuiltFromSchedule(policyId: policyId)

        let wrote = try await writer.write(entry, Verdict(status: .failure, successDetectedAt: nil))

        #expect(wrote)
        #expect(store.statusSnapshot(of: entry.id) == .failure)
        #expect(policy.retryQuestions.isEmpty)
    }

    @Test("a row with no attempt is not this writer's to decide")
    func scheduledRowIsNotWritten() async throws {
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        policy.answerRetry(true)
        let writer = makeWriter(store: store, policy: policy)
        let id = try await store.scheduleOne(policyId: policyId)
        let entry = try #require(try await store.getEntry(id: id))

        let wrote = try await writer.write(entry, .dispatchFailure)

        #expect(!wrote)
        #expect(store.statusSnapshot(of: id) == .pendingSubmission)
        #expect(policy.retryQuestions.isEmpty)
    }

    @Test("a verdict about a superseded attempt is refused")
    func staleAttemptIsRefused() async throws {
        let store = InMemoryDurableTxRepository()
        let writer = makeWriter(store: store)
        let stale = try store.givenPending()

        // The row is rebuilt onto different bytes while the stale verdict is still in flight.
        try store.forceStatus(stale.id, to: .pendingSubmission)
        try await store.startAttemptFixture(id: stale.id, payload: "rebuilt")

        let wrote = try await writer.write(stale, .dispatchFailure)

        #expect(!wrote)
        #expect(store.statusSnapshot(of: stale.id) == .pending)
    }

    @Test("a verdict about a status that has moved on is refused")
    func staleStatusIsRefused() async throws {
        let store = InMemoryDurableTxRepository()
        let writer = makeWriter(store: store)
        let observed = try store.givenPending()
        try store.forceStatus(observed.id, to: .pendingSuccess)

        let wrote = try await writer.write(observed, .dispatchFailure)

        #expect(!wrote)
        #expect(store.statusSnapshot(of: observed.id) == .pendingSuccess)
    }
}

// MARK: - Support

private extension VerdictWriterTests {
    func makeWriter(
        store: InMemoryDurableTxRepository,
        policy: ScriptedSubmissionPolicy? = nil
    ) -> DurableVerdictWriter {
        let registry = DurableSubmissionPolicyRegistry()
        if let policy {
            registry.register(policy, for: policyId)
        }

        return DurableVerdictWriter(store: store, policies: registry, logger: nil)
    }
}

private extension Verdict {
    /// The verdict the tracker proposes for an extrinsic that was included and whose dispatch failed.
    static var dispatchFailure: Verdict {
        Verdict(status: .failure, successDetectedAt: nil, failure: .dispatchFailed)
    }
}

extension InMemoryDurableTxRepository {
    /// A registered, pending transaction with an attempt — a row mid-flight.
    func givenPending(payload: String = "first") throws -> DurableTxEntry {
        let attempt = try DurableTxAttempt(from: .fixture(payload: payload))
        let entry = DurableTxEntry(domainId: .test, attempt: attempt, status: .pending)
        insert(entry)

        return try #require(allEntries.first { $0.id == entry.id })
    }

    /// Registers one already-built transaction carrying a policy — the eager-submission shape.
    func registerOne(
        policyId: SubmissionPolicyId,
        params: Data = Data(),
        payload: String = "first"
    ) async throws -> DurableTxId {
        let ids = try await register(
            [DurableTxRegistration(
                domainId: .test,
                groupId: nil,
                attempt: DurableTxAttempt(from: .fixture(payload: payload)),
                policy: SubmissionPolicy(id: policyId, params: params)
            )],
            onRegister: { _, _ in }
        )

        return try #require(ids.first)
    }

    /// Schedules one transaction and returns its id, leaving it `pendingSubmission`.
    func scheduleOne(
        policyId: SubmissionPolicyId,
        groupId: DurableTxGroupId? = nil,
        params: Data = Data(),
        domainId: TxDomainId = .test
    ) async throws -> DurableTxId {
        let ids = try await schedule(
            [DurableTxSchedule(
                domainId: domainId,
                groupId: groupId,
                policy: SubmissionPolicy(id: policyId, params: params)
            )],
            in: nil,
            onRegister: { _, _ in }
        )

        return try #require(ids.first)
    }

    /// Schedules a transaction and gives it its first attempt, which is the state a policy-owned row is
    /// in while it is in flight.
    func givenBuiltFromSchedule(
        policyId: SubmissionPolicyId,
        params: Data = Data(),
        payload: String = "first"
    ) async throws -> DurableTxEntry {
        let id = try await scheduleOne(policyId: policyId, params: params)
        _ = try await startAttemptFixture(id: id, payload: payload)

        return try #require(try await getEntry(id: id))
    }

    @discardableResult
    func startAttemptFixture(id: DurableTxId, payload: String) async throws -> Bool {
        try await startAttempt(id: id, attempt: DurableTxAttempt(from: .fixture(payload: payload)))
    }
}
