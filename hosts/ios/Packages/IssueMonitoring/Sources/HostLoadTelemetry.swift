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
        self.recorder = recorder.isRecording ? recorder : nil
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

    /// Read first thing in the tap handler, before any hop or animation. The page it opens mints its session.
    public func beginOpen(source: HostOpenSource) -> HostOpenTap? {
        guard isEnabled else { return nil }

        return HostOpenTap(
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

    /// Whether a load step for `productID` should be reported. Inside a session, a step reports once for each
    /// product: the cold read, or the first cache hit, and the open's second read of what it just applied
    /// stays silent. A failed read always reports and claims nothing, so the read that follows still counts.
    /// Outside a session only cache hits are thinned, once per trigger, since a lookup repeated on every render
    /// is not a load.
    public func shouldReport(_ step: String, productID: String, outcome: HostEventOutcome) -> Bool {
        guard let context else { return false }
        guard outcome != .error, context.sessionID != nil || outcome == .cacheHit else { return true }

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
