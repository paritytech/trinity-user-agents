import Foundation

public struct HostLoadTelemetry: Sendable {
    public static let disabled = HostLoadTelemetry()

    private let recorder: (any HostEventRecording)?
    private let origin: HostEventOrigin
    private let sessions: HostSessionRegistry
    private let monotonicMilliseconds: @Sendable () -> Int

    public init(
        recorder: any HostEventRecording,
        origin: HostEventOrigin = .current,
        sessions: HostSessionRegistry = .shared,
        monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() }
    ) {
        self.recorder = recorder
        self.origin = origin
        self.sessions = sessions
        self.monotonicMilliseconds = monotonicMilliseconds
    }

    private init() {
        recorder = nil
        origin = .current
        sessions = HostSessionRegistry()
        monotonicMilliseconds = { 0 }
    }

    public var isEnabled: Bool {
        recorder != nil
    }

    public func now() -> Int? {
        isEnabled ? monotonicMilliseconds() : nil
    }

    public func openSession(productID: String, sessionID: UUID) {
        guard isEnabled else { return }
        sessions.open(productID: productID, sessionID: sessionID)
    }

    public func closeSession(productID: String, sessionID: UUID) {
        guard isEnabled else { return }
        sessions.close(productID: productID, sessionID: sessionID)
    }

    public func sessionID(forProductOrSubname name: String) -> UUID? {
        sessions.sessionID(forProductOrSubname: name)
    }

    public func record(_ event: (_ origin: HostEventOrigin, _ monotonicMilliseconds: Int) -> HostEvent) {
        recorder?.record(event(origin, monotonicMilliseconds()))
    }
}
