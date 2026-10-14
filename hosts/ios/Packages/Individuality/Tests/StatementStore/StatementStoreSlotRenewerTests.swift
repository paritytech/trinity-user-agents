import Foundation
import SubstrateSdk
import StructuredConcurrency
import IssueMonitoring
import Testing
@testable import Individuality

struct StatementStoreSlotRenewerTests {
    private let keyManager = FakeBandersnatchKeyManaging()
    private let chainId = "polkadot"
    private let collection = "test-collection"

    private func makePersonOrigin() -> PersonOrigin {
        .lite(0, keyManager)
    }

    private func makeRenewer(
        slotInfoProvider: FakeSlotInfoProvider,
        repository: FakeSlotAccounting,
        submitter: FakeSubmitter,
        periodProvider: FakeChainTimeProvider,
        originFactory: FakeOriginFactory = FakeOriginFactory()
    ) -> StatementStoreSlotRenewer {
        let queue = SerialOperationQueue()
        let logger = FakeLogger()

        return StatementStoreSlotRenewer(
            chainId: chainId,
            slotInfoProvider: slotInfoProvider,
            accounting: repository,
            submitter: submitter,
            originFactory: originFactory,
            chainTimeProvider: periodProvider,
            serialQueue: queue,
            issueDiagnostics: .make(reporter: NoopIssueReporter()),
            logger: logger
        )
    }

    // Test: Submit failure leaves row stale
    @Test func submitFailureDoesNotMarkRenewedOrDelete() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()
        periodProvider.setPeriod(100)

        let accountA = Data([0x01])
        let accountB = Data([0x02])

        let rowA = AllowanceRecord(
            accountId: accountA,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )
        let rowB = AllowanceRecord(
            accountId: accountB,
            allocatedAt: Date(timeIntervalSince1970: 2_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )

        repository.setInitialRecords([rowA, rowB])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 0),
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 1)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()
        submitter.failureSequence = [true, false]

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider
        )

        try await renewer.renew()

        #expect(repository.markRenewedCalls.count == 1)
        #expect(repository.markRenewedCalls[0].record.accountId == accountA)
        #expect(repository.deleteForCalls.isEmpty)
    }

    // Test: Full capacity - all rows renewed
    @Test func fullCapacityRenewsAllRows() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()
        periodProvider.setPeriod(100)

        let accountA = Data([0x01])
        let accountB = Data([0x02])

        let rowA = AllowanceRecord(
            accountId: accountA,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )
        let rowB = AllowanceRecord(
            accountId: accountB,
            allocatedAt: Date(timeIntervalSince1970: 2_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )

        repository.setInitialRecords([rowA, rowB])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 0),
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 1)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider
        )

        try await renewer.renew()

        #expect(repository.markRenewedCalls.count == 2)
        #expect(repository.deleteForCalls.isEmpty)
        #expect(submitter.submitCalls.count == 2)
    }

    // Test: PGAS-shaped rows (nil latestRenewedPeriod) are not stale
    @Test func pgasRowsWithNilPeriodAreNotStale() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()
        periodProvider.setPeriod(100)

        let pgasAccount = Data([0x01])

        // PGAS row: nil latestRenewedPeriod
        let pgasRow = AllowanceRecord(
            accountId: pgasAccount,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .pgas,
            priority: .normal,
            latestRenewedPeriod: nil
        )

        repository.setInitialRecords([pgasRow])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 0)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider
        )

        try await renewer.renew()

        // PGAS row should NOT be renewed (not returned by staleRows)
        #expect(repository.markRenewedCalls.isEmpty)
        #expect(submitter.submitCalls.isEmpty)
    }

    // Test: Rejection during period rollover leaves row stale
    @Test func renewal_rollover_rejection_leaves_row_stale() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()

        let accountA = Data([0x01])
        let rowA = AllowanceRecord(
            accountId: accountA,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )

        repository.setInitialRecords([rowA])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 0)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()
        submitter.failureSequence = [true]
        submitter.failureError = SlotSubmissionError.extrinsicFailed(
            NSError(domain: "test", code: 1)
        )

        periodProvider.scriptedSeconds = [
            UInt64(100) * UInt64(TimeInterval.secondsInDay),
            UInt64(100) * UInt64(TimeInterval.secondsInDay),
            UInt64(101) * UInt64(TimeInterval.secondsInDay)
        ]

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider
        )

        try await renewer.renew()

        #expect(repository.deleteForCalls.isEmpty)
        #expect(repository.markRenewedCalls.isEmpty)
    }

    // Test: Rejection in same period deletes row
    @Test func renewal_same_period_rejection_deletes_row() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()

        let accountA = Data([0x01])
        let rowA = AllowanceRecord(
            accountId: accountA,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )

        repository.setInitialRecords([rowA])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 0)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()
        submitter.failureSequence = [true]
        submitter.failureError = SlotSubmissionError.extrinsicFailed(
            NSError(domain: "test", code: 1)
        )

        periodProvider.scriptedSeconds = [
            UInt64(100) * UInt64(TimeInterval.secondsInDay),
            UInt64(100) * UInt64(TimeInterval.secondsInDay),
            UInt64(100) * UInt64(TimeInterval.secondsInDay)
        ]

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider
        )

        try await renewer.renew()

        #expect(repository.deleteForCalls.contains(accountA))
        #expect(repository.markRenewedCalls.isEmpty)
    }

    // Test: Renewal ordering by priority
    @Test func renewal_prioritizes_high_priority_rows_over_newer_rows() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()
        periodProvider.setPeriod(100)

        let accountHigh = Data([0x01])
        let accountNormal = Data([0x02])

        // High priority with older allocatedAt
        let rowHigh = AllowanceRecord(
            accountId: accountHigh,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            priority: .high,
            latestRenewedPeriod: 99
        )
        // Normal priority with newer allocatedAt
        let rowNormal = AllowanceRecord(
            accountId: accountNormal,
            allocatedAt: Date(timeIntervalSince1970: 2_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )

        repository.setInitialRecords([rowHigh, rowNormal])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: makePersonOrigin(), seq: 0)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider
        )

        try await renewer.renew()

        // Should renew only the high-priority row (first slot taken by priority)
        #expect(repository.markRenewedCalls.count == 1)
        #expect(repository.markRenewedCalls[0].record.accountId == accountHigh)
    }

    @Test func renewalSignsEachSlotWithItsOwnOrigin() async throws {
        let repository = FakeSlotAccounting()
        let periodProvider = FakeChainTimeProvider()
        periodProvider.setPeriod(100)

        let accountA = Data([0x01])
        let accountB = Data([0x02])

        let rowA = AllowanceRecord(
            accountId: accountA,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )
        let rowB = AllowanceRecord(
            accountId: accountB,
            allocatedAt: Date(timeIntervalSince1970: 2_000),
            kind: .statementStore,
            priority: .normal,
            latestRenewedPeriod: 99
        )

        repository.setInitialRecords([rowA, rowB])

        let slotProvider = FakeSlotInfoProvider()
        let slots = [
            SSSRenewalSlot(personOrigin: .full(0, FakeBandersnatchKeyManaging()), seq: 0),
            SSSRenewalSlot(personOrigin: .lite(0, FakeBandersnatchKeyManaging()), seq: 0)
        ]
        slotProvider.updateRenewalSlots(.success(slots))

        let submitter = FakeSubmitter()
        let originFactory = FakeOriginFactory()

        let renewer = makeRenewer(
            slotInfoProvider: slotProvider,
            repository: repository,
            submitter: submitter,
            periodProvider: periodProvider,
            originFactory: originFactory
        )

        try await renewer.renew()

        #expect(repository.markRenewedCalls.count == 2)
        #expect(submitter.submitCalls.count == 2)
        let sssCalls = try #require(originFactory.sssOriginCalls.count == 2 ? originFactory.sssOriginCalls : nil)

        #expect(sssCalls[0].personOrigin.isFull, "First call should use .full origin")
        #expect(sssCalls[1].personOrigin.isLite, "Second call should use .lite origin")
        #expect(sssCalls[0].seq == 0)
        #expect(sssCalls[1].seq == 0)
    }
}

private extension PersonOrigin {
    var isFull: Bool {
        if case .full = self {
            return true
        }
        return false
    }

    var isLite: Bool {
        if case .lite = self {
            return true
        }
        return false
    }
}
