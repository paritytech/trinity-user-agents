import Foundation
import SubstrateSdk

struct SSSOccupiedSlot {
    let personOrigin: PersonOrigin
    let seq: UInt32
    let alias: Data
    let accountId: AccountId
    let since: UInt64

    init(personOrigin: PersonOrigin, seq: UInt32, alias: Data, accountId: AccountId, since: UInt64) {
        self.personOrigin = personOrigin
        self.seq = seq
        self.alias = alias
        self.accountId = accountId
        self.since = since
    }
}

/// `signerKeyId` is set only when this installation holds the holder's key; new runtimes need it to sign the takeover.
public struct SSSEvictionCandidate {
    public let personOrigin: PersonOrigin
    public let seq: UInt32
    public let alias: Data
    public let holder: AccountId
    public let since: UInt64
    public let signerKeyId: String?

    public init(
        personOrigin: PersonOrigin,
        seq: UInt32,
        alias: Data,
        holder: AccountId,
        since: UInt64,
        signerKeyId: String?
    ) {
        self.personOrigin = personOrigin
        self.seq = seq
        self.alias = alias
        self.holder = holder
        self.since = since
        self.signerKeyId = signerKeyId
    }
}

protocol StatementStoreSlotEvictionPicking {
    func rankCandidates(
        from occupiedSlots: [SSSOccupiedSlot],
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority
    ) async throws -> [SSSEvictionCandidate]
}

final class StatementStoreSlotEvictionPicker: StatementStoreSlotEvictionPicking {
    private let accounting: StatementStoreSlotAccounting

    init(accounting: StatementStoreSlotAccounting) {
        self.accounting = accounting
    }

    func rankCandidates(
        from occupiedSlots: [SSSOccupiedSlot],
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority
    ) async throws -> [SSSEvictionCandidate] {
        let slotsOfOthers = occupiedSlots.filter { $0.accountId != accountId }
        let records = try await accounting.records(for: slotsOfOthers.map(\.accountId))

        let ranked = slotsOfOthers.compactMap { slot -> RankedCandidate? in
            // A slot with no local row belongs to another installation; it ranks as a normal allowance.
            let record = records[slot.accountId]
            let priority = record?.priority ?? .normal

            guard priority.isEvictable, priority <= callerPriority else {
                return nil
            }

            let candidate = SSSEvictionCandidate(
                personOrigin: slot.personOrigin,
                seq: slot.seq,
                alias: slot.alias,
                holder: slot.accountId,
                since: slot.since,
                signerKeyId: record?.signerKeyId
            )
            return RankedCandidate(priority: priority, candidate: candidate)
        }

        guard !ranked.isEmpty else {
            throw StatementStoreAllowanceError.noEvictableSlots
        }

        return ranked
            .sorted { ($0.priority, $0.candidate.since) < ($1.priority, $1.candidate.since) }
            .map(\.candidate)
    }
}

private extension StatementStoreSlotEvictionPicker {
    struct RankedCandidate {
        let priority: AllowanceRecord.Priority
        let candidate: SSSEvictionCandidate
    }
}
