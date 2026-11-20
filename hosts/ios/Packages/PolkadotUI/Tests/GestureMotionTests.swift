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

    @Test("A horizontal surface yields to vertical chat scrolling")
    @MainActor
    func panArbitration() {
        #expect(!GestureSurfaceView.claimsDrag(axis: .horizontal, translation: CGPoint(x: 3, y: 12), velocity: .zero))
        #expect(GestureSurfaceView.claimsDrag(axis: .horizontal, translation: CGPoint(x: 12, y: 3), velocity: .zero))
        #expect(!GestureSurfaceView.claimsDrag(axis: .free, translation: .zero, velocity: .zero))
    }
}
