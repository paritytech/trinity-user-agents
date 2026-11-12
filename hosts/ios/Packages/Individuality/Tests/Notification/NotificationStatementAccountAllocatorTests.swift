import Clocks
import DurableTransactions
import Foundation
import SubstrateSdk
import Testing
@testable import Individuality

struct NotificationStatementAccountAllocatorTests {
    private let repository = FakeNotificationSlotRepository()
    private let ledger = FakeClaimLedger()
    private let dependencies: NotificationSlotDependencies

    init() {
        repository.highestSeqByCollection = [NotificationPersons.full.collectionIdentifier: 1]
        dependencies = .test(repository: repository, origins: [NotificationPersons.full])
    }

    @Test func initiateAllocationSchedulesClaimWhenSlotIsFree() async throws {
        try await makeAllocator().initiateAllocation(for: .target(1))

        #expect(ledger.scheduled.map(\.groupId) == [NotificationSlotDomain.groupId(for: .target(1))])
        #expect(ledger.scheduled.first?.policy == NotificationSlotDomain.policy(for: .target(1)))
    }

    @Test func doesNotScheduleAgainWhileClaimIsLive() async throws {
        ledger.setClaims(for: .target(1), statuses: [.pending])

        try await makeAllocator().initiateAllocation(for: .target(1))

        #expect(ledger.scheduled.isEmpty)
    }

    @Test func retriesTargetWhoseEarlierClaimGaveUp() async throws {
        ledger.setClaims(for: .target(1), statuses: [.failure])

        try await makeAllocator().initiateAllocation(for: .target(1))

        #expect(ledger.scheduled.count == 1)
    }

    @Test func failsWithNoFreeSlotWhenNoSlotIsFree() async throws {
        try repository.register(NotificationPersons.full, period: 100, seq: 0)
        try repository.register(NotificationPersons.full, period: 100, seq: 1)

        await #expect(throws: NotificationAllocationError.noFreeSlotInPeriod) {
            try await makeAllocator().initiateAllocation(for: .target(1))
        }
    }

    @Test func initiateAllocationsSchedulesLeadingTargetsUpToFreeSlotCount() async throws {
        let initiated = try await makeAllocator().initiateAllocations(for: [.target(1), .target(2), .target(3)])

        #expect(initiated == [.target(1), .target(2)])
        #expect(ledger.scheduled.count == 2)
    }

    @Test func reservesSlotAtSchedulingSoFollowingAllocationCannotTakeIt() async throws {
        let allocator = makeAllocator()

        try await allocator.initiateAllocation(for: .target(1))
        try await allocator.initiateAllocation(for: .target(2))

        await #expect(throws: NotificationAllocationError.noFreeSlotInPeriod) {
            try await allocator.initiateAllocation(for: .target(3))
        }
    }

    @Test func awaitAllocatedSucceedsOnceClaimHasExecuted() async throws {
        ledger.setClaims(for: .target(1), statuses: [.failure, .pendingSuccess])

        try await makeAllocator().awaitAllocated(.target(1), timeout: .seconds(60))
    }

    @Test func awaitAllocatedFailsWithNoFreeSlotWhenEveryClaimGaveUp() async throws {
        ledger.setClaims(for: .target(1), statuses: [.failure])

        await #expect(throws: NotificationAllocationError.noFreeSlotInPeriod) {
            try await makeAllocator().awaitAllocated(.target(1), timeout: .seconds(60))
        }
    }

    @Test func awaitAllocatedTimesOutWhileClaimIsPending() async throws {
        ledger.setClaims(for: .target(1), statuses: [.pending])

        await #expect(throws: NotificationAllocationError.timeout) {
            try await makeAllocator(clock: ImmediateClock()).awaitAllocated(.target(1), timeout: .seconds(60))
        }
    }

    private func makeAllocator(clock: any Clock<Duration> = ContinuousClock())
        -> NotificationStatementAccountAllocator {
        NotificationStatementAccountAllocator(dependencies: dependencies, ledger: ledger, clock: clock)
    }
}
