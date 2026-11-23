import Testing
import enum TrUAPIHost.ExpandedCardFaceOutcome
import enum TrUAPIHost.ExpandedCardTurnOutcome
import enum TrUAPIHost.PocketCardSide
@testable import polkadot_app

private let cardWidth = 300.0

@MainActor
struct PocketCardTurnTests {
    private let frames = SteppedFrames()

    private func turn(reduceMotion: Bool = false) -> PocketCardTurn {
        let turn = PocketCardTurn(frames: frames, reduceMotion: { reduceMotion })
        turn.backChanged(available: true)
        return turn
    }

    private func dragged(by points: Double) -> PocketCardTurn {
        let turn = turn()
        turn.beginDrag()
        turn.drag(by: points, width: cardWidth)
        return turn
    }

    /// Held at `angle` by the user, reached by swiping over one face at a time as a user does.
    private func held(at angle: Double) -> PocketCardTurn {
        let turn = turn(reduceMotion: true)
        var remaining = angle
        while abs(remaining) > 180 {
            let flip = remaining > 0 ? 180.0 : -180.0
            turn.beginDrag()
            turn.drag(by: flip / 180 * cardWidth, width: cardWidth)
            turn.release(velocity: 0, width: cardWidth)
            remaining -= flip
        }
        turn.beginDrag()
        turn.drag(by: remaining / 180 * cardWidth, width: cardWidth)
        return turn
    }

    // A card let go part way must land on a face; a short accidental drag must not turn it over.
    @Test
    func aCardReleasedAtRestTurnsOverOnlyWhenDraggedPastHalfItsWidth() {
        let short = dragged(by: cardWidth / 2 - 10)
        short.release(velocity: 0, width: cardWidth)
        frames.runToEnd()
        let long = dragged(by: cardWidth / 2 + 10)
        long.release(velocity: 0, width: cardWidth)
        frames.runToEnd()

        #expect([short.shownSide, long.shownSide] == [.front, .back])
    }

    // A swipe shows the other side of the card; it is not a spinner, however far or hard it goes.
    @Test(arguments: [(0.0, 20_000.0), (0, -20_000), (2 * cardWidth, 20_000), (-2 * cardWidth, 0)])
    func aSwipeTurnsTheCardOverOnceHoweverFarOrHard(points: Double, velocity: Double) {
        let turn = dragged(by: points)

        turn.release(velocity: velocity, width: cardWidth)
        frames.runToEnd()

        #expect(abs(turn.angle) == 180)
    }

    // The user holds the card until it has settled, so the page cannot move it under a finger just lifted.
    @Test
    func theUserHoldsTheCardUntilItsSettleEnds() {
        let turn = dragged(by: cardWidth * 0.8)

        turn.release(velocity: 0, width: cardWidth)
        let whileSettling = turn.isUserMoving
        frames.runToEnd()

        #expect([whileSettling, turn.isUserMoving] == [true, false])
    }

    @Test
    func aNewDragStopsTheCardSettling() {
        let turn = dragged(by: cardWidth * 0.8)
        turn.release(velocity: 0, width: cardWidth)
        frames.run(frames: 3)

        turn.beginDrag()
        let held = turn.angle
        frames.runToEnd()

        #expect(turn.angle == held)
        #expect(turn.isUserMoving)
    }

    // The page turns the card the way a right thumb swipes it, right to left, which lowers the angle.
    @Test
    func turningToASideShowsItAndTurningToTheSideUpDoesNothing() {
        let turn = turn()

        turn.turn(to: .front)
        let unmoved = turn.angle
        turn.turn(to: .back)
        frames.runToEnd()
        let turned = turn.angle
        turn.turn(to: .front)
        frames.runToEnd()

        #expect([unmoved, turned] == [0, -180])
        #expect(turn.angle.truncatingRemainder(dividingBy: 360) == 0)
    }

    // The page is told its request applied, so the card has to end on the side it asked for last.
    @Test
    func thePagesLastRequestWinsEvenWhileTheCardIsTurning() {
        let turn = turn()

        turn.turn(to: .back)
        frames.run(frames: 4)
        turn.turn(to: .front)
        frames.runToEnd()
        let afterFront = turn.angle
        turn.turn(to: .back)
        frames.run(frames: 4)
        turn.turn(to: .front)
        turn.turn(to: .back)
        frames.runToEnd()

        #expect(afterFront == 0)
        #expect(turn.shownSide == .back)
    }

    // A screen-reader user asks for the other side of where the card is going, not of where it is mid-turn,
    // and the card goes back the short way rather than spinning on.
    @Test
    func turningOverMidTurnReversesTheTurn() {
        let turn = turn()

        turn.turn(to: .back)
        frames.run(frames: 4)
        turn.turnOver()
        frames.runToEnd()

        #expect(turn.angle == 0)
    }

    // A card leaving shrinks back into the list, which shows only fronts, and a finger cannot hold it back.
    @Test
    func closingTurnsTheCardHomeEvenMidSettleAndWithADragOnIt() {
        let turned = turn()
        turned.turn(to: .back)
        frames.runToEnd()
        turned.turnHome()
        frames.runToEnd()

        let turn = dragged(by: cardWidth * 0.8)
        turn.release(velocity: 5_000, width: cardWidth)
        frames.run(frames: 3)
        turn.turnHome()
        frames.run(frames: 1)
        turn.beginDrag()
        turn.drag(by: cardWidth, width: cardWidth)
        frames.runToEnd()

        #expect([turned.angle, turn.angle].map { $0.truncatingRemainder(dividingBy: 360) } == [0, 0])
        #expect(!turn.isUserMoving)
    }

    // A back that disappears while it shows leaves nothing to turn to, so the front comes back without a turn, and
    // the page must not hear UserMoving ever after from the fling the snap cut short.
    @Test
    func losingTheBackShowsTheFrontAtOnceAndStopsTheTurn() {
        let turn = dragged(by: cardWidth * 0.8)
        turn.release(velocity: 5_000, width: cardWidth)
        frames.run(frames: 3)

        turn.backChanged(available: false)
        let snapped = turn.angle
        frames.runToEnd()

        #expect(snapped.truncatingRemainder(dividingBy: 360) == 0)
        #expect(turn.angle == snapped)
        #expect(!turn.isUserMoving && !turn.canTurn)
    }

    // The finger may still be down when the manifest says there is no back, and the card must not turn to one.
    @Test
    func aDragUnderWayStopsTurningWhenTheBackGoesAway() {
        let turn = dragged(by: cardWidth * 0.3)

        turn.backChanged(available: false)
        turn.drag(by: cardWidth * 0.5, width: cardWidth)
        turn.release(velocity: 5_000, width: cardWidth)
        frames.runToEnd()

        #expect(turn.angle == 0)
    }

    @Test
    func reduceMotionTurnsTheCardWithoutASpring() {
        let turn = turn(reduceMotion: true)

        turn.turn(to: .back)

        #expect(turn.angle == -180)
    }

    // Each face is drawn within ±180°, so the side up is never mirrored, and the face up is the one turned toward
    // the viewer.
    @Test(arguments: [0.0, 45, 90, 135, -135, 180, -180, 270, 540, -630])
    func drawsTheSideUpFacingTheViewer(angle: Double) {
        let turn = held(at: angle)
        let rotations = [PocketCardSide.front, .back].map { turn.layerRotation($0) }
        let shown = turn.layerRotation(turn.shownSide)

        #expect(turn.angle == angle)
        #expect(rotations.allSatisfy { (-180 ..< 180).contains($0) })
        #expect(abs(shown) <= 90)
    }
}

/// The page's answers, decided in one place and in this order. A missing back is told first, so the page does not
/// wait for the finger only to retry into `NoBack`.
struct ExpandedCardPageRequestTests {
    @Test(arguments: [
        (ExpandedCardPageRequest.back(shown: true), false, true, false, ExpandedCardTurnOutcome.noBack),
        (.back(shown: true), true, true, true, .userMoving),
        (.face(shown: false), true, true, true, .userMoving),
        (.back(shown: false), true, false, false, .notPresented),
        (.face(shown: true), true, false, false, .notPresented),
        (.back(shown: false), false, false, true, .applied),
        (.back(shown: true), true, false, true, .applied),
        (.face(shown: false), false, false, true, .applied)
    ])
    func answersThePageInTableOrder(
        request: ExpandedCardPageRequest,
        hasBack: Bool,
        isUserMoving: Bool,
        isPresented: Bool,
        outcome: ExpandedCardTurnOutcome
    ) {
        #expect(request.outcome(hasBack: hasBack, isUserMoving: isUserMoving, isPresented: isPresented) == outcome)
    }

    // The face call has no `NoBack` to give, so each of its answers keeps its meaning.
    @Test
    func theFaceCallKeepsEveryAnswerItCanBeGiven() {
        let answers: [ExpandedCardTurnOutcome] = [.applied, .userMoving, .notPresented, .unsupported]

        #expect(answers.map(ExpandedCardFaceOutcome.init) == [.applied, .userMoving, .notPresented, .unsupported])
    }
}

/// Frames stepped by the test, so a turn can be stopped part way and its end state read.
@MainActor
private final class SteppedFrames: PocketCardTurnFrames {
    private var step: ((Double) -> Bool)?

    func start(_ step: @escaping (Double) -> Bool) {
        self.step = step
    }

    func run(frames count: Int) {
        for _ in 0 ..< count {
            guard let step, step(1.0 / 60) else {
                step = nil
                return
            }
        }
    }

    func runToEnd() {
        run(frames: 600)
    }
}
