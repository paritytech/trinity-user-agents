import SwiftUI
import Testing
@testable import polkadot_app

/// The Pocket draws an opened card at its own place in the stack, so the stack
/// clears out of its way as Android's does: the cards below slide off the
/// bottom, the cards above shrink back, and only the opened card stays to move.
struct PocketCardMotionTests {
    private let lift = CGSize(width: 0, height: -66)

    @Test
    func eachCardMovesAsOnAndroidWhileThePocketClearsForAnOpenOne() {
        let slidAway = PocketCardMotion(
            offset: CGSize(width: 0, height: PocketCardSize.height * 4),
            scale: 1,
            opacity: 0
        )
        let shrunkAway = PocketCardMotion(offset: .zero, scale: 0.9, opacity: 0)
        let lifted = PocketCardMotion(offset: lift, scale: 1, opacity: 1)

        #expect(motions(openAt: 1, cleared: true) == [shrunkAway, lifted, slidAway, slidAway])
    }

    /// Before the Pocket clears and after it comes back, only the opened card
    /// is away from its slot, and it stays fully drawn.
    @Test
    func onlyTheOpenCardMovesWhileThePocketIsInPlace() {
        let lifted = PocketCardMotion(offset: lift, scale: 1, opacity: 1)

        #expect(motions(openAt: 1, cleared: false) == [.resting, lifted, .resting, .resting])
    }

    @Test
    func nothingMovesWithoutAnOpenCard() {
        #expect(motions(openAt: nil, cleared: true) == Array(repeating: .resting, count: 4))
    }

    private func motions(openAt openIndex: Int?, cleared: Bool) -> [PocketCardMotion] {
        (0 ..< 4).map { index in
            PocketCardMotion.of(cardAt: index, openAt: openIndex, openOffset: lift, cleared: cleared)
        }
    }
}
