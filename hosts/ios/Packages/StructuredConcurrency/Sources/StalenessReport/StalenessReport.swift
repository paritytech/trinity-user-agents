import AsyncExtensions
import Foundation

public protocol StallActivitySource: Sendable {
    var activities: AnyAsyncSequence<[StallActivity]> { get }
    /// After this returns the source MUST stop publishing `id` until the underlying work ends.
    func dismiss(id: UUID) async
}

public final class StalenessReport: StalenessReportCollecting, StallActivitySource {
    public static let shared = StalenessReport()

    /// Set once at app startup before any reporting call runs, never toggled afterwards.
    /// Local SPM packages cannot see `TESTNET_FEATURE`, so instrumentation compiled into them must
    /// stay inert unless the app opts in — otherwise Release builds record activities that no
    /// StallBoard ever consumes or dismisses.
    public nonisolated(unsafe) static var isEnabled = false

    let eventContinuation: AsyncStream<StallEvent>.Continuation
    let subject: AsyncCurrentValueSubject<[StallActivity]>
    let currentDate: @Sendable () -> Date

    private let maxSteps: Int

    public init(
        currentDate: @escaping @Sendable () -> Date = { Date() },
        maxSteps: Int = 64
    ) {
        let (stream, continuation) = AsyncStream<StallEvent>.makeStream()
        eventContinuation = continuation
        subject = AsyncCurrentValueSubject([])
        self.maxSteps = maxSteps
        self.currentDate = currentDate

        Task { [subject, stream] in
            var tree = StallActivityTree(maxSteps: maxSteps)

            for await event in stream {
                if case let .barrier(signal) = event {
                    signal()
                    continue
                }

                guard tree.apply(event) else { continue }
                subject.value = tree.visibleActivities
            }
        }
    }

    deinit {
        eventContinuation.finish()
    }

    public var activities: AnyAsyncSequence<[StallActivity]> {
        subject.eraseToAnyAsyncSequence()
    }

    public func startRegion(_: StallableRegion) -> any StalenessRegionHandling {
        NoOpStalenessCollector.shared
    }

    public func dismiss(id: UUID) async {
        eventContinuation.yield(.dismissed(id: id))
    }

    public func trackActivity<R>(_ title: String, body: () async throws -> R) async rethrows -> R {
        try await withStallRegion(handle: makeHandle(title: title), body: body)
    }

    /// Test barrier: resumes once every event yielded before this call has been folded into the tree.
    /// Exact rather than timing-based — the aggregator is a single consumer of an ordered stream.
    func drain() async {
        await withCheckedContinuation { continuation in
            eventContinuation.yield(.barrier { continuation.resume() })
        }
    }
}

private extension StalenessReport {
    /// Nests under the enclosing handle when this report already owns it, otherwise opens a new root.
    func makeHandle(title: String) -> StallHandle {
        if let parent = StalenessDiagnostics.collector as? StallHandle, parent.report === self {
            return parent.makeChild(title: title)
        }

        let activityId = UUID()
        eventContinuation.yield(.activityStarted(id: activityId, title: title, at: currentDate()))

        return StallHandle(id: activityId, report: self)
    }
}

final class StallHandle: StalenessRegionHandling {
    let id: UUID
    let report: StalenessReport

    init(id: UUID, report: StalenessReport) {
        self.id = id
        self.report = report
    }

    func makeChild(title: String) -> StallHandle {
        let childId = UUID()
        report.eventContinuation.yield(.regionStarted(
            id: childId,
            parent: id,
            title: title,
            at: report.currentDate()
        ))

        return StallHandle(id: childId, report: report)
    }

    func startRegion(_ region: StallableRegion) -> any StalenessRegionHandling {
        makeChild(title: region.title)
    }

    func end() {
        report.eventContinuation.yield(.ended(id: id, at: report.currentDate(), outcome: .finished))
    }

    func fail(error: Error) {
        report.eventContinuation.yield(.ended(id: id, at: report.currentDate(), outcome: .failed(error)))
    }
}
