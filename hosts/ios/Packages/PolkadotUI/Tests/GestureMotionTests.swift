import CoreGraphics
import Testing
@testable import PolkadotUI

struct GestureMotionTests {
    @Test("A short release or zero travel cannot complete")
    func incompleteRelease() {
        #expect(!GestureMotion.completes(
            CGPoint(x: 170, y: 0), axis: .horizontal, travel: 200, release: .complete(thresholdPercent: 90)
        ))
        #expect(!GestureMotion.completes(
            .zero, axis: .horizontal, travel: 0, release: .complete(thresholdPercent: 90)
        ))
        #expect(!GestureMotion.completes(
            CGPoint(x: 200, y: 0), axis: .horizontal, travel: 200, release: .snapBack
        ))
    }

    @Test("Threshold completion and fractions use the declared axis")
    func completion() {
        #expect(GestureMotion.completes(
            CGPoint(x: 180, y: 0), axis: .horizontal, travel: 200, release: .complete(thresholdPercent: 90)
        ))
        #expect(GestureMotion.fraction(CGPoint(x: 0, y: 100), axis: .vertical, travel: 200) == 500)
    }

    @Test("Motion remains bounded and preserves an existing Stay position")
    func clampedMotion() {
        #expect(GestureMotion.position(
            origin: CGPoint(x: 70, y: 0), translation: CGPoint(x: 30, y: 90), axis: .horizontal, travel: 200
        ) == CGPoint(x: 100, y: 0))
        #expect(GestureMotion.position(
            origin: .zero, translation: CGPoint(x: -30, y: 400), axis: .free, travel: 200
        ) == CGPoint(x: 0, y: 200))
    }

    @Test("The slop is fed in over three pan frames, then the thumb sits where the finger is")
    func slopCatchUp() {
        let translation = PanTranslation(recognized: CGPoint(x: 20, y: 0), slop: CGPoint(x: 9, y: 3))
        #expect(GestureMotion.fromTouchDown(translation, step: 0) == CGPoint(x: 20, y: 0))
        #expect(GestureMotion.fromTouchDown(translation, step: 1) == CGPoint(x: 23, y: 1))
        #expect(GestureMotion.fromTouchDown(translation, step: 2) == CGPoint(x: 26, y: 2))
        #expect(GestureMotion.fromTouchDown(translation, step: 3) == CGPoint(x: 29, y: 3))
        #expect(GestureMotion.fromTouchDown(translation, step: 40) == CGPoint(x: 29, y: 3))
    }

    @Test("The slop is how far the finger moved from its touch-down before the pan was recognised")
    @MainActor
    func slopFromTouchDown() {
        #expect(GestureSurfaceView.slop(
            touchDown: CGPoint(x: 100, y: 50), location: CGPoint(x: 130, y: 52), translation: CGPoint(x: 20, y: 0)
        ) == CGPoint(x: 10, y: 2))
        #expect(GestureSurfaceView.slop(
            touchDown: nil, location: CGPoint(x: 130, y: 52), translation: CGPoint(x: 20, y: 0)
        ) == .zero)
    }

    @Test("A horizontal surface yields to vertical chat scrolling")
    @MainActor
    func panArbitration() {
        #expect(!GestureSurfaceView.claimsDrag(axis: .horizontal, translation: CGPoint(x: 3, y: 12), velocity: .zero))
        #expect(GestureSurfaceView.claimsDrag(axis: .horizontal, translation: CGPoint(x: 12, y: 3), velocity: .zero))
        #expect(!GestureSurfaceView.claimsDrag(axis: .free, translation: .zero, velocity: .zero))
    }
}
