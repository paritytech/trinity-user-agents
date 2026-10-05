import CoreMotion
import Foundation
import os

/// Where gravity comes from, so it can be stubbed for something that has none.
public protocol DeviceMotionObservable: AnyObject, Sendable {
    /// How often readings arrive, for an observer that integrates between them.
    var updateInterval: TimeInterval { get }

    /// Starts the sensor if nobody was watching, and keeps it running until the returned token
    /// goes away. Readings arrive on the main queue.
    func observe(_ receive: @escaping (CMAcceleration) -> Void) -> DeviceMotionToken
}

/// Holds a subscription open. Releasing it releases the sensor.
public final class DeviceMotionToken {
    private let cancel: () -> Void

    init(cancel: @escaping () -> Void) {
        self.cancel = cancel
    }

    deinit {
        cancel()
    }
}

/// The app's one `CMMotionManager`, shared by everything that reacts to how the phone is held.
///
/// Apple asks for a single instance per process, and there is a concrete reason here rather than
/// only guidance: the card effects and the coins' lighting are on screen together, so a manager
/// each meant two copies of the same sensor pipeline delivering the same readings.
///
/// Raw gravity rather than anything derived, because its two readers want different things from
/// it: the card effects normalize it against a reference captured after a warmup, and the coins
/// turn their studio by it. Both would rather have the reading than each other's interpretation
/// of it.
///
/// Reference counted, so the sensor runs only while something is watching. The readings are the
/// same for everyone, which is what makes sharing them safe: nobody's subscription changes what
/// anybody else sees.
public final class DeviceMotionSource: DeviceMotionObservable, @unchecked Sendable {
    public static let shared = DeviceMotionSource()

    /// Fast enough for a specular highlight to track a wrist and for the card shine to feel
    /// attached to the phone. Both readers smooth their own way on top of it.
    public let updateInterval: TimeInterval = 1.0 / 40

    private struct State {
        var observers: [UUID: (CMAcceleration) -> Void] = [:]
    }

    private let manager = CMMotionManager()
    private let state = OSAllocatedUnfairLock(initialState: State())

    init() {}

    public func observe(_ receive: @escaping (CMAcceleration) -> Void) -> DeviceMotionToken {
        let id = UUID()
        let isFirst = state.withLockUnchecked { state in
            defer { state.observers[id] = receive }

            return state.observers.isEmpty
        }

        if isFirst { start() }

        return DeviceMotionToken { [weak self] in self?.forget(id) }
    }
}

private extension DeviceMotionSource {
    func forget(_ id: UUID) {
        let isLast = state.withLockUnchecked { state in
            state.observers[id] = nil

            return state.observers.isEmpty
        }

        if isLast { manager.stopDeviceMotionUpdates() }
    }

    func start() {
        guard manager.isDeviceMotionAvailable, !manager.isDeviceMotionActive else { return }

        manager.deviceMotionUpdateInterval = updateInterval
        manager.startDeviceMotionUpdates(to: .main) { [weak self] update, _ in
            guard let self, let gravity = update?.gravity else { return }

            let observers = state.withLockUnchecked { Array($0.observers.values) }

            for receive in observers {
                receive(gravity)
            }
        }
    }
}
