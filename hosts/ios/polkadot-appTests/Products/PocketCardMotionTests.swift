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

        #expect(motions(openAt: 1, clearance: 1) == [shrunkAway, lifted, slidAway, slidAway])
    }

    /// Before the Pocket clears and after it comes back, only the opened card
    /// is away from its slot, and it stays fully drawn.
    @Test
    func onlyTheOpenCardMovesWhileThePocketIsInPlace() {
        let lifted = PocketCardMotion(offset: lift, scale: 1, opacity: 1)

        #expect(motions(openAt: 1, clearance: 0) == [.resting, lifted, .resting, .resting])
    }

    /// A finger can hold the Pocket part way to cleared, so each card is drawn
    /// part way between its slot and where it clears to, and the swipe shows
    /// the same close as the timed one.
    @Test
    func eachCardIsHalfWayWhenThePocketIsHalfCleared() {
        let halfSlid = PocketCardMotion(
            offset: CGSize(width: 0, height: PocketCardSize.height * 2),
            scale: 1,
            opacity: 0.5
        )
        let halfShrunk = PocketCardMotion(offset: .zero, scale: 0.95, opacity: 0.5)
        let lifted = PocketCardMotion(offset: lift, scale: 1, opacity: 1)

        #expect(motions(openAt: 1, clearance: 0.5) == [halfShrunk, lifted, halfSlid, halfSlid])
    }

    @Test
    func nothingMovesWithoutAnOpenCard() {
        #expect(motions(openAt: nil, clearance: 0) == Array(repeating: .resting, count: 4))
    }

    /// The whole Pocket clears behind an item that is not in the stack, such as
    /// Collectibles, so every card shrinks away while that item is open.
    @Test
    func everyCardShrinksAwayWhenThePocketClearsForAnItemOutsideTheStack() {
        #expect(motions(openAt: nil, clearance: 1) == Array(repeating: .shrunkAway, count: 4))
    }

    private func motions(openAt openIndex: Int?, clearance: CGFloat) -> [PocketCardMotion] {
        (0 ..< 4).map { index in
            PocketCardMotion.of(cardAt: index, openAt: openIndex, openOffset: lift, clearance: clearance)
        }
    }
}
