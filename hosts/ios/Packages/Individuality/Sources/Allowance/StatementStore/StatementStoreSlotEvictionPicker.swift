import Foundation
import SubstrateSdk

struct SSSOccupiedSlot {
    let personOrigin: PersonOrigin
    let seq: UInt32
    let accountId: AccountId
    let since: UInt64

    init(personOrigin: PersonOrigin, seq: UInt32, accountId: AccountId, since: UInt64) {
        self.personOrigin = personOrigin
        self.seq = seq
        self.accountId = accountId
        self.since = since
    }
}

protocol StatementStoreSlotEvictionPicking {
    func pickCandidate(
        from occupiedSlots: [SSSOccupiedSlot],
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority,
        cooldown: UInt32,
        nowSeconds: UInt64
    ) async throws -> SSSOccupiedSlot
}

final class StatementStoreSlotEvictionPicker: StatementStoreSlotEvictionPicking {
    private let accounting: StatementStoreSlotAccounting

    init(accounting: StatementStoreSlotAccounting) {
        self.accounting = accounting
    }

    func pickCandidate(
        from occupiedSlots: [SSSOccupiedSlot],
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority,
        cooldown: UInt32,
        nowSeconds: UInt64
    ) async throws -> SSSOccupiedSlot {
        let slotsOfOthers = occupiedSlots.filter { $0.accountId != accountId }

        let prioritiesByAccount = try await accounting.priorities(
            for: slotsOfOthers.map(\.accountId)
        )

        let evictable = slotsOfOthers.filter { slot in
            let priority = prioritiesByAccount[slot.accountId] ?? .normal
            return priority <= callerPriority
        }

        guard !evictable.isEmpty else {
            throw StatementStoreAllowanceError.noEvictableSlots
        }

        let cooledDown = evictable.filter { nowSeconds >= $0.since + UInt64(cooldown) }

        let oldest = cooledDown.min { lhs, rhs in
            let lhsPriority = prioritiesByAccount[lhs.accountId] ?? .normal
            let rhsPriority = prioritiesByAccount[rhs.accountId] ?? .normal
            return (lhsPriority, lhs.since) < (rhsPriority, rhs.since)
        }

        guard let oldest else {
            throw StatementStoreAllowanceError.noSlotsAvailable(
                secsToWait: secondsToWait(evictable: evictable, nowSeconds: nowSeconds, cooldown: cooldown)
            )
        }

        return oldest
    }
}

private extension StatementStoreSlotEvictionPicker {
    func secondsToWait(
        evictable: [SSSOccupiedSlot],
        nowSeconds: UInt64,
        cooldown: UInt32
    ) -> TimeInterval {
        let minSince = evictable.map(\.since).min() ?? nowSeconds
        return max(TimeInterval(minSince + UInt64(cooldown)) - TimeInterval(nowSeconds), 0)
    }
}
