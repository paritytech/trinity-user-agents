import CoreGraphics
import Testing
@testable import PolkadotUI

struct SlideToConfirmTrackTests {
    private let travel: CGFloat = 200

    @Test("A slide released short of the threshold springs back and confirms nothing")
    func shortSlideSpringsBack() {
        var track = SlideToConfirmTrack()

        track.drag(translation: 150, travel: travel)
        #expect(track.progress == 0.75)
        #expect(track.isDragging)

        #expect(track.release(lifted: true) == .springBack)
        #expect(track.progress == 0)
        #expect(!track.isHeldAtEnd)
    }

    @Test("A slide released past the threshold confirms once and holds at the end")
    func fullSlideConfirmsOnce() {
        var track = SlideToConfirmTrack()

        track.drag(translation: 185, travel: travel)
        #expect(track.release(lifted: true) == .confirm)
        #expect(track.progress == 1)
        #expect(track.isHeldAtEnd)

        track.drag(translation: -200, travel: travel)
        #expect(track.progress == 1)
        #expect(track.release(lifted: true) == .none)
        #expect(track.confirm() == .none)
    }

    @Test("A drag the system cancels never confirms, however far it went")
    func cancelledDragDoesNotConfirm() {
        var track = SlideToConfirmTrack()

        track.drag(translation: travel, travel: travel)
        #expect(track.release(lifted: false) == .springBack)
        #expect(track.progress == 0)
    }

    @Test("Progress follows the finger from where the drag began and stays on the track")
    func progressIsClamped() {
        var track = SlideToConfirmTrack()

        track.drag(translation: -50, travel: travel)
        #expect(track.progress == 0)
        track.drag(translation: 500, travel: travel)
        #expect(track.progress == 1)
        track.drag(translation: 100, travel: travel)
        #expect(track.progress == 0.5)
    }

    @Test("A track with no room to travel does not move")
    func zeroTravelDoesNotMove() {
        var track = SlideToConfirmTrack()

        track.drag(translation: 100, travel: 0)
        #expect(track.progress == 0)
        #expect(track.release(lifted: true) == .none)
    }

    @Test("Releasing without a drag does nothing")
    func releaseWithoutDragIsIgnored() {
        var track = SlideToConfirmTrack()

        #expect(track.release(lifted: true) == .none)
    }

    @Test("The accessibility confirmation fires once, like a full slide")
    func directConfirmationFiresOnce() {
        var track = SlideToConfirmTrack()

        #expect(track.confirm() == .confirm)
        #expect(track.progress == 1)
        #expect(track.confirm() == .none)
    }

    @Test("Loading or confirmed from the product holds the control at the end")
    func productHoldsAtEnd() {
        var track = SlideToConfirmTrack()

        track.follow(productHoldsAtEnd: true)
        #expect(track.isHeldAtEnd)
        #expect(track.progress == 1)

        track.drag(translation: -100, travel: travel)
        #expect(track.progress == 1)
        #expect(track.confirm() == .none)
    }

    @Test("A tree that is neither loading nor confirmed puts a slid control back at the start")
    func productReleasesTheControl() {
        var track = SlideToConfirmTrack()
        _ = track.confirm()

        track.follow(productHoldsAtEnd: false)
        #expect(!track.isHeldAtEnd)
        #expect(track.progress == 0)
        #expect(track.confirm() == .confirm)
    }

    @Test("A control first drawn as confirmed starts at the end")
    func startsHeldWhenDrawnConfirmed() {
        let track = SlideToConfirmTrack(heldAtEnd: true)

        #expect(track.progress == 1)
        #expect(track.isHeldAtEnd)
    }
}
