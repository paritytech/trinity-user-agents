import Foundation

public enum IssueMonitoringFactory {
    /// Returns the reporting service the current build can support. Callers stay
    /// unconditional: when no SDK is linked, or no DSN was injected, the result is a no-op.
    public static func createService(dsn: String) -> IssueMonitoringServiceProtocol {
        #if SENTRY_ENABLED
            guard !dsn.isEmpty else {
                return NoopIssueMonitoringService()
            }

            return SentryIssueMonitoringService(dsn: dsn)
        #else
            return NoopIssueMonitoringService()
        #endif
    }
}
