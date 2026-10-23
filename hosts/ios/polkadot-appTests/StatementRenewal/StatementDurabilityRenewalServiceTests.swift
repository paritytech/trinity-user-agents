import Foundation
import StatementDelivery
import Testing

@testable import polkadot_app

struct StatementDurabilityRenewalServiceTests {
    private static let requests = OutgoingStatementRenewalSummary(
        isComplete: true,
        candidates: 3,
        due: 2,
        republished: 2,
        failed: 0,
        statementsWithMissedPeriods: 1,
        missedPeriods: 1
    )

    private static let acceptances = OutgoingStatementRenewalSummary(
        isComplete: true,
        candidates: 1,
        due: 0,
        republished: 0,
        failed: 0,
        statementsWithMissedPeriods: 0,
        missedPeriods: 0
    )

    @Test func runRenewsAllowancesAndEveryStatementKindThroughTheSharedRun() async {
        let allowances = AllowanceSlotsRenewalSpy()
        let sharedRun = SharedRunSpy<StatementDurabilityRenewalReport>()
        let service = makeService(allowances: allowances, sharedRun: sharedRun)

        let report = await service.renewAll()

        #expect(sharedRun.callCount == 1)
        #expect(allowances.callCount == 1)
        #expect(report == StatementDurabilityRenewalReport(
            chatRequests: Self.requests,
            chatAcceptances: Self.acceptances
        ))
    }

    @Test func missedPeriodIsReportedWithCountersOnly() async {
        let missedPeriod = RecordingIssueDiagnostic()
        let service = makeService(allowances: AllowanceSlotsRenewalSpy(), missedPeriod: missedPeriod)

        await service.renewAll()

        #expect(missedPeriod.failureCount == 1)
        #expect(missedPeriod.lastCounters == ["statements": 1, "missedPeriods": 1])
    }

    @Test func runWithoutGapsResetsTheLapse() async {
        let missedPeriod = RecordingIssueDiagnostic()
        let service = makeService(
            allowances: AllowanceSlotsRenewalSpy(),
            missedPeriod: missedPeriod,
            requests: Self.acceptances
        )

        await service.renewAll()

        #expect(missedPeriod.failureCount == 0)
        #expect(missedPeriod.recoveryCount == 1)
    }
}

private extension StatementDurabilityRenewalServiceTests {
    func makeService(
        allowances: AllowanceSlotsRenewalSpy,
        missedPeriod: RecordingIssueDiagnostic = RecordingIssueDiagnostic(),
        requests: OutgoingStatementRenewalSummary = Self.requests,
        sharedRun: SharedRunSpy<StatementDurabilityRenewalReport> = SharedRunSpy()
    ) -> StatementDurabilityRenewalService {
        StatementDurabilityRenewalService(
            allowances: allowances,
            renewers: StatementRenewerProviderStub(chatRequests: requests, chatAcceptances: Self.acceptances),
            issues: StatementRenewalIssueDiagnostics(
                chatRequestsMissedPeriod: missedPeriod,
                chatAcceptancesMissedPeriod: RecordingIssueDiagnostic()
            ),
            sharedRun: sharedRun,
            logger: StubLogger()
        )
    }
}
