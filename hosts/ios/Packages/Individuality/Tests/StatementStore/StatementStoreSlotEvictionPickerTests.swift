import Foundation
import Testing
import SubstrateSdk
@testable import Individuality

struct StatementStoreSlotEvictionPickerTests {
    private let keyManager = FakeBandersnatchKeyManaging()

    @Test
    func allSlotsOwnedByCallerThrowsNoEvictableSlots() async throws {
        let caller = try Data.randomOrError(of: 32)
        let picker = makePicker(records: [])

        let slots = [
            SSSOccupiedSlot(personOrigin: liteOrigin(), seq: 0, accountId: caller, since: 100),
            SSSOccupiedSlot(personOrigin: fullOrigin(), seq: 1, accountId: caller, since: 200)
        ]

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.pickCandidate(
                from: slots,
                excluding: caller,
                callerPriority: .normal,
                cooldown: 0,
                nowSeconds: 300
            )
        }
    }

    @Test
    func allOtherSlotsOutrankCallerThrowsNoEvictableSlots() async throws {
        let caller = try Data.randomOrError(of: 32)
        let firstOther = try Data.randomOrError(of: 32)
        let secondOther = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [
            makeRecord(accountId: firstOther, priority: .high),
            makeRecord(accountId: secondOther, priority: .high)
        ])

        let slots = [
            SSSOccupiedSlot(personOrigin: liteOrigin(), seq: 0, accountId: firstOther, since: 100),
            SSSOccupiedSlot(personOrigin: fullOrigin(), seq: 1, accountId: secondOther, since: 200)
        ]

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.pickCandidate(
                from: slots,
                excluding: caller,
                callerPriority: .normal,
                cooldown: 0,
                nowSeconds: 300
            )
        }
    }

    @Test
    func cooldownNotElapsedThrowsNoSlotsAvailableWithWait() async throws {
        let caller = try Data.randomOrError(of: 32)
        let other = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [makeRecord(accountId: other, priority: .normal)])

        let slots = [
            SSSOccupiedSlot(personOrigin: liteOrigin(), seq: 0, accountId: other, since: 100)
        ]

        await #expect(throws: StatementStoreAllowanceError.noSlotsAvailable(secsToWait: 40)) {
            try await picker.pickCandidate(
                from: slots,
                excluding: caller,
                callerPriority: .high,
                cooldown: 60,
                nowSeconds: 120
            )
        }
    }

    @Test
    func picksLowestPriorityThenOldest() async throws {
        let caller = try Data.randomOrError(of: 32)
        let highPriorityAccount = try Data.randomOrError(of: 32)
        let recentNormalAccount = try Data.randomOrError(of: 32)
        let oldestNormalAccount = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [
            makeRecord(accountId: highPriorityAccount, priority: .high),
            makeRecord(accountId: recentNormalAccount, priority: .normal),
            makeRecord(accountId: oldestNormalAccount, priority: .normal)
        ])

        let slots = [
            SSSOccupiedSlot(personOrigin: fullOrigin(), seq: 0, accountId: highPriorityAccount, since: 10),
            SSSOccupiedSlot(personOrigin: liteOrigin(), seq: 1, accountId: recentNormalAccount, since: 50),
            SSSOccupiedSlot(personOrigin: fullOrigin(), seq: 2, accountId: oldestNormalAccount, since: 20)
        ]

        let result = try await picker.pickCandidate(
            from: slots,
            excluding: caller,
            callerPriority: .high,
            cooldown: 0,
            nowSeconds: 100
        )

        guard case .full = result.personOrigin else {
            Issue.record("Expected full origin")
            return
        }
        #expect(result.seq == 2)
    }

    @Test
    func emptyOccupiedSlotsThrowsNoEvictableSlots() async throws {
        let caller = try Data.randomOrError(of: 32)
        let picker = makePicker(records: [])

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.pickCandidate(
                from: [],
                excluding: caller,
                callerPriority: .normal,
                cooldown: 0,
                nowSeconds: 300
            )
        }
    }
}

private extension StatementStoreSlotEvictionPickerTests {
    func makePicker(records: [AllowanceRecord]) -> StatementStoreSlotEvictionPicker {
        let accounting = FakeSlotAccounting()
        accounting.setInitialRecords(records)
        return StatementStoreSlotEvictionPicker(accounting: accounting)
    }

    func makeRecord(accountId: AccountId, priority: AllowanceRecord.Priority) -> AllowanceRecord {
        AllowanceRecord(
            accountId: accountId,
            allocatedAt: Date(),
            kind: .statementStore,
            priority: priority,
            latestRenewedPeriod: nil
        )
    }

    func liteOrigin(_ ringIndex: UInt32 = 0) -> PersonOrigin {
        .lite(ringIndex, keyManager)
    }

    func fullOrigin(_ ringIndex: UInt32 = 0) -> PersonOrigin {
        .full(ringIndex, keyManager)
    }
}
