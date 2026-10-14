import Testing
import Foundation
import Foundation_iOS
import Individuality
@testable import Coinage

/// Ring state — `RingKeysStatus` and `RecyclersUnloadedCount` — belongs to a ring, not a voucher, and
/// vouchers commonly share one. `VoucherLocationService.recyclers` is what collapses them: one
/// ``RecyclerKey`` per distinct ring, which becomes one storage request each however many vouchers
/// sit in it.
@Suite("VoucherLocationService ring deduplication")
struct VoucherRingDeduplicationTests {
    @Test("Vouchers sharing a ring collapse to one recycler key")
    func sharedRingCollapses() {
        let rings = recyclers(
            placements: [
                0: (exponent: 4, ring: 7),
                1: (exponent: 4, ring: 7),
                2: (exponent: 4, ring: 7)
            ]
        )

        #expect(rings.count == 3, "every voucher still resolves to its ring")
        #expect(Set(rings.values).count == 1, "but they share one subscription key")
    }

    @Test("Rings are distinguished by denomination as well as index")
    func sameIndexDifferentDenomination() {
        let rings = recyclers(
            placements: [
                0: (exponent: 4, ring: 7),
                1: (exponent: 5, ring: 7)
            ]
        )

        #expect(Set(rings.values).count == 2)
    }

    @Test("Distinct rings stay distinct")
    func distinctRings() {
        let rings = recyclers(
            placements: [
                0: (exponent: 4, ring: 1),
                1: (exponent: 4, ring: 2),
                2: (exponent: 4, ring: 2)
            ]
        )

        #expect(Set(rings.values).count == 2)
    }

    /// A voucher with no ring yet contributes no ring subscription at all.
    @Test("Unplaced vouchers contribute no recycler key")
    func unplacedVouchersAreSkipped() {
        let voucherByIndex = [
            CoinageKeyIndex.harness(0): voucher(0, exponent: 4),
            CoinageKeyIndex.harness(1): voucher(1, exponent: 4)
        ]
        let positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>] = [
            0: .defined(included(ring: 7, position: 0)),
            1: .defined(onboarding())
        ]

        let rings = VoucherLocationService.recyclers(
            positions: positions,
            voucherByIndex: voucherByIndex
        )

        #expect(rings.count == 1)
        #expect(rings[0] == RecyclerKey(exponent: 4, index: 7))
        #expect(rings[1] == nil)
    }

    @Test("A retracted or undelivered position contributes no recycler key")
    func retractedAndUndeliveredAreSkipped() {
        let voucherByIndex = [
            CoinageKeyIndex.harness(0): voucher(0, exponent: 4),
            CoinageKeyIndex.harness(1): voucher(1, exponent: 4)
        ]
        let positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>] = [
            0: .defined(nil),
            1: .undefined
        ]

        let rings = VoucherLocationService.recyclers(
            positions: positions,
            voucherByIndex: voucherByIndex
        )

        #expect(rings.isEmpty)
    }
}

// MARK: - Fixtures

private extension VoucherRingDeduplicationTests {
    func recyclers(
        placements: [CoinageKeyIndex: (exponent: Int16, ring: MembersPallet.RingIndex)]
    ) -> [CoinageKeyIndex: RecyclerKey] {
        var voucherByIndex: [CoinageKeyIndex: Voucher] = [:]
        var positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>] = [:]

        for (index, placement) in placements {
            voucherByIndex[index] = voucher(index, exponent: placement.exponent)
            positions[index] = .defined(included(ring: placement.ring, position: UInt32(index.item)))
        }

        return VoucherLocationService.recyclers(
            positions: positions,
            voucherByIndex: voucherByIndex
        )
    }

    func voucher(_ index: CoinageKeyIndex, exponent: Int16) -> Voucher {
        Voucher(
            exponent: exponent,
            derivationIndex: index,
            allocatedAt: Date(timeIntervalSinceReferenceDate: 0),
            readyAt: Date(timeIntervalSinceReferenceDate: 60),
            remoteState: .onboarding,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        )
    }

    func included(ring: MembersPallet.RingIndex, position: UInt32) -> MembersPallet.RingPosition {
        .included(.init(ringIndex: ring, ringPage: 0, ringPosition: position))
    }

    func onboarding() -> MembersPallet.RingPosition {
        .onboarding(.init(queuePage: 0, queuedAt: 0))
    }
}
