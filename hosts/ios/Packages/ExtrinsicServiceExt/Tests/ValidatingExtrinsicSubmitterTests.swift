import Testing
import Foundation
import SubstrateSdk
import SubstrateOperation
@testable import ExtrinsicService
@testable import ExtrinsicServiceExt

@Suite("ValidatingExtrinsicSubmitter")
struct ValidatingExtrinsicSubmitterTests {
    @Test("pre-submission invalid blocks submission and triggers recovery")
    func preSubmissionInvalidBlocksSubmission() async throws {
        let context = try makeContext(validity: .invalid(.badProof))

        context.submit()

        try await context.recovery.recovered.wait()
        #expect(context.base.submitAndSubscribeCount == 0, "a doomed extrinsic must never be submitted")
        guard case .preSubmissionValidation = try #require(context.recovery.failures.first) else {
            Issue.record("expected preSubmissionValidation")
            return
        }
    }

    @Test("abort after pre-submission invalid reports the failure to the caller")
    func abortReportsFailureToCaller() async throws {
        let context = try makeContext(validity: .invalid(.badProof))
        let failed = context.statusEvent { if case .failure = $0 { true } else { false } }

        context.submit()

        try await failed.wait()
    }

    @Test("submission error triggers recovery and surfaces the error on abort")
    func submissionErrorTriggersRecovery() async throws {
        let context = try makeContext(validity: .valid)
        let failed = context.statusEvent { result in
            if case let .failure(error) = result, error is TestError { true } else { false }
        }

        context.submit()
        try await context.base.subscribed.wait()

        context.base.emit(.failure(TestError.submissionFailed))

        try await failed.wait()
        guard case .submission = try #require(context.recovery.failures.first) else {
            Issue.record("a base submission error must recover as .submission")
            return
        }
    }

    @Test("a failing validation call fails open and still submits")
    func validationCallFailureFailsOpen() async throws {
        let context = try makeContext(validity: .valid)
        context.api.fallback = .failure(TestError.validationFailed)

        context.submit()

        try await context.base.subscribed.wait()
        #expect(context.recovery.failures.isEmpty, "an inconclusive validation must not trigger recovery")
    }

    @Test("a failing block hash fetch fails open and still submits")
    func blockHashFailureFailsOpen() async throws {
        let context = try makeContext(validity: .invalid(.badProof))
        context.blockInfo.currentHashError = TestError.validationFailed

        context.submit()

        try await context.base.subscribed.wait()
        #expect(context.api.callCount == 0, "validation is unreachable without a block hash")
    }

    @Test("validated bytes are the extrinsic body without its length prefix")
    func validatedBytesAreTheBody() async throws {
        let context = try makeContext(validity: .valid)

        context.submit()

        try await context.base.subscribed.wait()
        #expect(context.api.validatedBodies == [Data([0x01])])
    }

    @Test("the one-shot submit path delegates to the base and forwards its results")
    func submitDelegatesToBase() throws {
        let context = try makeContext(validity: .valid)
        context.base.stubSubmitResults([.success(ExtrinsicSubmittedModel(txHash: "0xhash", sender: .none))])

        var received: [SubmitExtrinsicResult]?
        context.submitter.submit(builtExtrinsics: [context.builtExtrinsic]) { received = $0 }

        #expect(context.base.submitCount == 1, "the one-shot path must delegate to the base submitter")
        let first = try #require(received?.first)
        #expect(try first.get().txHash == "0xhash", "the base's results must reach the caller unchanged")
    }

    @Test("a caller that refuses the handle gets nothing submitted on its behalf")
    func rejectedSubscriptionIdSubmitsNothing() async throws {
        let context = try makeContext(validity: .valid)

        context.submitter.submitAndSubscribe(
            builtExtrinsic: context.builtExtrinsic,
            runningIn: .global(),
            subscriptionIdClosure: { _ in false },
            notificationClosure: { _ in }
        )
        await settle()

        #expect(context.base.submitAndSubscribeCount == 0)
    }

    @Test("a valid extrinsic is delegated to the base")
    func validExtrinsicIsDelegated() async throws {
        let context = try makeContext(validity: .valid)

        context.submit()

        try await context.base.subscribed.wait()
        #expect(context.base.submittedExtrinsics.first?.extrinsic == "0x0401")
        #expect(context.recovery.failures.isEmpty)
    }

    @Test("an inconclusive validity still submits")
    func inconclusiveValidityStillSubmits() async throws {
        let context = try makeContext(validity: .unknown(.cannotLookup))

        context.submit()

        try await context.base.subscribed.wait()
        #expect(context.recovery.failures.isEmpty)
    }

    @Test("pool invalidation recovers as txInvalidation")
    func poolInvalidationTriggersRecovery() async throws {
        let context = try makeContext(validity: .valid)

        context.submit()
        try await context.base.subscribed.wait()

        context.base.emit(.success(makeStatus(.onChain(.invalid))))

        try await context.recovery.recovered.wait()
        guard case .txInvalidation = try #require(context.recovery.failures.first) else {
            Issue.record("expected txInvalidation")
            return
        }
    }

    @Test("a pool drop triggers recovery")
    func droppedExtrinsicTriggersRecovery() async throws {
        let context = try makeContext(validity: .valid)

        context.submit()
        try await context.base.subscribed.wait()

        context.base.emit(.success(makeStatus(.onChain(.dropped))))

        try await context.recovery.recovered.wait()
        guard case .txInvalidation = try #require(context.recovery.failures.first) else {
            Issue.record("expected txInvalidation")
            return
        }
    }

    @Test("usurped ends the attempt instead of leaking the subscription")
    func usurpedEndsTheAttempt() async throws {
        let context = try makeContext(validity: .valid)
        let terminal = context.statusEvent { result in
            guard case let .success(model) = result else { return false }
            return model.statusUpdate.getFinalExtrinsicFailure() != nil
        }

        context.submit()
        try await context.base.subscribed.wait()

        context.base.emit(.success(makeStatus(.onChain(.unsurped("0xother")))))

        try await terminal.wait()
        try await context.base.cancelled.wait()
        #expect(
            context.base.cancelledIds.contains(42),
            "an usurped extrinsic must not leave its base subscription live"
        )
        #expect(
            context.recovery.failures.isEmpty,
            "an usurped extrinsic lost its nonce slot — resubmitting it can only fail"
        )
    }

    @Test("in-block is forwarded without recovery")
    func inBlockIsForwarded() async throws {
        let context = try makeContext(validity: .valid)
        let forwarded = context.statusEvent { result in
            guard case let .success(model) = result else { return false }
            return model.statusUpdate.getInBlockOrFinalizedHash() != nil
        }

        context.submit()
        try await context.base.subscribed.wait()

        context.base.emit(.success(makeStatus(.onChain(.inBlock("0xabc")))))

        try await forwarded.wait()
        #expect(context.recovery.failures.isEmpty)
    }

    @Test("cancel unsubscribes the current base subscription")
    func cancelUnsubscribesBase() async throws {
        let context = try makeContext(validity: .valid)

        context.submit()
        try await context.base.subscribed.wait()

        let handle = try #require(context.reportedIds.first, "wrapper must report a subscription id handle")
        context.submitter.cancelExtrinsicWatch(for: handle)

        #expect(context.base.cancelledIds.contains(42), "cancel must reach the base subscription")
    }

    @Test("the handle is reported before validation so cancel always reaches")
    func handleIsReportedBeforeValidation() throws {
        let context = try makeContext(validity: .valid)

        context.submit()

        #expect(
            context.reportedIds.count == 1,
            "the handle must exist before any validation or submission has run"
        )
    }

    @Test("cancelling during recovery prevents resubmission")
    func cancelDuringRecoveryPreventsResubmission() async throws {
        let context = try makeContext(validity: .invalid(.badProof))
        context.recovery.decision = .resubmit(context.builtExtrinsic)
        context.recovery.onRecover = { [weak context] in
            guard let context, let handle = context.reportedIds.first else {
                Issue.record("no handle to cancel — recovery is unreachable from the caller")
                return
            }
            context.submitter.cancelExtrinsicWatch(for: handle)
        }

        context.submit()
        await settle(for: .milliseconds(500))

        #expect(context.base.submitAndSubscribeCount == 0, "a cancelled watch must never resubmit")
    }

    @Test("resubmission keeps the handle stable and cancels the latest base subscription")
    func resubmitKeepsHandleStable() async throws {
        let context = try makeContext(validity: .valid)
        try await resubmitOnce(context)

        #expect(context.reportedIds.count == 1, "the caller-visible handle must not change on resubmission")

        let handle = try #require(context.reportedIds.first)
        context.submitter.cancelExtrinsicWatch(for: handle)

        #expect(
            context.base.cancelledIds.contains(57),
            "cancel must unsubscribe the current base subscription, not the first one"
        )
    }

    @Test("resubmission does not re-emit created")
    func resubmissionDoesNotReemitCreated() async throws {
        let context = try makeContext(validity: .valid)
        let created = context.statusEvent(isCreated)

        context.submit()
        try await created.wait()

        context.recovery.decision = .resubmit(context.builtExtrinsic)
        context.base.emit(.success(makeStatus(.onChain(.invalid))))

        try await context.base.subscribed.wait(occurrences: 2)
        await settle()

        #expect(created.occurrences == 1, "the caller must see created once, not once per attempt")
    }

    @Test("created is still delivered when the first attempt never reached the base")
    func createdDeliveredWhenFirstAttemptBlocked() async throws {
        let context = try makeContext(validity: .invalid(.badProof))
        context.recovery.decision = .resubmit(context.builtExtrinsic)
        let created = context.statusEvent(isCreated)

        context.submit()

        try await created.wait()
    }
}

private extension ValidatingExtrinsicSubmitterTests {
    enum TestError: Error {
        case submissionFailed
        case validationFailed
    }

    final class Context: @unchecked Sendable {
        let submitter: ValidatingExtrinsicSubmitter
        let base: ExtrinsicSubmittingMock
        let recovery: ExtrinsicSubmissionRecoveringMock
        let api: TaggedTransactionQueueApiMock
        let blockInfo: BlockInfoProvidingMock
        let builtExtrinsic: ExtrinsicBuiltModel

        typealias StatusResult = Result<ExtrinsicSubscribedStatusModel, Error>

        private let mutex = NSLock()
        private var storedReportedIds: [UInt16] = []
        private var statusMatchers: [(predicate: (StatusResult) -> Bool, event: TestEvent)] = []

        func statusEvent(_ predicate: @escaping (StatusResult) -> Bool) -> TestEvent {
            let event = TestEvent()
            mutex.withLock { statusMatchers.append((predicate, event)) }
            return event
        }

        var reportedIds: [UInt16] {
            mutex.withLock { storedReportedIds }
        }

        init(
            submitter: ValidatingExtrinsicSubmitter,
            base: ExtrinsicSubmittingMock,
            recovery: ExtrinsicSubmissionRecoveringMock,
            api: TaggedTransactionQueueApiMock,
            blockInfo: BlockInfoProvidingMock,
            builtExtrinsic: ExtrinsicBuiltModel
        ) {
            self.submitter = submitter
            self.base = base
            self.recovery = recovery
            self.api = api
            self.blockInfo = blockInfo
            self.builtExtrinsic = builtExtrinsic
        }

        deinit {
            reportedIds.forEach { submitter.cancelExtrinsicWatch(for: $0) }
        }

        private func handleStatus(_ result: StatusResult) {
            let matched = mutex.withLock {
                statusMatchers.filter { $0.predicate(result) }.map(\.event)
            }
            matched.forEach { $0.signal() }
        }

        func submit() {
            submitter.submitAndSubscribe(
                builtExtrinsic: builtExtrinsic,
                runningIn: .global(),
                subscriptionIdClosure: { [weak self] id in
                    self?.mutex.withLock { self?.storedReportedIds.append(id) }
                    return true
                },
                notificationClosure: { [weak self] result in
                    self?.handleStatus(result)
                }
            )
        }
    }

    func makeContext(validity: TransactionValidity) throws -> Context {
        let api = TaggedTransactionQueueApiMock()
        api.fallback = .validity(validity)

        let base = ExtrinsicSubmittingMock()
        let recovery = ExtrinsicSubmissionRecoveringMock()
        let header = try BlockInfoProvidingMock.makeHeader()
        let blockInfo = BlockInfoProvidingMock(header: header, headTicks: 0)

        let submitter = ValidatingExtrinsicSubmitter(
            base: base,
            validationApi: api,
            recovery: recovery,
            blockInfoProvider: blockInfo,
            chainId: "test-chain",
            logger: nil
        )

        return Context(
            submitter: submitter,
            base: base,
            recovery: recovery,
            api: api,
            blockInfo: blockInfo,
            builtExtrinsic: ExtrinsicBuiltModel(extrinsic: "0x0401", sender: .none, mortality: .immortal)
        )
    }

    func isCreated(_ result: Result<ExtrinsicSubscribedStatusModel, Error>) -> Bool {
        guard case let .success(model) = result else { return false }
        if case .created = model.statusUpdate.extrinsicStatus { return true }
        return false
    }

    func makeStatus(_ status: ExtrinsicStatus) -> ExtrinsicSubscribedStatusModel {
        ExtrinsicSubscribedStatusModel(
            statusUpdate: ExtrinsicStatusUpdate(extrinsicHash: "0xdeadbeef", extrinsicStatus: status),
            sender: .none
        )
    }

    func resubmitOnce(_ context: Context) async throws {
        context.submit()
        try await context.base.subscribed.wait()

        context.recovery.decision = .resubmit(context.builtExtrinsic)
        context.base.emit(.success(makeStatus(.onChain(.invalid))))

        try await context.base.subscribed.wait(occurrences: 2)
    }
}
