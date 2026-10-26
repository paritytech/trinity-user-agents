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

    /// Whether this build links a reporting SDK; feature diagnostics are no-ops otherwise.
    public static var isReportingSupported: Bool {
        #if SENTRY_ENABLED
            true
        #else
            false
        #endif
    }

    /// Reports through the SDK `IssueMonitoringServiceProtocol.setup()` started; a no-op until then and in Release.
    public static func createReporter() -> IssueReporting {
        #if SENTRY_ENABLED
            SentryIssueReporter()
        #else
            NoopIssueReporter()
        #endif
    }
}
