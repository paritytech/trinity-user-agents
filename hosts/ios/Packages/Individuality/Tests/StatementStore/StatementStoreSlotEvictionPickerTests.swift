import Foundation
import Testing
import SubstrateSdk
@testable import Individuality

struct StatementStoreSlotEvictionPickerTests {
    private let keyManager = FakeBandersnatchKeyManaging()

    @Test
    func allSlotsOwnedByCallerThrowsNoEvictableSlots() async throws {
        let caller = try Data.randomOrError(of: 32)
        let picker = makePicker(records: [makeRecord(accountId: caller, priority: .normal)])

        let slots = [
            makeSlot(liteOrigin(), seq: 0, accountId: caller, since: 100),
            makeSlot(fullOrigin(), seq: 1, accountId: caller, since: 200)
        ]

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.rankCandidates(from: slots, excluding: caller, callerPriority: .normal)
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
            makeSlot(liteOrigin(), seq: 0, accountId: firstOther, since: 100),
            makeSlot(fullOrigin(), seq: 1, accountId: secondOther, since: 200)
        ]

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.rankCandidates(from: slots, excluding: caller, callerPriority: .normal)
        }
    }

    @Test
    func criticalSlotsAreNeverEvictedEvenByCriticalCaller() async throws {
        let caller = try Data.randomOrError(of: 32)
        let other = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [makeRecord(accountId: other, priority: .critical)])
        let slots = [makeSlot(liteOrigin(), seq: 0, accountId: other, since: 100)]

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.rankCandidates(from: slots, excluding: caller, callerPriority: .critical)
        }
    }

    @Test
    func unknownAccountsRankAsNormalAndMissingSignerKeysAreKept() async throws {
        let caller = try Data.randomOrError(of: 32)
        let unknown = try Data.randomOrError(of: 32)
        let unsignable = try Data.randomOrError(of: 32)
        let signable = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [
            makeRecord(accountId: unsignable, priority: .normal, signerKeyId: nil),
            makeRecord(accountId: signable, priority: .normal)
        ])

        let slots = [
            makeSlot(liteOrigin(), seq: 0, accountId: unknown, since: 10),
            makeSlot(liteOrigin(), seq: 1, accountId: unsignable, since: 20),
            makeSlot(liteOrigin(), seq: 2, accountId: signable, since: 30)
        ]

        let candidates = try await picker.rankCandidates(from: slots, excluding: caller, callerPriority: .high)

        #expect(candidates.map(\.holder) == [unknown, unsignable, signable])
        #expect(candidates.map(\.signerKeyId) == [nil, nil, "//signer"])
    }

    @Test
    func unknownAccountIsNotEvictedByNormalCallerBelowItsRank() async throws {
        let caller = try Data.randomOrError(of: 32)
        let unknown = try Data.randomOrError(of: 32)
        let picker = makePicker(records: [])

        let candidates = try await picker.rankCandidates(
            from: [makeSlot(liteOrigin(), seq: 0, accountId: unknown, since: 10)],
            excluding: caller,
            callerPriority: .normal
        )

        #expect(candidates.map(\.holder) == [unknown])
    }

    @Test
    func criticalCallerEvictsHighSlotAndSkipsCritical() async throws {
        let caller = try Data.randomOrError(of: 32)
        let criticalAccount = try Data.randomOrError(of: 32)
        let highAccount = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [
            makeRecord(accountId: criticalAccount, priority: .critical),
            makeRecord(accountId: highAccount, priority: .high)
        ])

        let slots = [
            makeSlot(liteOrigin(), seq: 0, accountId: criticalAccount, since: 100),
            makeSlot(fullOrigin(), seq: 1, accountId: highAccount, since: 200)
        ]

        let candidates = try await picker.rankCandidates(from: slots, excluding: caller, callerPriority: .critical)

        #expect(candidates.map(\.holder) == [highAccount])
    }

    @Test
    func ranksLowestPriorityThenOldestAndCarriesSlotIdentity() async throws {
        let caller = try Data.randomOrError(of: 32)
        let highPriorityAccount = try Data.randomOrError(of: 32)
        let recentNormalAccount = try Data.randomOrError(of: 32)
        let oldestNormalAccount = try Data.randomOrError(of: 32)

        let picker = makePicker(records: [
            makeRecord(accountId: highPriorityAccount, priority: .high, signerKeyId: "//high"),
            makeRecord(accountId: recentNormalAccount, priority: .normal, signerKeyId: "//recent"),
            makeRecord(accountId: oldestNormalAccount, priority: .normal, signerKeyId: "//oldest")
        ])

        let slots = [
            makeSlot(fullOrigin(), seq: 0, accountId: highPriorityAccount, since: 10),
            makeSlot(liteOrigin(), seq: 1, accountId: recentNormalAccount, since: 50),
            makeSlot(fullOrigin(), seq: 2, accountId: oldestNormalAccount, since: 20)
        ]

        let candidates = try await picker.rankCandidates(from: slots, excluding: caller, callerPriority: .high)

        #expect(candidates.map(\.holder) == [oldestNormalAccount, recentNormalAccount, highPriorityAccount])
        #expect(candidates.map(\.signerKeyId) == ["//oldest", "//recent", "//high"])
        #expect(candidates.map(\.seq) == [2, 1, 0])
        #expect(candidates.map(\.since) == [20, 50, 10])
        #expect(candidates.first?.alias == alias(forSeq: 2))
        guard case .full = candidates[0].personOrigin else {
            Issue.record("Expected full origin")
            return
        }
    }

    @Test
    func emptyOccupiedSlotsThrowsNoEvictableSlots() async throws {
        let caller = try Data.randomOrError(of: 32)
        let picker = makePicker(records: [])

        await #expect(throws: StatementStoreAllowanceError.noEvictableSlots) {
            try await picker.rankCandidates(from: [], excluding: caller, callerPriority: .normal)
        }
    }
}

private extension StatementStoreSlotEvictionPickerTests {
    func makePicker(records: [AllowanceRecord]) -> StatementStoreSlotEvictionPicker {
        let accounting = FakeSlotAccounting()
        accounting.setInitialRecords(records)
        return StatementStoreSlotEvictionPicker(accounting: accounting)
    }

    func makeRecord(
        accountId: AccountId,
        priority: AllowanceRecord.Priority,
        signerKeyId: String? = "//signer"
    ) -> AllowanceRecord {
        AllowanceRecord(
            accountId: accountId,
            allocatedAt: Date(),
            kind: .statementStore,
            domain: .product,
            priority: priority,
            latestRenewedPeriod: nil,
            signerKeyId: signerKeyId
        )
    }

    func makeSlot(_ origin: PersonOrigin, seq: UInt32, accountId: AccountId, since: UInt64) -> SSSOccupiedSlot {
        SSSOccupiedSlot(personOrigin: origin, seq: seq, alias: alias(forSeq: seq), accountId: accountId, since: since)
    }

    func alias(forSeq seq: UInt32) -> Data {
        Data(repeating: UInt8(seq), count: 32)
    }

    func liteOrigin(_ ringIndex: UInt32 = 0) -> PersonOrigin {
        .lite(ringIndex, keyManager)
    }

    func fullOrigin(_ ringIndex: UInt32 = 0) -> PersonOrigin {
        .full(ringIndex, keyManager)
    }
}
