import Testing
import Foundation
import SubstrateSdk
import SubstrateOperation
@testable import ExtrinsicService
@testable import ExtrinsicServiceExt

@Suite("ResubmitWhenValidRecovery")
struct ResubmitWhenValidRecoveryTests {
    @Test("resubmits as soon as the runtime reports valid")
    func resubmitsWhenValid() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.responses = [.validity(.valid)]

        let decision = try await makeRecovery(api: api, ticks: 3).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .txInvalidation
        )

        expectResubmit(decision)
        #expect(api.callCount == 1)
    }

    @Test("waits while invalid or unknown, then resubmits")
    func waitsThenResubmits() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.responses = [
            .validity(.invalid(.future)),
            .validity(.unknown(.cannotLookup)),
            .validity(.valid)
        ]

        let decision = try await makeRecovery(api: api, ticks: 5).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .txInvalidation
        )

        expectResubmit(decision)
        #expect(api.callCount == 3)
    }

    @Test("aborts once mortality has expired", arguments: [InvalidTransaction.stale, .ancientBirthBlock])
    func abortsWhenMortalityExpired(reason: InvalidTransaction) async throws {
        let api = TaggedTransactionQueueApiMock()
        api.responses = [.validity(.invalid(reason))]

        let decision = try await makeRecovery(api: api, ticks: 5).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .txInvalidation
        )

        expectAbort(decision, "expected abort for \(reason)")
        #expect(api.callCount == 1)
    }

    @Test("aborts once maxAttempts is reached")
    func abortsOnMaxAttempts() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.fallback = .validity(.invalid(.future))

        let decision = try await makeRecovery(api: api, ticks: 5, maxAttempts: 2).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .txInvalidation
        )

        expectAbort(decision)
        #expect(api.callCount == 2)
    }

    @Test("a validation error is inconclusive and waits")
    func validationErrorWaits() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.responses = [.failure(TestError.validationFailed), .validity(.valid)]

        let decision = try await makeRecovery(api: api, ticks: 5).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .submission(TestError.validationFailed)
        )

        expectResubmit(decision)
        #expect(api.callCount == 2)
    }

    @Test("aborts when the new-heads stream ends without becoming valid")
    func abortsWhenStreamEnds() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.fallback = .validity(.invalid(.future))

        let decision = try await makeRecovery(api: api, ticks: 2).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .txInvalidation
        )

        expectAbort(decision)
        #expect(api.callCount == 2)
    }

    @Test("aborts when validation keeps failing even without an attempt cap")
    func abortsWhenValidationKeepsFailing() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.fallback = .failure(TestError.validationFailed)

        // maxAttempts nil is what the PGAS wiring uses: mortality can only bound the loop while
        // validate_transaction actually answers, so a permanently failing call must still terminate.
        let recovery = try makeRecovery(api: api, ticks: 500, maxAttempts: nil)
        let builtExtrinsic = try makeBuiltExtrinsic()

        let decision = await recovery.recover(builtExtrinsic: builtExtrinsic, failure: .txInvalidation)

        expectAbort(decision, "a persistently failing validation must not wait forever")
        #expect(
            api.callCount == ResubmitWhenValidRecovery.maxConsecutiveValidationFailures,
            "the loop must stop after the consecutive-failure cap"
        )
    }

    @Test("transient validation failures do not count towards the cap")
    func transientFailuresDoNotCap() async throws {
        let api = TaggedTransactionQueueApiMock()
        // Alternating failures never reach the cap, so the eventual .valid still wins.
        api.responses = Array(
            repeating: [
                TaggedTransactionQueueApiMock.Response.failure(TestError.validationFailed),
                .validity(.invalid(.future))
            ],
            count: 20
        ).flatMap { $0 } + [.validity(.valid)]

        let decision = try await makeRecovery(api: api, ticks: 500).recover(
            builtExtrinsic: makeBuiltExtrinsic(),
            failure: .txInvalidation
        )

        expectResubmit(decision, "intermittent validation errors must not abort recovery")
    }

    @Test("an undecodable extrinsic aborts immediately")
    func undecodableExtrinsicAborts() async throws {
        let api = TaggedTransactionQueueApiMock()
        let recovery = try makeRecovery(api: api, ticks: 500)

        let decision = await recovery.recover(
            builtExtrinsic: ExtrinsicBuiltModel(extrinsic: "not-hex", sender: .none, mortality: .immortal),
            failure: .txInvalidation
        )

        expectAbort(decision, "an undecodable body can never become valid")
        #expect(api.callCount == 0, "no runtime call should be attempted for an undecodable body")
    }

    @Test("task cancellation ends the wait and aborts")
    func cancellationEndsTheWait() async throws {
        let api = TaggedTransactionQueueApiMock()
        api.fallback = .validity(.invalid(.future))

        // An open stream parks the loop between blocks — the state an abandoned claim sits in.
        let recovery = try makeRecovery(api: api, ticks: 0, keepsStreamOpen: true)
        let builtExtrinsic = try makeBuiltExtrinsic()

        let task = Task {
            await recovery.recover(builtExtrinsic: builtExtrinsic, failure: .txInvalidation)
        }
        task.cancel()

        await expectAbort(task.value, "a cancelled recovery must abort rather than keep waiting")
    }
}

private extension ResubmitWhenValidRecoveryTests {
    enum TestError: Error {
        case validationFailed
    }

    func makeRecovery(
        api: TaggedTransactionQueueApiMock,
        ticks: Int,
        maxAttempts: Int? = nil,
        keepsStreamOpen: Bool = false
    ) throws -> ResubmitWhenValidRecovery {
        let header = try BlockInfoProvidingMock.makeHeader()
        let blockInfoProvider = BlockInfoProvidingMock(header: header, headTicks: ticks)
        blockInfoProvider.keepsStreamOpen = keepsStreamOpen

        return ResubmitWhenValidRecovery(
            chainId: "test-chain",
            maxAttempts: maxAttempts,
            validationApi: api,
            blockInfoProvider: blockInfoProvider,
            logger: nil
        )
    }

    func makeBuiltExtrinsic() throws -> ExtrinsicBuiltModel {
        ExtrinsicBuiltModel(extrinsic: "0x0401", sender: .none, mortality: .immortal)
    }

    func expectResubmit(
        _ decision: ExtrinsicSubmissionFailureRecovery,
        _ message: Comment = "expected resubmit",
        sourceLocation: SourceLocation = #_sourceLocation
    ) {
        guard case let .resubmit(extrinsic) = decision else {
            Issue.record(message, sourceLocation: sourceLocation)
            return
        }

        #expect(
            extrinsic.extrinsic == "0x0401",
            "recovery must resubmit the extrinsic it was given",
            sourceLocation: sourceLocation
        )
    }

    func expectAbort(
        _ decision: ExtrinsicSubmissionFailureRecovery,
        _ message: Comment = "expected abort",
        sourceLocation: SourceLocation = #_sourceLocation
    ) {
        guard case .abort = decision else {
            Issue.record(message, sourceLocation: sourceLocation)
            return
        }
    }
}
