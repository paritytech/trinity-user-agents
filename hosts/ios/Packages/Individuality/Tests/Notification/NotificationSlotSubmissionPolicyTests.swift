import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import SubstrateSdk
import Testing
@testable import Individuality

struct NotificationSlotSubmissionPolicyTests {
    private let target = AccountId.target(1)
    private let repository = FakeNotificationSlotRepository()
    private let originFactory = FakeOriginFactory()
    private let dependencies: NotificationSlotDependencies

    init() {
        repository.highestSeqByCollection = [NotificationPersons.full.collectionIdentifier: 2]
        dependencies = .test(repository: repository, origins: [NotificationPersons.full])
    }

    @Test func givesUpAndReleasesReservationWhenNoSlotIsFree() async throws {
        repository.highestSeqByCollection = [NotificationPersons.full.collectionIdentifier: 0]
        try repository.register(NotificationPersons.full, period: 100, seq: 0)
        try dependencies.reservations.reserve(slotKey(seq: 1), for: target)
        let claim = scheduledClaim()

        let preparations = try await makePolicy().prepareSubmission([claim])

        #expect(isGiveUp(preparations[claim.id]))
        #expect(dependencies.reservations.reserved(for: target) == nil)
        #expect(originFactory.notificationOriginCalls.isEmpty)
    }

    @Test func buildsClaimOnSlotReservedWhenScheduled() async throws {
        try dependencies.reservations.reserve(slotKey(seq: 2), for: target)
        let claim = scheduledClaim()

        let preparations = try await makePolicy().prepareSubmission([claim])

        #expect(isReady(preparations[claim.id]))
        #expect(originFactory.notificationOriginCalls.map(\.seq) == [2])
    }

    @Test func movesClaimToFreeSlotOnceReservedSlotIsTakenOnChain() async throws {
        try dependencies.reservations.reserve(slotKey(seq: 2), for: target)
        try repository.register(NotificationPersons.full, period: 100, seq: 2)

        _ = try await makePolicy().prepareSubmission([scheduledClaim()])

        #expect(originFactory.notificationOriginCalls.map(\.seq) == [0])
        #expect(dependencies.reservations.reserved(for: target) == slotKey(seq: 0))
    }

    @Test func rebuildsEveryFailureKind() async {
        let policy = makePolicy()
        let entry = DurableTxEntry.fixture(domainId: NotificationSlotDomain.domainId)

        for failure in DurableFailureKind.allCases {
            #expect(await policy.canRetry(entry, params: target, failure: failure))
        }
    }

    private func makePolicy() -> NotificationSlotSubmissionPolicy {
        NotificationSlotSubmissionPolicy(
            dependencies: dependencies,
            originFactory: originFactory,
            factory: FakeDurableTxMaking()
        )
    }

    private func scheduledClaim() -> ScheduledDurableTx {
        ScheduledDurableTx(
            id: UUID(),
            domainId: NotificationSlotDomain.domainId,
            groupId: NotificationSlotDomain.groupId(for: target),
            policy: NotificationSlotDomain.policy(for: target)
        )
    }

    private func slotKey(seq: UInt8) -> NotificationSlot.Key {
        NotificationSlot(personOrigin: NotificationPersons.full, period: 100, seq: seq).key
    }

    private func isGiveUp(_ preparation: SubmissionPreparation?) -> Bool {
        if case .giveUp = preparation { return true }
        return false
    }

    private func isReady(_ preparation: SubmissionPreparation?) -> Bool {
        if case .ready = preparation { return true }
        return false
    }
}
