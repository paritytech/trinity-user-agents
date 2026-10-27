import Foundation
import IssueMonitoring
import SubstrateSdk
import Testing
@testable import Individuality

struct NotificationSeqPickerTests {
    private let period: UInt32 = 100
    private let repository = FakeNotificationSlotRepository()
    private let reservations = NotificationSeqReservations()

    init() {
        repository.highestSeqByCollection = [
            NotificationPersons.full.collectionIdentifier: 1,
            NotificationPersons.lite.collectionIdentifier: 1
        ]
    }

    @Test func listsFullPersonSeqsBeforeLiteSeqs() async throws {
        let free = try await makePicker().freeSlots(period: period, forTarget: nil)

        #expect(free.map(\.key) == [
            key(NotificationPersons.full, seq: 0),
            key(NotificationPersons.full, seq: 1),
            key(NotificationPersons.lite, seq: 0),
            key(NotificationPersons.lite, seq: 1)
        ])
    }

    @Test func excludesSeqsRegisteredOnChainOrReservedForAnotherAccount() async throws {
        try repository.register(NotificationPersons.full, period: period, seq: 0)
        try reservations.reserve(key(NotificationPersons.full, seq: 1), for: .target(1))

        let free = try await makePicker().freeSlots(period: period, forTarget: .target(2))

        #expect(free.map(\.key) == [key(NotificationPersons.lite, seq: 0), key(NotificationPersons.lite, seq: 1)])
    }

    @Test func keepsSeqReservedForAskingAccountFreeForIt() async throws {
        try reservations.reserve(key(NotificationPersons.full, seq: 0), for: .target(1))

        let free = try await makePicker().freeSlots(period: period, forTarget: .target(1))

        #expect(free.first?.key == key(NotificationPersons.full, seq: 0))
    }

    @Test func offersNothingOnRuntimeWithoutNotificationSlots() async throws {
        repository.supported = false

        let free = try await makePicker().freeSlots(period: period, forTarget: nil)

        #expect(free.isEmpty)
    }

    private func makePicker() -> NotificationSeqPicker {
        NotificationSeqPicker(
            sources: .test(repository: repository),
            reservations: reservations,
            unsupportedRuntime: NoopIssueDiagnostic(),
            logger: FakeLogger()
        )
    }

    private func key(_ origin: PersonOrigin, seq: UInt8) -> NotificationSlot.Key {
        NotificationSlot(personOrigin: origin, period: period, seq: seq).key
    }
}
