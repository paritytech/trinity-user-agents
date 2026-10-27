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

    /// Log sink for the service `createService(dsn:)` starts, or nil when that service is a no-op.
    public static func createLogSink(dsn: String) -> HostLogSink? {
        #if SENTRY_ENABLED
            dsn.isEmpty ? nil : SentryHostLogSink()
        #else
            nil
        #endif
    }

    /// Builds the immutable product bootstrap only when this build links reporting support.
    public static func createNativeHostTelemetry(
        hostSessionID: UUID,
        truapiHostVersion: @autoclosure () -> String
    ) -> NativeHostTelemetry? {
        guard isReportingSupported else { return nil }

        return NativeHostTelemetry(
            hostSessionID: hostSessionID,
            origin: .current,
            truapiHostVersion: truapiHostVersion()
        )
    }

    /// Records typed native host lifecycle events; builds without reporting support receive a no-op recorder.
    public static func createHostEventRecorder() -> HostEventRecording {
        #if SENTRY_ENABLED
            SentryHostEventRecorder()
        #else
            NoopHostEventRecorder()
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
