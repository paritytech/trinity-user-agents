import CoreMotion
import SwiftUI

// Shared gyroscope source for the card effects. Gravity comes from
// `DeviceMotionSource`, the app's single `CMMotionManager` (Apple advises one
// per app, and the coins' lighting reads the same one), ref-counted across
// every `.motionFillingShader` and `.motionShine` in use. Tilt is normalised to
// -1...1, relative to a reference captured after a short warmup so neutral
// locks onto the user's hold.
// `delayedTilt` is a lagged copy used by trailing layers (e.g. a wordmark).

@MainActor
@Observable
final class CardEffectMotionEngine {
    static let shared = CardEffectMotionEngine()

    private(set) var tilt: CGPoint = .zero
    private(set) var delayedTilt: CGPoint = .zero

    private let source: DeviceMotionObservable
    private var subscription: DeviceMotionToken?
    private var reference: CMAcceleration?
    private var warmupFrames = 0
    private var clients = 0

    private let warmupThreshold = 30
    private let sensitivity = 0.30

    private let smoothing = 0.33 // larger = snappier
    private let delaySmoothing = 0.05 // smaller = wordmark trails further

    private init(source: DeviceMotionObservable = DeviceMotionSource.shared) {
        self.source = source
    }

    func retain() {
        clients += 1
        if clients == 1 { startUpdates() }
    }

    func release() {
        clients = max(0, clients - 1)
        if clients == 0 { stopUpdates() }
    }

    private func startUpdates() {
        reference = nil
        warmupFrames = 0
        tilt = .zero
        delayedTilt = .zero
        subscription = source.observe { [weak self] gravity in
            MainActor.assumeIsolated {
                self?.apply(gravity)
            }
        }
    }

    private func stopUpdates() {
        subscription = nil
    }

    private func apply(_ gravity: CMAcceleration) {
        guard let reference else {
            warmupFrames += 1
            if warmupFrames >= warmupThreshold { reference = gravity }
            return
        }

        let targetX = max(-1, min(1, (gravity.x - reference.x) / sensitivity))
        let targetY = max(-1, min(1, (reference.y - gravity.y) / sensitivity))

        tilt.x += (targetX - tilt.x) * smoothing
        tilt.y += (targetY - tilt.y) * smoothing

        delayedTilt.x += (tilt.x - delayedTilt.x) * delaySmoothing
        delayedTilt.y += (tilt.y - delayedTilt.y) * delaySmoothing
    }
}
