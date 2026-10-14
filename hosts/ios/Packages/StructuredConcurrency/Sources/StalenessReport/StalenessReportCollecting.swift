import AsyncExtensions

public enum RegionOutcome: Sendable {
    case finished
    case failed(Error)
}

public protocol StalenessReportCollecting: AnyObject, Sendable {
    func startRegion(_ region: StallableRegion) -> any StalenessRegionHandling
}

/// A region that is currently open, and the collector for regions nested inside it.
public protocol StalenessRegionHandling: StalenessReportCollecting {
    /// Closes this region and every region still open underneath it. Idempotent.
    func end()
    /// Closes this region as failed. Idempotent.
    func fail(error: Error)
}

public enum StalenessDiagnostics {
    @TaskLocal public static var collector: any StalenessReportCollecting = NoOpStalenessCollector.shared
}

public final class NoOpStalenessCollector: StalenessRegionHandling {
    public static let shared = NoOpStalenessCollector()

    public func startRegion(_: StallableRegion) -> any StalenessRegionHandling { self }

    public func end() {}

    public func fail(error _: Error) {}
}

/// Runs `body` inside an already-opened `handle`, closing it on return, throw, or cancellation.
/// Rebinds the task-local so regions opened by callees nest under this one.
func withStallRegion<R>(
    handle: any StalenessRegionHandling,
    body: () async throws -> R
) async rethrows -> R {
    do {
        let result = try await StalenessDiagnostics.$collector.withValue(handle) {
            try await body()
        }
        handle.end()
        return result
    } catch {
        // Cancellation is not a failure: a user abandoning a flow must not raise a failure banner.
        if error is CancellationError {
            handle.end()
        } else {
            handle.fail(error: error)
        }
        throw error
    }
}

/// Opens a region around `body`, closing it on return, throw, or cancellation.
/// Regions opened by callees nest under this one.
public func markStallRegion<R>(
    _ title: String,
    body: () async throws -> R
) async rethrows -> R {
    let collector = StalenessDiagnostics.collector

    #if DEBUG
        if StalenessReport.isEnabled, collector is NoOpStalenessCollector {
            assertionFailure(
                "markStallRegion(\"\(title)\") has no enclosing activity and will be dropped — "
                    + "wrap the calling flow in markStallActivity"
            )
        }
    #endif

    return try await withStallRegion(
        handle: collector.startRegion(StallableRegion(title: title)),
        body: body
    )
}

/// Opens a root activity when staleness reporting is enabled, otherwise runs `body` untouched.
/// Package-level instrumentation should call this rather than `StalenessReport.shared` directly.
public func markStallActivity<R>(
    _ title: String,
    body: () async throws -> R
) async rethrows -> R {
    guard StalenessReport.isEnabled else {
        return try await body()
    }

    return try await StalenessReport.shared.trackActivity(title, body: body)
}
