import Foundation

public struct HostLoadTelemetry: Sendable {
    public static let disabled = HostLoadTelemetry()

    private let recorder: (any HostEventRecording)?
    private let origin: HostEventOrigin
    private let cacheHits: HostCacheHitLedger
    private let monotonicMilliseconds: @Sendable () -> Int
    private let epochMilliseconds: @Sendable () -> Int64

    public init(
        recorder: any HostEventRecording,
        origin: HostEventOrigin = .current,
        cacheHits: HostCacheHitLedger = .shared,
        monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() },
        epochMilliseconds: @escaping @Sendable () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1_000) }
    ) {
        self.recorder = recorder
        self.origin = origin
        self.cacheHits = cacheHits
        self.monotonicMilliseconds = monotonicMilliseconds
        self.epochMilliseconds = epochMilliseconds
    }

    private init() {
        recorder = nil
        origin = .current
        cacheHits = HostCacheHitLedger()
        monotonicMilliseconds = { 0 }
        epochMilliseconds = { 0 }
    }

    public var isEnabled: Bool {
        recorder != nil
    }

    public func now() -> Int? {
        isEnabled ? monotonicMilliseconds() : nil
    }

    public func makeSessionID() -> UUID? {
        isEnabled ? UUID() : nil
    }

    /// Read first thing in the tap handler, before any hop or animation.
    public func beginOpen(source: HostOpenSource) -> HostOpenTap? {
        guard isEnabled else { return nil }

        return HostOpenTap(
            hostSessionID: UUID(),
            source: source,
            openedAtEpochMilliseconds: epochMilliseconds(),
            monotonicMilliseconds: monotonicMilliseconds()
        )
    }

    public func withContext<R>(
        session: UUID?,
        trigger: HostLoadTrigger,
        isolation _: isolated (any Actor)? = #isolation,
        operation: () async throws -> R
    ) async rethrows -> R {
        guard isEnabled else { return try await operation() }

        return try await HostLoadContext.$current.withValue(
            HostLoadContext(sessionID: session, trigger: trigger),
            operation: operation
        )
    }

    /// Steps report only inside a load context: a lookup made for an icon or a name is not a load.
    public var context: HostLoadContext? {
        isEnabled ? HostLoadContext.current : nil
    }

    /// The clock, read only for a step that will report: one run inside a load context.
    public func loadStepStart() -> Int? {
        context == nil ? nil : now()
    }

    public func isFirstCacheHit(of step: String, productID: String) -> Bool {
        guard let context else { return false }

        return cacheHits.claim(step: step, productID: productID, context: context)
    }

    public func record(_ event: (_ origin: HostEventOrigin, _ monotonicMilliseconds: Int) -> HostEvent) {
        recorder?.record(event(origin, monotonicMilliseconds()))
    }
}

public struct HostLoadContext: Sendable, Equatable {
    public let sessionID: UUID?
    public let trigger: HostLoadTrigger

    public init(sessionID: UUID?, trigger: HostLoadTrigger) {
        self.sessionID = sessionID
        self.trigger = trigger
    }

    @TaskLocal static var current: HostLoadContext?
}
