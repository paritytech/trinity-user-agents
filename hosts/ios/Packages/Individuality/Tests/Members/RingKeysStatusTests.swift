import Foundation
import Testing
@testable import Individuality

/// A ring's keys are stored in pages of `MaxFlexibleRingExponent` capacity. A member's `ringPosition`
/// counts within its own page, while `RingKeysStatus.included` counts the keys baked into the root across
/// the whole ring, so the two only compare once the preceding pages are added back.
struct RingKeysStatusTests {
    /// A page holds `MaxFlexibleRingExponent.ringCapacity()` keys — 255 when the exponent is `R2e9`.
    private let keysPerPage = 255

    @Test func keyOnFirstPageIsIncludedOnceTheRingBakedPastIt() {
        #expect(includesKey(included: 101, page: 0, position: 100))
    }

    @Test func keyOnFirstPageIsNotIncludedBeforeTheRingBakedIt() {
        #expect(!includesKey(included: 100, page: 0, position: 100))
    }

    @Test func keyOnLaterPageCountsThePagesBeforeIt() {
        // Page 1, position 100 is the ring's 356th key, so 355 baked keys stop just short of it.
        #expect(!includesKey(included: 355, page: 1, position: 100))
        #expect(includesKey(included: 356, page: 1, position: 100))
    }

    @Test func onboardingKeyIsNotIncluded() {
        let status = MembersPallet.RingKeysStatus(total: 400, included: 400)
        let position = MembersPallet.RingPosition.onboarding(.init(queuePage: 0, queuedAt: 0))

        #expect(!status.includesKey(from: position, keysPerPage: keysPerPage))
    }

    @Test func suspendedKeyIsNotIncluded() {
        let status = MembersPallet.RingKeysStatus(total: 400, included: 400)

        #expect(!status.includesKey(from: .suspended, keysPerPage: keysPerPage))
    }
}

private extension RingKeysStatusTests {
    func includesKey(included: UInt32, page: MembersPallet.PageIndex, position: UInt32) -> Bool {
        let status = MembersPallet.RingKeysStatus(total: 767, included: included)
        let ringPosition = MembersPallet.RingPosition.Included(
            ringIndex: 0,
            ringPage: page,
            ringPosition: position
        )

        return status.includesKey(at: ringPosition, keysPerPage: keysPerPage)
    }
}
