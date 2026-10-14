import Testing
import Foundation
import Foundation_iOS
import Individuality
@testable import Coinage

/// Covers `VoucherLocationService.resolveLocations` — the pure join of the member-position and
/// ring-status snapshots — with an emphasis on the retraction cases `UncertainStorage<Value?>`
/// distinguishes: not delivered (a missing key or `.undefined`), `.defined(nil)` (delivered empty /
/// retracted), and `.defined(value)` (delivered with a value).
@Suite("VoucherLocationService.resolveLocations")
struct VoucherLocationResolveTests {
    private let observedAt = Date(timeIntervalSince1970: 1_000)
    /// A page holds `MaxFlexibleRingExponent.ringCapacity()` keys — 255 when the exponent is `R2e9`.
    private let keysPerPage = 255

    @Test("A member row delivered empty (retracted) reverts the voucher to unlocated")
    func retractedMemberBecomesUnlocated() {
        let resolved = resolve([0: .defined(nil)])
        #expect(resolved[0] == .unlocated)
    }

    @Test("A member position not delivered in the update is left untouched — no write")
    func undeliveredPositionIsSkipped() {
        // `.undefined` is the subscription saying nothing about this key; it must not be resolved.
        let resolved = resolve([0: .undefined])
        #expect(resolved[0] == nil)
    }

    @Test("An included voucher confirmed by its ring status is in-recycler with the real member count")
    func includedAndConfirmedIsInRecycler() {
        let resolved = resolve(
            [0: .defined(included(ring: 5, position: 1))],
            [0: .defined(status(total: 10, included: 3))]
        )
        #expect(resolved[0] == .inRecycler(.init(index: 5, membersCount: 3, enteredAt: observedAt)))
    }

    @Test("An onboarding position is onboarding")
    func onboardingPositionIsOnboarding() {
        #expect(resolve([0: .defined(onboarding())])[0] == .onboarding)
    }

    @Test("A suspended position (no ring index) is onboarding")
    func suspendedPositionIsOnboarding() {
        #expect(resolve([0: .defined(.suspended)])[0] == .onboarding)
    }

    @Test("An included voucher whose ring status is delivered empty (retracted) falls back to onboarding")
    func includedButRingRetractedIsOnboarding() {
        let resolved = resolve(
            [0: .defined(included(ring: 5, position: 1))],
            [0: .defined(nil)]
        )
        #expect(resolved[0] == .onboarding)
    }

    @Test("An included voucher whose ring status has not arrived yet is deferred — no write")
    func includedButStatusNotArrivedIsDeferred() {
        // The status map has no entry for the voucher: not delivered, distinct from delivered-empty.
        let resolved = resolve([0: .defined(included(ring: 5, position: 1))])
        #expect(resolved[0] == nil)
    }

    @Test("An included voucher whose ring does not yet cover its key is deferred — no write")
    func includedButKeyNotYetInRingIsDeferred() {
        // The key sits at position 3 of page 0; a ring that has admitted only 2 keys does not cover it yet.
        let resolved = resolve(
            [0: .defined(included(ring: 5, position: 3))],
            [0: .defined(status(total: 10, included: 2))]
        )
        #expect(resolved[0] == nil)
    }

    @Test("A key on a later ring page counts the pages before it")
    func includedOnLaterPageCountsPrecedingPages() {
        // Page 1, position 3 is the ring's 259th key, so 258 baked keys stop just short of it.
        let position = included(ring: 5, page: 1, position: 3)

        #expect(resolve([0: .defined(position)], [0: .defined(status(total: 300, included: 258))])[0] == nil)
        #expect(
            resolve([0: .defined(position)], [0: .defined(status(total: 300, included: 259))])[0]
                == .inRecycler(.init(index: 5, membersCount: 259, enteredAt: observedAt))
        )
    }

    @Test("A retraction, a confirmed inclusion and an onboarding resolve independently in one batch")
    func mixedBatchResolvesPerVoucher() {
        let resolved = resolve(
            [
                0: .defined(nil),
                1: .defined(included(ring: 7, position: 0)),
                2: .defined(onboarding())
            ],
            [1: .defined(status(total: 4, included: 2))]
        )
        #expect(resolved[0] == .unlocated)
        #expect(resolved[1] == .inRecycler(.init(index: 7, membersCount: 2, enteredAt: observedAt)))
        #expect(resolved[2] == .onboarding)
        #expect(resolved.count == 3)
    }
}

// MARK: - Helpers

private extension VoucherLocationResolveTests {
    func resolve(
        _ positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>],
        _ statuses: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingKeysStatus?>] = [:]
    ) -> [CoinageKeyIndex: Voucher.OnChainState] {
        VoucherLocationService.resolveLocations(
            positions: positions,
            statuses: statuses,
            keysPerPage: keysPerPage,
            observedAt: observedAt
        )
    }

    func included(
        ring: MembersPallet.RingIndex,
        page: MembersPallet.PageIndex = 0,
        position: UInt32
    ) -> MembersPallet.RingPosition {
        .included(.init(ringIndex: ring, ringPage: page, ringPosition: position))
    }

    func onboarding() -> MembersPallet.RingPosition {
        .onboarding(.init(queuePage: 0, queuedAt: 0))
    }

    func status(total: UInt32, included: UInt32) -> MembersPallet.RingKeysStatus {
        .init(total: total, included: included)
    }
}
