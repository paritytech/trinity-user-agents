import CoreGraphics
import Testing
@testable import PolkadotUI

struct GestureDragThresholdTests {
    @Test("No setting is the low threshold; a number is points; system keeps the system pan")
    func settings() {
        #expect(GestureDragThreshold.points(setting: nil) == GestureDragThreshold.defaultPoints)
        #expect(GestureDragThreshold.points(setting: "") == GestureDragThreshold.defaultPoints)
        #expect(GestureDragThreshold.points(setting: "2.5") == 2.5)
        #expect(GestureDragThreshold.points(setting: "system") == nil)
        #expect(GestureDragThreshold.points(setting: "-4") == GestureDragThreshold.defaultPoints)
        #expect(GestureDragThreshold.points(setting: "soon") == GestureDragThreshold.defaultPoints)
    }

    @Test("The low threshold is under a millimetre on a 460 ppi, 3x screen and below the system's 10 points")
    func aboutAMillimetre() {
        let pointsPerMillimetre: CGFloat = 460 / 3 / 25.4
        #expect(GestureDragThreshold.defaultPoints / pointsPerMillimetre < 1)
        #expect(GestureDragThreshold.defaultPoints < 10)
    }

    @Test("A pan starts once the finger is the threshold away from its touch-down, in any direction")
    func crossing() {
        #expect(!GestureDragThreshold.crosses(CGPoint(x: 4.9, y: 0), threshold: 5))
        #expect(GestureDragThreshold.crosses(CGPoint(x: 5, y: 0), threshold: 5))
        #expect(GestureDragThreshold.crosses(CGPoint(x: 3, y: 4), threshold: 5))
        #expect(!GestureDragThreshold.crosses(CGPoint(x: -3, y: 3), threshold: 5))
        #expect(GestureDragThreshold.crosses(.zero, threshold: 0))
    }

    @Test("The surface uses its own recogniser for a threshold and the system pan for none")
    @MainActor
    func recognizerChoice() {
        let own = GestureSurfaceView.makeDragRecognizer(5)
        #expect((own as? ThresholdPanGestureRecognizer)?.threshold == 5)
        let system = GestureSurfaceView.makeDragRecognizer(nil)
        #expect((system as? TouchDownPanGestureRecognizer)?.maximumNumberOfTouches == 1)
    }

    @Test("Before it begins the recogniser reports no movement and no speed")
    @MainActor
    func idleRecognizer() {
        let recognizer = ThresholdPanGestureRecognizer(threshold: 5)
        #expect(recognizer.translation(in: nil) == .zero)
        #expect(recognizer.velocity(in: nil) == .zero)
        #expect(recognizer.touchDown == nil)
    }

    @Test("A scratch takes the finger where it lands; any other pan waits for the threshold")
    func scratchStartsAtTouchDown() {
        #expect(GestureSurfaceView.claimsAtTouchDown(axis: .free, reportsLocation: true))
        #expect(!GestureSurfaceView.claimsAtTouchDown(axis: .free, reportsLocation: false))
        #expect(!GestureSurfaceView.claimsAtTouchDown(axis: .horizontal, reportsLocation: true))
        #expect(!GestureSurfaceView.claimsAtTouchDown(axis: .vertical, reportsLocation: true))
        #expect(!GestureSurfaceView.claimsAtTouchDown(axis: nil, reportsLocation: true))
    }

    @Test("A surface that starts at touch-down tells its recogniser to begin with no movement")
    @MainActor
    func surfaceStartsAtTouchDown() {
        let view = GestureSurfaceView()
        let recognizer = view.panRecognizer as? ThresholdPanGestureRecognizer
        #expect(recognizer?.beginsAtTouchDown == false)
        view.startsAtTouchDown = true
        #expect(recognizer?.beginsAtTouchDown == true)
    }
}
