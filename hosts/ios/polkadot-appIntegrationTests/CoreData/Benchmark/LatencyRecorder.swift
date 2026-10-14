import Foundation

struct LatencySummary: Codable, Sendable {
    let scenario: String
    let variant: String
    let count: Int
    let p50Ms: Double
    let p95Ms: Double
    let p99Ms: Double
    let maxMs: Double
    let wallMs: Double
    let opsPerSecond: Double
}

/// Collects durations for one scenario and variant; `summary()` reports percentiles, never means.
final class LatencyRecorder: @unchecked Sendable {
    let scenario: String
    let variant: StackVariant

    private let lock = NSLock()
    private var durations: [Duration] = []
    private var firstStart: ContinuousClock.Instant?
    private var lastEnd: ContinuousClock.Instant?

    init(scenario: String, variant: StackVariant) {
        self.scenario = scenario
        self.variant = variant
    }

    func time<T>(_ body: () async throws -> T) async rethrows -> T {
        let start = ContinuousClock.now
        defer { append(start: start, end: ContinuousClock.now) }
        return try await body()
    }

    func record(since start: ContinuousClock.Instant) {
        append(start: start, end: ContinuousClock.now)
    }

    func summary() -> LatencySummary {
        lock.lock()
        defer { lock.unlock() }

        let sorted = durations.map(Self.milliseconds).sorted()
        let wall = zip(firstStart, lastEnd).map { Self.milliseconds($1 - $0) } ?? 0
        let ops = wall > 0 ? Double(sorted.count) / (wall / 1_000) : 0

        return LatencySummary(
            scenario: scenario,
            variant: variant.rawValue,
            count: sorted.count,
            p50Ms: Self.percentile(sorted, 0.5),
            p95Ms: Self.percentile(sorted, 0.95),
            p99Ms: Self.percentile(sorted, 0.99),
            maxMs: sorted.last ?? 0,
            wallMs: wall,
            opsPerSecond: ops
        )
    }
}

private extension LatencyRecorder {
    func append(start: ContinuousClock.Instant, end: ContinuousClock.Instant) {
        lock.lock()
        defer { lock.unlock() }

        durations.append(end - start)
        firstStart = firstStart.map { min($0, start) } ?? start
        lastEnd = lastEnd.map { max($0, end) } ?? end
    }

    static func milliseconds(_ duration: Duration) -> Double {
        let components = duration.components
        return Double(components.seconds) * 1_000 + Double(components.attoseconds) / 1e15
    }

    static func percentile(_ sorted: [Double], _ quantile: Double) -> Double {
        guard !sorted.isEmpty else {
            return 0
        }

        let index = min(sorted.count - 1, Int(Double(sorted.count) * quantile))
        return sorted[index]
    }
}

private func zip<A, B>(_ first: A?, _ second: B?) -> (A, B)? {
    guard let first, let second else {
        return nil
    }

    return (first, second)
}
