import Clocks
import Foundation
import StatementDelivery
import Testing
import FoundationExtTestSupport

@Suite(.timeLimit(.minutes(10)))
struct OutgoingStatementRenewerTests {
    private let now = Date(timeIntervalSince1970: 1_000_000)
    private let store = FakeOutgoingStore()
    private let resolver = FakeResolver()
    private let service = FakeOutgoingService()

    @Test func acknowledgedStatementIsMarkedWithoutPublishing() async {
        store.setCandidates([candidate("A", period: 9)])
        store.acknowledge("A")

        _ = await makeRenewer().renew()

        #expect(store.markedAcknowledged == ["A"])
        #expect(service.publishes.isEmpty)
    }

    @Test func staleStatementsAreResolvedInOneBatchAndRepublishedIntoCurrentPeriod() async {
        store.setCandidates([candidate("newer", period: 9), candidate("older", period: 8)])

        _ = await makeRenewer().renew()

        #expect(resolver.calls == [["newer", "older"]])
        #expect(Set(store.published.map(\.id)) == ["newer", "older"])
        #expect(store.published.allSatisfy { $0.period == 10 })
    }

    @Test func currentCopyStillStoredIsLeftAlone() async {
        store.setCandidates([candidate("A", period: 10)])
        service.markStored("A")

        _ = await makeRenewer().renew()

        #expect(resolver.calls.isEmpty)
        #expect(service.publishes.isEmpty)
    }

    @Test func missingCurrentCopyIsRepublished() async {
        store.setCandidates([candidate("A", period: 10)])

        _ = await makeRenewer().renew()

        #expect(store.published.map(\.period) == [10])
    }

    @Test func boundPassedAbandonsStatement() async {
        store.setCandidates([candidate("A", period: 9, age: 8 * 86_400)])

        _ = await makeRenewer(bound: 7 * 86_400).renew()

        #expect(store.abandoned == ["A"])
        #expect(service.publishes.isEmpty)
    }

    @Test func withoutBoundOldStatementsKeepRenewing() async {
        store.setCandidates([candidate("A", period: 9, age: 30 * 86_400)])

        _ = await makeRenewer(bound: nil).renew()

        #expect(store.abandoned.isEmpty)
        #expect(store.published.map(\.id) == ["A"])
    }

    @Test func summaryCountsRepublishedStatementsAndWholeMissedPeriods() async {
        store.setCandidates([candidate("lapsed", period: 7), candidate("rollover", period: 9)])

        let summary = await makeRenewer().renew()

        let expected = OutgoingStatementRenewalSummary(
            isComplete: true,
            candidates: 2,
            due: 2,
            republished: 2,
            failed: 0,
            statementsWithMissedPeriods: 1,
            missedPeriods: 2
        )
        #expect(summary == expected)
    }

    @Test func terminalResolutionMarksStatementFailed() async {
        store.setCandidates([candidate("A", period: 9)])
        resolver.fail("A", with: TestTerminalError())

        _ = await makeRenewer().renew()

        #expect(store.failed == ["A"])
        #expect(service.publishes.isEmpty)
    }
}

private extension OutgoingStatementRenewerTests {
    func candidate(_ id: String, period: UInt32, age: TimeInterval = 0) -> PublishedStatement<TestInput> {
        PublishedStatement(input: TestInput(statementId: id), period: period, createdAt: now.addingTimeInterval(-age))
    }

    func makeRenewer(bound: TimeInterval? = nil) -> OutgoingStatementRenewer<TestInput> {
        let delivery = OutgoingStatementDeliveryService(
            dependencies: OutgoingStatementDeliveryDependencies(
                store: store,
                resolver: resolver,
                service: service,
                builder: FakeBuilder(),
                postDelivery: nil
            ),
            execution: OutgoingStatementDeliveryExecution(
                backgroundExecutor: InlineExecutor(),
                clock: ImmediateClock()
            ),
            diagnostics: .silent
        )

        return OutgoingStatementRenewer(
            dependencies: OutgoingStatementRenewalDependencies(
                store: store,
                resolver: resolver,
                service: service,
                delivery: delivery
            ),
            bound: bound,
            dateProvider: FixedDateProvider(now),
            diagnostics: .silent
        )
    }
}
